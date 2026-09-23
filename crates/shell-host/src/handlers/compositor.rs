//! `wl_surface` events: frame callbacks (the frame machine's `Callback` input), the integer
//! scale fallback, and output enter/leave; plus the `wl_shm` accessor.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl CompositorHandler for HostState {
    /// The integer fallback only (`preferred_buffer_scale`): with fractional-scale bound, the
    /// `preferred_scale` event wins and this is ignored.
    fn scale_factor_changed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        todo!()
    }

    fn transform_changed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_transform: wl_output::Transform,
    ) {
        todo!()
    }

    /// Count the callback, then `FrameInput::Callback(doc animation)`.
    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        todo!()
    }

    fn surface_enter(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        todo!()
    }

    fn surface_leave(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        todo!()
    }
}

impl ShmHandler for HostState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
