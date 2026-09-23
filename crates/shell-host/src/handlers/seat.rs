//! Seats and capabilities: getting the pointer (with its cursor-shape device), the keyboard and
//! the text input, and the per-seat input state the other handlers share.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::time::Instant;

use blitz_traits::events::MouseEventButtons;
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat};
use wayland_client::{Connection, QueueHandle};

use crate::id::SurfaceId;
use crate::input::pointer::PointerPos;
use crate::state::HostState;

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
}

impl SeatHandler for HostState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        todo!()
    }

    /// Pointer: `get_pointer` plus the cursor-shape device. Keyboard: `get_keyboard` without
    /// sctk's repeat (the repeat timer is the context's, via `TimerRequest::Repeat*`), plus the
    /// text input and the data device.
    fn new_capability(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        todo!()
    }

    fn remove_capability(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        todo!()
    }

    fn remove_seat(&mut self, conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        todo!()
    }
}
