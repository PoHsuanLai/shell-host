//! Seats and capabilities: getting the pointer (with its cursor-shape device), the keyboard and
//! the text input, and the per-seat input state the other handlers share.

use std::time::{Duration, Instant};

use blitz_traits::events::MouseEventButtons;
use keyboard_types::Modifiers;
use smithay_client_toolkit::data_device_manager::data_device::DataDevice;
use smithay_client_toolkit::seat::keyboard::KeyEvent;
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat};
use wayland_client::{Connection, QueueHandle};

use crate::frame::{FrameInput, TimerCmd};
use crate::id::SurfaceId;
use crate::input::pointer::PointerPos;
use crate::state::HostState;
use crate::timers::TimerRequest;

/// The pointer and keyboard state of the (single, first) seat.
#[derive(Debug)]
pub struct SeatInput {
    pub(crate) seat: Option<wl_seat::WlSeat>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    /// The surface under the pointer and the serial of its `enter` (for cursor changes).
    pub(crate) pointer_focus: Option<(SurfaceId, u32)>,
    pub(crate) position: PointerPos,
    pub(crate) buttons: MouseEventButtons,
    pub(crate) keyboard_focus: Option<SurfaceId>,
    /// When the keyboard focus last arrived, for the first-mouse filter.
    pub(crate) focused_at: Option<Instant>,
    pub(crate) modifiers: Modifiers,
    /// The serial of the last press, for popup grabs, selections and activation tokens.
    pub(crate) last_serial: u32,
    /// The seat's data device, created alongside the keyboard (`new_capability`); the object
    /// `handlers::data_device`'s claim helper calls `set_selection` on. Held here (rather than
    /// just its raw `wl_data_device::WlDataDevice`) because `DataDevice::drop` releases the
    /// protocol object on a v2+ manager.
    pub(crate) data_device: Option<DataDevice>,
    /// The most recently pressed key, while it may still be auto-repeating (cleared on release
    /// or on `KeyboardMode`/focus loss); what `TimerRequest::RepeatStart`'s timer, once armed by
    /// `HostCtx`, re-synthesizes a `BlitzKeyEvent` from on every firing.
    pub(crate) repeating: Option<KeyEvent>,
    /// `wl_keyboard.repeat_info`'s delay and inter-repeat interval, as last reported; a
    /// reasonable value before the first report (xkbcommon/GTK's own default).
    pub(crate) repeat_delay: Duration,
    pub(crate) repeat_interval: Duration,
}

impl SeatHandler for HostState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        // `Clipboard::paste` needs a connection to flush; any seat event hands us one.
        if self.clipboard.conn.is_none() {
            self.clipboard.conn = Some(conn.clone());
        }
        // Only the first seat is tracked (`SeatInput`'s own doc: "the (single, first) seat").
        if self.input.seat.is_none() {
            self.input.seat = Some(seat);
        }
    }

    /// Pointer: `get_pointer` plus the cursor-shape device. Keyboard: `get_keyboard` without
    /// sctk's repeat (the repeat timer is the context's, via `TimerRequest::Repeat*`), plus the
    /// text input and the data device.
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        match capability {
            Capability::Pointer => {
                let Ok(pointer) = self.seat.get_pointer(qh, &seat) else {
                    return;
                };
                if let Some(manager) = &self.cursor.manager {
                    self.cursor.device = Some(manager.get_shape_device(&pointer, qh));
                }
                self.input.pointer = Some(pointer);
            }
            Capability::Keyboard => {
                if let Ok(keyboard) = self.seat.get_keyboard::<Self, Self>(qh, &seat, None) {
                    self.input.keyboard = Some(keyboard);
                }
                if let Some(text_input) = &mut self.text_input {
                    text_input.attach(&seat, qh);
                }
                if let Some(manager) = &self.data_device {
                    self.input.data_device = Some(manager.get_data_device(qh, &seat));
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        match capability {
            Capability::Pointer => {
                self.input.pointer = None;
                self.input.pointer_focus = None;
                self.input.position = PointerPos::OUTSIDE;
                self.input.buttons = MouseEventButtons::None;
                self.cursor.device = None;
            }
            Capability::Keyboard => {
                self.input.keyboard = None;
                self.input.keyboard_focus = None;
                self.input.focused_at = None;
                self.input.repeating = None;
                self.timers.push(TimerRequest::RepeatStop);
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        self.input.seat = None;
        self.input.pointer = None;
        self.input.keyboard = None;
        self.input.pointer_focus = None;
        self.input.position = PointerPos::OUTSIDE;
        self.input.buttons = MouseEventButtons::None;
        self.input.keyboard_focus = None;
        self.input.focused_at = None;
        self.input.modifiers = Modifiers::empty();
        self.input.data_device = None;
        self.input.repeating = None;
        self.cursor.device = None;
    }
}

/// Shared by every input handler in this wave (`pointer`, `keyboard`, `text_input`,
/// `data_device`): none of them has its own registry of surfaces, and each needs to turn a
/// `wl_surface` into the id it was spawned with, and to invalidate that surface's document after
/// an event changes it.
impl HostState {
    /// The id of the surface `surface` belongs to, or `None` for a surface that is not (or is no
    /// longer) one of ours.
    pub(crate) fn surface_id_for(&self, surface: &WlSurface) -> Option<SurfaceId> {
        self.surfaces
            .iter()
            .find(|(_, host_surface)| host_surface.role.wl_surface() == surface)
            .map(|(id, _)| *id)
    }

    /// Step `id`'s frame machine on `FrameInput::Invalidate` and queue the watchdog change it
    /// asks for, if any. Handlers cannot arm calloop timers themselves (F9): they record the
    /// request here and `HostCtx::flush` arms or cancels it after the dispatch that produced it.
    pub(crate) fn invalidate(&mut self, id: SurfaceId) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let action = surface.step(FrameInput::Invalidate);
        if !matches!(action.timer, TimerCmd::Keep) {
            self.timers.push(TimerRequest::Watchdog {
                id,
                cmd: action.timer,
            });
        }
    }
}
