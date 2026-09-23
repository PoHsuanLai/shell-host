//! Pointer frames -> document events, through `input::pointer`.

use blitz_traits::events::{BlitzWheelEvent, Point, PointerCoords, UiEvent};
use cursor_icon::CursorIcon;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::{Connection, QueueHandle};

use crate::id::SurfaceId;
use crate::input::pointer::{self, ButtonChange, FirstMouse, PointerPos};
use crate::role::Role;
use crate::state::HostState;

impl PointerHandler for HostState {
    /// Enter: focus and cursor. Leave: `clear_hover()` then `input::pointer::leave_event`.
    /// Motion, press (move-before-down, first-mouse filter), release, axis
    /// (`input::pointer::wheel_delta`). Each changed document is invalidated.
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
                    self.input.position = pos;
                    // A fresh `enter` invalidates any earlier serial, even for the same shape:
                    // force `Cursor::set` past its no-op-when-unchanged check.
                    self.cursor.current = None;
                    self.cursor.set(pointer, serial, Some(CursorIcon::Default));
                }
                PointerEventKind::Leave { serial: _ } => {
                    if self.input.pointer_focus.map(|(focus, _)| focus) == Some(id) {
                        self.input.pointer_focus = None;
                    }
                    self.input.position = PointerPos::OUTSIDE;
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.document.clear_hover();
                        surface
                            .document
                            .handle_event(pointer::leave_event(self.input.modifiers));
                    }
                    self.invalidate(id);
                }
                PointerEventKind::Motion { time: _ } => {
                    self.input.position = pos;
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.document.handle_event(pointer::move_event(
                            pos,
                            self.input.buttons,
                            self.input.modifiers,
                        ));
                    }
                    self.invalidate(id);
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
                    self.input.buttons =
                        pointer::buttons_after(held_before, btn, ButtonChange::Pressed);
                    if self.first_mouse(id) == FirstMouse::Swallow {
                        // The press still counts as having happened (buttons updated above), it
                        // just never reaches the document: this is the click that activates the
                        // window, not a click on anything in it.
                        continue;
                    }
                    let events = pointer::press_events(pos, btn, held_before, self.input.modifiers);
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        for event in events {
                            surface.document.handle_event(event);
                        }
                    }
                    self.invalidate(id);
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
                    let event = pointer::release_event(pos, btn, held_before, self.input.modifiers);
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.document.handle_event(event);
                    }
                    self.invalidate(id);
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
                    let (x, y) = (pos.x as f32, pos.y as f32);
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
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.document.handle_event(UiEvent::Wheel(wheel));
                    }
                    self.invalidate(id);
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
