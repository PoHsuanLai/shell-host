//! Outputs appearing, changing and going: `HostEvent::Output*`.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::output::{OutputHandler, OutputState};
use wayland_client::protocol::wl_output;
use wayland_client::{Connection, QueueHandle};

use crate::output::OutputInfo;
use crate::state::HostState;

impl OutputHandler for HostState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        todo!()
    }

    fn update_output(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        todo!()
    }

    fn output_destroyed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        todo!()
    }
}

/// The host's view of an sctk output, by its registry name. Pure.
pub fn output_info(name: u32, info: &smithay_client_toolkit::output::OutputInfo) -> OutputInfo {
    todo!()
}
