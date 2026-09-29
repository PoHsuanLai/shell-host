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

use crate::csd::Zone;
use crate::frame::{FrameInput, TimerCmd};
use crate::grab::GrabRecord;
use crate::id::SurfaceId;
use crate::input::capability::{Announcement, Held, capability_change};
use crate::input::focus::PointerFocus;
use crate::input::pointer::PointerPos;
use crate::live::Live;
use crate::modality::{self, InputModality, Stroke};
use crate::modifiers::Modifiers as HeldModifiers;
use crate::state::HostState;
use crate::timers::TimerRequest;

/// The pointer and keyboard state of the (single, first) seat.
#[derive(Debug)]
pub struct SeatInput {
    pub(crate) seat: Option<wl_seat::WlSeat>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    /// The surface under the pointer and the serial of its `enter` (for cursor changes).
    pub(crate) pointer_focus: PointerFocus,
    pub(crate) position: PointerPos,
    pub(crate) buttons: MouseEventButtons,
    pub(crate) keyboard_focus: Option<SurfaceId>,
    /// When the keyboard focus last arrived, for the first-mouse filter.
    pub(crate) focused_at: Option<Instant>,
    pub(crate) modifiers: Modifiers,
    /// The seat's modifiers in `use_modifiers`' typed, no-`bool` shape: kept
    /// alongside `modifiers` because `wl_keyboard.modifiers` may arrive with no surface of ours
    /// focused (the protocol's own escape hatch for tying modifiers to pointer focus instead),
    /// so a surface that only just gained keyboard focus can be handed the seat's
    /// already-current state rather than starting from nothing.
    pub(crate) held_modifiers: HeldModifiers,
    /// Pointer or keyboard, whichever last drove the seat (`modality`): one value
    /// for the seat, shared by every surface's `SurfaceHandle` (`use_input_modality`).
    pub(crate) modality: Live<InputModality>,
    /// The serial of the last press, for popup grabs, selections and activation tokens.
    pub(crate) last_serial: u32,
    /// The pointer's surface and held press, for the move/resize serial rule; shared with
    /// every `SurfaceHandle` so `begin_move()` can refuse synchronously.
    pub(crate) grab: GrabRecord,
    /// Whether the pointer is on the focused surface's document or in its resize band. Only
    /// re-decided while no button is held: a drag that started on the document stays the
    /// document's when it crosses the band (the implicit grab).
    pub(crate) zone: Zone,
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
        self.adopt_seat(conn, seat);
    }

    /// Pointer: `get_pointer` plus the cursor-shape device. Keyboard: `get_keyboard`, the text
    /// input and the data device (`handlers::devices`). Only when no object for the capability
    /// is held: a re-announced capability reuses the one it has.
    fn new_capability(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // sctk calls `new_seat` only for a seat announced after startup; a seat that was
        // already there when the host bound the globals (every seat, in practice) first shows
        // up here, with its capabilities.
        self.adopt_seat(conn, seat.clone());
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        self.announce(qh, &seat, capability, Announcement::Added);
    }

    /// Release the capability's object (`wl_pointer.release`/`wl_keyboard.release`): dropping
    /// it would leave it live on the compositor's side, delivering every press again once a
    /// new one exists.
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        self.announce(qh, &seat, capability, Announcement::Removed);
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        self.release_pointer();
        self.release_keyboard();
        self.input.seat = None;
        self.input.data_device = None;
    }
}

impl HostState {
    /// Decide what `announcement` of `capability` means for the object held
    /// (`input::capability`) and act on it. Touch is not used.
    fn announce(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
        capability: Capability,
        announcement: Announcement,
    ) {
        match capability {
            Capability::Pointer => {
                let change = capability_change(announcement, Held::of(&self.input.pointer));
                self.change_pointer(change, qh, seat);
            }
            Capability::Keyboard => {
                let change = capability_change(announcement, Held::of(&self.input.keyboard));
                self.change_keyboard(change, qh, seat);
            }
            _ => {}
        }
    }
}

/// Shared by every input handler (`pointer`, `keyboard`, `text_input`,
/// `data_device`): none of them has its own registry of surfaces, and each needs to turn a
/// `wl_surface` into the id it was spawned with, and to invalidate that surface's document after
/// an event changes it.
impl HostState {
    /// Track `seat` if no seat is tracked yet (only the first seat is, `SeatInput`'s own doc),
    /// and give the clipboard the connection it flushes on.
    fn adopt_seat(&mut self, conn: &Connection, seat: wl_seat::WlSeat) {
        if self.clipboard.conn.is_none() {
            self.clipboard.conn = Some(conn.clone());
        }
        if self.input.seat.is_none() {
            self.input.seat = Some(seat);
        }
    }

    /// The id of the surface `surface` belongs to, or `None` for a surface that is not (or is no
    /// longer) one of ours.
    pub(crate) fn surface_id_for(&self, surface: &WlSurface) -> Option<SurfaceId> {
        self.surfaces
            .iter()
            .find(|(_, host_surface)| host_surface.role.wl_surface() == surface)
            .map(|(id, _)| *id)
    }

    /// Step `id`'s frame machine on `FrameInput::Invalidate` and queue the watchdog change it
    /// asks for, if any. Handlers cannot arm calloop timers themselves: they record the
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

impl HostState {
    /// The seat saw `stroke`: publish the modality it leaves (`modality::after`) to every
    /// surface; an unchanged modality wakes nobody (`Live::publish`).
    pub(crate) fn note_stroke(&mut self, stroke: Stroke) {
        let next = modality::after(self.input.modality.get(), stroke);
        let _ = self.input.modality.publish(next);
    }
}
