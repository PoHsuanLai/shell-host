//! Layer-surface configure and close.

mod decide;

pub use decide::configured_size;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, QueueHandle};

use crate::event::CloseReason;
use crate::frame::FrameInput;
use crate::handlers::{close_surface_and_popups, follow_buffer_scale, surface_id_by_wl};
use crate::id::SurfaceId;
use crate::role::Role;
use crate::scale::{self, Scale, Size};
use crate::state::HostState;
use crate::surface::Configured;
use crate::timers::TimerRequest;

impl LayerShellHandler for HostState {
    /// `HostEvent::SurfaceClosed{reason: Compositor}`, closing its popups first.
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let Some(id) = surface_id_by_wl(self, layer.wl_surface()) else {
            return;
        };
        close_surface_and_popups(self, id, CloseReason::Compositor);
    }

    /// Record the size (0 on an axis keeps the requested size), mark configured, resize
    /// the presenter, invalidate (the render that follows sends the regions).
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(id) = surface_id_by_wl(self, layer.wl_surface()) else {
            return;
        };

        let Some((logical, surface_scale)) = record_configure(self, id, &configure) else {
            return;
        };
        follow_configure(self, id, layer.wl_surface(), logical, surface_scale);
    }
}

/// After a configure recorded `logical` on surface `id` (at `surface_scale`): the viewport
/// destination (or the integer buffer scale), the presenter's size, and an invalidation whose
/// render sends the regions. Shared by layer and lock surfaces.
pub(crate) fn follow_configure(
    state: &mut HostState,
    id: SurfaceId,
    wl_surface: &WlSurface,
    logical: Size,
    surface_scale: Scale,
) {
    let buffer = scale::buffer_size(logical, surface_scale);
    follow_buffer_scale(state, wl_surface, logical, surface_scale);

    if let Some(host_surface) = state.surfaces.get_mut(&id)
        && let Some(presenter) = &mut host_surface.presenter
    {
        presenter.resize(state.gpu.as_ref(), buffer);
    }

    // Regions are not sent here: the invalidate below renders, and the render resolves
    // them after layout and sends whichever changed (`ctx::paint::send_regions`).
    if let Some(host_surface) = state.surfaces.get_mut(&id) {
        let action = host_surface.step(FrameInput::Invalidate);
        state.timers.push(TimerRequest::Watchdog {
            id,
            cmd: action.timer,
        });
    }
}

/// Records the configured logical size on `id`'s surface and marks it configured; returns its
/// logical size and scale (for the caller's buffer/viewport arithmetic), or `None` if `id` no
/// longer names a live layer surface.
fn record_configure(
    state: &mut HostState,
    id: SurfaceId,
    configure: &LayerSurfaceConfigure,
) -> Option<(Size, Scale)> {
    let host_surface = state.surfaces.get_mut(&id)?;
    let Role::Layer { config, .. } = &host_surface.role else {
        return None;
    };
    let logical = configured_size(config.size, configure.new_size);
    host_surface.configure(logical);
    host_surface.configured = Configured::Done;
    Some((logical, host_surface.scale))
}
