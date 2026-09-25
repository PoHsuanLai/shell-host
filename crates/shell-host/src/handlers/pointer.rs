//! Pointer frames -> document events, through `input::pointer`.

use blitz_traits::events::{
    BlitzWheelEvent, MouseEventButton, MouseEventButtons, Point, PointerCoords, UiEvent,
};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::{Connection, QueueHandle};

use crate::csd::Zone;
use crate::grab::Press;
use crate::id::SurfaceId;
use crate::input::pointer::{self, ButtonChange, FirstMouse, PointerPos};
use crate::role::Role;
use crate::state::HostState;

impl PointerHandler for HostState {
    /// Enter: focus, zone and cursor. Leave: `SurfaceDocument::pointer_left` (the leave move,
    /// then `clear_hover()`).
    /// Motion, press (move-before-down, first-mouse filter), release, axis
    /// (`input::pointer::wheel_delta`), in the document's coordinates; in a client-decorated
    /// window's resize band (`csd`) the host shows the resize cursor and a press starts the
    /// resize, and the document sees neither. Each changed document is invalidated. Every
    /// press and release updates the seat's grab record (the move/resize serial rule).
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(id) = self.surface_id_for(&event.surface) else {
                continue;
            };
            let pos = PointerPos {
                x: event.position.0,
                y: event.position.1,
            };
            // `PointerEventKind` is `Clone` but not `Copy`; `event` is only borrowed.
            match event.kind.clone() {
                PointerEventKind::Enter { serial } => {
                    self.input.pointer_focus = Some((id, serial));
                    self.forget_buttons();
                    self.input.grab.update(|facts| facts.focus = Some(id));
                    self.input.position = pos;
                    self.input.zone = self.zone_of(id, pos);
                    // A fresh `enter` invalidates any earlier serial, even for the same shape:
                    // force `Cursor::set` past its no-op-when-unchanged check.
                    self.cursor.current = None;
                    self.show_cursor(pointer, id);
                }
                PointerEventKind::Leave { serial: _ } => {
                    if self.input.pointer_focus.map(|(focus, _)| focus) == Some(id) {
                        self.input.pointer_focus = None;
                    }
                    self.input.grab.update(|facts| {
                        if facts.focus == Some(id) {
                            facts.focus = None;
                        }
                    });
                    self.input.zone = Zone::Document;
                    self.forget_buttons();
                    self.input.position = PointerPos::OUTSIDE;
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.document.pointer_left(self.input.modifiers);
                    }
                    self.invalidate(id);
                }
                PointerEventKind::Motion { time: _ } => {
                    self.input.position = pos;
                    self.pointer_moved(id, pos);
                    self.show_cursor(pointer, id);
                }
                PointerEventKind::Press {
                    button,
                    serial,
                    time: _,
                } => {
                    self.input.last_serial = serial;
                    let Some(btn) = pointer::button_from_evdev(button) else {
                        continue;
                    };
                    self.input.position = pos;
                    let held_before = self.input.buttons;
                    if held_before == MouseEventButtons::None {
                        self.input.zone = self.zone_of(id, pos);
                    }
                    self.input.buttons =
                        pointer::buttons_after(held_before, btn, ButtonChange::Pressed);
                    self.input.grab.update(|facts| {
                        facts.press = Some(Press {
                            surface: id,
                            serial,
                        });
                    });
                    if let Zone::Edge(edge) = self.input.zone {
                        // The band is the host's: the press starts the resize, whether or not
                        // the window is active, and the document never hears of it.
                        if btn == MouseEventButton::Main {
                            self.resize_from_band(id, edge);
                        }
                        continue;
                    }
                    if self.first_mouse(id) == FirstMouse::Swallow {
                        // The press still counts as having happened (buttons updated above), it
                        // just never reaches the document: this is the click that activates the
                        // window, not a click on anything in it.
                        continue;
                    }
                    let at = self.document_pos(id, pos);
                    let events = pointer::press_events(at, btn, held_before, self.input.modifiers);
                    self.deliver(id, events);
                }
                PointerEventKind::Release {
                    button,
                    serial: _,
                    time: _,
                } => {
                    let Some(btn) = pointer::button_from_evdev(button) else {
                        continue;
                    };
                    self.input.position = pos;
                    let held_before = self.input.buttons;
                    self.input.buttons =
                        pointer::buttons_after(held_before, btn, ButtonChange::Released);
                    if self.input.buttons == MouseEventButtons::None {
                        self.input.grab.update(|facts| facts.press = None);
                    }
                    if self.input.zone == Zone::Document {
                        let at = self.document_pos(id, pos);
                        let event =
                            pointer::release_event(at, btn, held_before, self.input.modifiers);
                        self.deliver(id, [event]);
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    time: _,
                    source: _,
                } => {
                    let Some(delta) = pointer::wheel_delta(horizontal, vertical) else {
                        continue;
                    };
                    if self.input.zone != Zone::Document {
                        continue;
                    }
                    let at = self.document_pos(id, pos);
                    let (x, y) = (at.x as f32, at.y as f32);
                    let wheel = BlitzWheelEvent {
                        delta,
                        coords: PointerCoords {
                            page_x: x,
                            page_y: y,
                            screen_x: x,
                            screen_y: y,
                            client_x: x,
                            client_y: y,
                        },
                        buttons: self.input.buttons,
                        mods: self.input.modifiers,
                        element: Point::default(),
                    };
                    self.deliver(id, [UiEvent::Wheel(wheel)]);
                }
            }
        }
    }
}

impl HostState {
    /// Whether a press on surface `id`, right now, is the click that activates it
    /// (design/13 §13.3.8): only for a toplevel, only when it is the one surface that currently
    /// has keyboard focus, and only within the first-mouse window (`HostConfig::first_mouse_window_ms`,
    /// the settings key `menus.first_mouse_window_ms`) of that focus arriving.
    fn first_mouse(&self, id: SurfaceId) -> FirstMouse {
        let is_toplevel = self
            .surfaces
            .get(&id)
            .is_some_and(|surface| matches!(surface.role, Role::Toplevel { .. }));
        if !is_toplevel || self.input.keyboard_focus != Some(id) {
            return FirstMouse::Deliver;
        }
        let since_activation = self.input.focused_at.map(|at| at.elapsed());
        pointer::first_mouse(since_activation, self.first_mouse_window)
    }
}
