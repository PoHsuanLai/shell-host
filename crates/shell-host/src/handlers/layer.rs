//! Layer-surface configure and close.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl LayerShellHandler for HostState {
    /// `HostEvent::SurfaceClosed{reason: Compositor}`, closing its popups first.
    fn closed(&mut self, conn: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface) {
        todo!()
    }

    /// Record the size (0 on an axis keeps the requested size), mark configured, create the
    /// presenter on first configure, apply regions, invalidate.
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        serial: u32,
    ) {
        todo!()
    }
}
