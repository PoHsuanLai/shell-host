//! Pointer frames -> document events, through `input::pointer`.

mod decide;

use blitz_traits::events::MouseEventButtons;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::{Connection, QueueHandle};

use crate::csd::Zone;
use crate::dom::document::FirstMouseOptIn;
use crate::grab::Press;
use crate::id::SurfaceId;
use crate::input::focus::{self, Departure, PointerFocus};
use crate::input::pointer::{self, ButtonChange, FirstMouse, PointerPos};
use crate::modality::Stroke;
use crate::state::HostState;
use decide::{Delivery, Focused, PressRoute};
use palmrest_protocol::Over;

impl PointerHandler for HostState {
    /// Enter: focus, zone, the move to its position (so `:hover` and `mouseenter` hold from the
    /// first frame) and cursor. Leave: `SurfaceDocument::pointer_left` (the leave move,
    /// then `clear_hover()`).
    /// Motion, press (move-before-down, first-mouse filter), release, axis (the scroll engine,
    /// `input::scroll`), in the document's coordinates; in a client-decorated
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
            // A surface that is no longer ours: the compositor's leave (or a straggling
            // motion) for a wl_surface the host unmapped or closed, whose stay `withdraw_pointer`
            // already ended.
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
                    self.input.pointer_focus = focus::enter(id, serial);
                    self.forget_buttons();
                    self.input.grab.update(|facts| facts.focus = Some(id));
                    self.input.position = pos;
                    self.input.zone = self.zone_of(id, pos);
                    // The enter carries a position and no motion follows it: a pointer that
                    // reaches the surface in one motion, or a surface mapped under a still
                    // pointer, would hover nothing until the next motion. Deliver the move
                    // here, as a press is preceded by one (`input::pointer`); it also gives
                    // the document's cursor to the `show_cursor` below. Some compositors send
                    // no enter at all to a surface mapped under a still pointer (cosmic-comp
                    // #2230); there the press's own move still covers the click.
                    self.pointer_moved(id, pos);
                    // A fresh `enter` invalidates any earlier serial, even for the same shape:
                    // force `Cursor::set` past its no-op-when-unchanged check.
                    self.cursor.current = None;
                    self.show_cursor(pointer, id);
                    self.palmrest.pointer_over(Over::Ours);
                }
                PointerEventKind::Leave { serial: _ } => {
                    // A leave the compositor sends is always a leave for the document, even
                    // when the record already moved on (an enter elsewhere seen first).
                    let _ = self.depart(id);
                    self.pointer_off(id);
                    self.invalidate(id);
                    if self.input.pointer_focus == PointerFocus::Nowhere {
                        self.palmrest.pointer_over(Over::NotOurs);
                    }
                }
                PointerEventKind::Motion { time: _ } => {
                    self.input.position = pos;
                    self.pointer_moved(id, pos);
                    let at = self.document_pos(id, pos);
                    self.scroll_pointer(id, at);
                    self.show_cursor(pointer, id);
                }
                PointerEventKind::Press {
                    button,
                    serial,
                    time: _,
                } => {
                    self.input.last_serial = serial;
                    // Any press, the band's and a swallowed first mouse's too: the person
                    // reached for the pointer.
                    self.note_stroke(Stroke::PointerPress);
                    let Some(btn) = pointer::button_from_evdev(button) else {
                        continue;
                    };
                    self.input.position = pos;
                    let held_before = self.input.buttons;
                    self.input.zone = self.zone_while_held(id, pos);
                    self.input.buttons =
                        pointer::buttons_after(held_before, btn, ButtonChange::Pressed);
                    self.input.grab.update(|facts| {
                        facts.press = Some(Press {
                            surface: id,
                            serial,
                        });
                    });
                    let first = self.first_mouse(id);
                    let opt_in = self.first_mouse_opt_in(id, first, pos);
                    let route = decide::press_route(self.input.zone, btn, first, opt_in);
                    match route {
                        // The band is the host's: the press starts the resize, whether or not
                        // the window is active, and the document never hears of it.
                        PressRoute::Resize(edge) => self.resize_from_band(id, edge),
                        // Absorbed: the band's. Swallowed: this is the click that activates the
                        // window, not a click on anything in it; the press still counts as
                        // having happened (buttons updated above).
                        PressRoute::Absorbed | PressRoute::Swallowed => {}
                        PressRoute::Deliver => {
                            let at = self.document_pos(id, pos);
                            let events =
                                pointer::press_events(at, btn, held_before, self.input.modifiers);
                            self.deliver(id, events);
                        }
                    }
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
                    if decide::delivery(self.input.zone) == Delivery::ToDocument {
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
                    source,
                } => {
                    if decide::delivery(self.input.zone) == Delivery::Withheld {
                        continue;
                    }
                    // The engine's, never Blitz's default scroll action (design/11 §11.3.1),
                    // except for a `data-wheel="capture"` element (`input::scroll`).
                    let at = self.document_pos(id, pos);
                    self.scroll_axis(id, (horizontal, vertical), source, at);
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
        let role = self.surfaces.get(&id).map(|surface| surface.role.kind());
        let focus = match self.input.keyboard_focus {
            Some(focused) if focused == id => Focused::Yes,
            Some(_) | None => Focused::No,
        };
        let since_activation = self.input.focused_at.map(|at| at.elapsed());
        decide::first_mouse_verdict(role, focus, since_activation, self.first_mouse_window)
    }

    /// Whether the element under `pos` on surface `id` opted into the activating click
    /// (`data-first-mouse`). Asked of the document only when the click would be swallowed.
    fn first_mouse_opt_in(
        &self,
        id: SurfaceId,
        first: FirstMouse,
        pos: PointerPos,
    ) -> FirstMouseOptIn {
        match (first, self.surfaces.get(&id)) {
            (FirstMouse::Swallow, Some(surface)) => {
                let at = self.document_pos(id, pos);
                surface.document.wants_first_mouse(at.x, at.y)
            }
            _ => FirstMouseOptIn::Default,
        }
    }

    /// Surface `id` is being unmapped or closed by the host. If the pointer is on it, end the
    /// stay now, while the surface is still known: the compositor's leave will name a
    /// `wl_surface` the host no longer has, and a document left hovering keeps
    /// its hover chain into the next mapping, so the element under the pointer then never gets
    /// a `mouseenter`. No invalidation: the surface is going, and a remapped one starts a new
    /// frame machine.
    pub(crate) fn withdraw_pointer(&mut self, id: SurfaceId) {
        if self.depart(id) == Departure::Lost {
            self.pointer_off(id);
        }
    }

    /// Update the focus record and the grab's copy of it for surface `id` departing.
    fn depart(&mut self, id: SurfaceId) -> Departure {
        let (after, departure) = focus::depart(self.input.pointer_focus, id);
        self.input.pointer_focus = after;
        self.input.grab.update(|facts| {
            if facts.focus == Some(id) {
                facts.focus = None;
            }
        });
        departure
    }

    /// The pointer is off surface `id`: reset its zone, buttons and position, and tell the
    /// document it left (`SurfaceDocument::pointer_left`).
    fn pointer_off(&mut self, id: SurfaceId) {
        self.input.zone = Zone::Document;
        self.forget_buttons();
        self.input.position = PointerPos::OUTSIDE;
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.document.pointer_left(self.input.modifiers);
        }
    }
}
