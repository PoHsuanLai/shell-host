//! `wl_surface` events: frame callbacks (the frame machine's `Callback` input), the integer
//! scale fallback, and output enter/leave; plus the `wl_shm` accessor.

use std::time::Instant;

use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::frame::FrameInput;
use crate::handlers::surface_id_by_wl;
use crate::scale::Scale120;
use crate::state::HostState;
use crate::timers::TimerRequest;

impl CompositorHandler for HostState {
    /// The integer fallback only (`preferred_buffer_scale`): with fractional-scale bound, the
    /// `preferred_scale` event wins and this is ignored.
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if self.fractional.is_some() {
            return;
        }
        let Some(id) = surface_id_by_wl(self, surface) else {
            return;
        };
        let Some(host_surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        host_surface.rescale(Scale120::from_integer_factor(new_factor));
        let action = host_surface.step(FrameInput::Invalidate);
        self.timers.push(TimerRequest::Watchdog {
            id,
            cmd: action.timer,
        });
    }

    /// No surface transform support: every surface stays untransformed (the host has no
    /// rotation feature), so there is nothing to record.
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    /// Count the callback, tick the document if it is animating or driven (`tick`), then
    /// `FrameInput::Callback` with that pacing.
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        let Some(id) = surface_id_by_wl(self, surface) else {
            return;
        };
        let Some(host_surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        host_surface.stats.callback();
        let animation = host_surface.frame_due(self.clock.instant_at(Instant::now()));
        let action = host_surface.step(FrameInput::Callback(animation));
        self.timers.push(TimerRequest::Watchdog {
            id,
            cmd: action.timer,
        });
    }

    /// Recorded for the surface it belongs to (`HostSurface` has no per-output membership field
    /// today, so there is nowhere further to route it); nothing currently reads it back, since
    /// output-affinity behaviour (choosing the output for `OutputTarget::CompositorPicks`,
    /// tuning the watchdog to the entering output's refresh rate) is not yet implemented.
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl ShmHandler for HostState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
