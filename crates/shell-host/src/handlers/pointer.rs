//! Pointer frames -> document events, through `input::pointer`.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerHandler};
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl PointerHandler for HostState {
    /// Enter: focus and cursor. Leave: `clear_hover()` then `input::pointer::leave_event`.
    /// Motion, press (move-before-down, first-mouse filter), release, axis
    /// (`input::pointer::wheel_delta`). Each changed document is invalidated.
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        todo!()
    }
}
