//! `HostCtx`: what the shell's event closure uses to manage surfaces. It is the event loop's
//! data: the Wayland state plus the loop-level pieces that depend on the app message type.

mod drag;
mod drag_icon;
pub mod flush;
mod lock;
mod ops;
mod paint;
mod palmrest;
mod rescale;
mod spawn;
mod timer;
mod window;

#[cfg(feature = "debug-inject")]
pub(crate) use paint::capture;
pub(crate) use window::begin_resize;

use std::collections::{BTreeMap, VecDeque};

use blitz_traits::shell::ColorScheme;
use calloop::{LoopHandle, RegistrationToken};
use tokio::sync::oneshot;

use crate::config::Backend;
use crate::error::Result;
use crate::frame::FrameInput;
use crate::handlers::activation::ActivationRequest;
use crate::handlers::output::output_info;
use crate::id::SurfaceId;
use crate::id::SurfaceName;
use crate::layer::LayerConfig;
use crate::msg::{SurfaceOp, Visibility};
use crate::output::OutputInfo;
use crate::root::HostRoot;
use crate::state::HostState;
use crate::stats::FrameStats;
use crate::toplevel::ToplevelConfig;
use blitz_kit::adapter::AdapterPref;

/// Whether `Host::run` should return after this iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    Continue,
    Exit,
}

/// The host, as the shell's event closure sees it.
pub struct HostCtx<M> {
    pub(crate) state: HostState,
    /// `HostMsg::App` messages waiting for the closure.
    pub(crate) app: VecDeque<M>,
    pub(crate) handle: LoopHandle<'static, HostCtx<M>>,
    /// Armed watchdog timers by surface, and the key-repeat timer.
    pub(crate) watchdogs: BTreeMap<SurfaceId, RegistrationToken>,
    /// Pending scroll-thumb wakes by surface (`TimerRequest::ScrollWake`).
    pub(crate) scroll_wakes: BTreeMap<SurfaceId, RegistrationToken>,
    pub(crate) repeat: Option<RegistrationToken>,
    pub(crate) running: Running,
    /// The backend in force: `Gpu` until no adapter survives validation, then `Cpu`.
    pub(crate) backend: Backend,
    pub(crate) adapter: AdapterPref,
    /// The wgpu instance every surface and the one device come from, made on the first surface.
    pub(crate) instance: Option<wgpu::Instance>,
}

impl<M: 'static> HostCtx<M> {
    /// The outputs as of now.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        let outputs = &self.state.outputs;
        outputs
            .outputs()
            .filter_map(|output| outputs.info(&output))
            .map(|info| output_info(info.id, &info))
            .collect()
    }

    /// Create a layer surface rendering `root`. `HostError::Unsupported` without layer-shell.
    pub fn spawn_layer(&mut self, config: LayerConfig, root: HostRoot) -> Result<SurfaceId> {
        let id = self.state.ids.next();
        spawn::layer(&mut self.state, id, config, root)?;
        self.presenter_or_close(id)?;
        Ok(id)
    }

    /// Create an xdg toplevel rendering `root`.
    pub fn spawn_toplevel(&mut self, config: ToplevelConfig, root: HostRoot) -> Result<SurfaceId> {
        let id = self.state.ids.next();
        spawn::toplevel(&mut self.state, id, config, root);
        self.presenter_or_close(id)?;
        Ok(id)
    }

    /// Give a new surface its presenter at once (the configure handlers only resize it); a
    /// surface that cannot have one is removed again, silently, since its id was never
    /// handed out.
    pub(crate) fn presenter_or_close(&mut self, id: SurfaceId) -> Result<()> {
        paint::ensure_presenter(self, id).inspect_err(|_| {
            ops::close(self, id, crate::event::CloseReason::Failed);
            self.state.events.retain(|event| {
                !matches!(event, crate::event::HostEvent::SurfaceClosed { id: gone, .. } if *gone == id)
            });
        })
    }

    /// Close a surface and its popups; `HostEvent::SurfaceClosed{reason: Requested}` follows.
    pub fn close(&mut self, id: SurfaceId) {
        ops::close(self, id, crate::event::CloseReason::Requested);
    }

    /// Hide or show a surface. Hidden keeps its `VirtualDom` warm.
    pub fn set_visibility(&mut self, id: SurfaceId, visibility: Visibility) {
        ops::set_visibility(self, id, visibility);
    }

    /// Apply `op` to surface `id` before its next paint.
    pub fn apply(&mut self, id: SurfaceId, op: SurfaceOp) {
        self.state
            .requests
            .push(crate::msg::Request::Apply { id, op });
    }

    /// Request an xdg-activation token on behalf of surface `id`, for launching `app_id`. The
    /// receiver yields `None` without xdg-activation or when refused.
    pub fn activation_token(
        &mut self,
        id: SurfaceId,
        app_id: Option<String>,
    ) -> oneshot::Receiver<Option<String>> {
        let (reply, answer) = oneshot::channel();
        request_token(&mut self.state, id, app_id, reply);
        answer
    }

    /// Surface `id`'s frame statistics: cumulative counters, and percentiles over the samples
    /// since the last [`HostCtx::take_stats`]; `None` for an unknown id. The one source of
    /// frame statistics: the `debug-inject` stdin `stats` command prints `take_stats`.
    pub fn stats(&self, id: SurfaceId) -> Option<FrameStats> {
        let adapter = self.state.adapter_label();
        self.state
            .surfaces
            .get(&id)
            .map(|surface| surface.report(&adapter))
    }

    /// [`HostCtx::stats`], then start a new sample window, so the next report covers only the
    /// time since this one (counters stay cumulative).
    pub fn take_stats(&mut self, id: SurfaceId) -> Option<FrameStats> {
        let stats = self.stats(id)?;
        if let Some(surface) = self.state.surfaces.get_mut(&id) {
            surface.stats.drain();
        }
        Some(stats)
    }

    /// The surface a debug command would address as `name` (the layer namespace, the toplevel
    /// app id, or `<parent>/popup`); the first match when several share a name.
    pub fn surface_named(&self, name: &SurfaceName) -> Option<SurfaceId> {
        self.state
            .surfaces
            .values()
            .find(|surface| surface.name == *name)
            .map(|surface| surface.id)
    }

    /// Tune every surface's scrolling (design/22 §3.7 `scroll`, §3.8 `scrollbar`): a shell
    /// reads its settings and calls this at start and on every change. Nothing animates from
    /// the change; the next scroll uses it.
    pub fn set_scroll_config(&mut self, config: crate::scroll::config::ScrollConfig) {
        self.state
            .set_scroll_tuning(crate::scroll::tuning::Tuning::new(config));
    }

    /// Set every document's `prefers-color-scheme`.
    pub fn set_color_scheme(&mut self, scheme: ColorScheme) {
        self.state.color_scheme = scheme;
        // Forgetting the applied geometry makes the next flush set every viewport again, with
        // the new scheme, and repaint.
        let ids: Vec<SurfaceId> = self.state.surfaces.keys().copied().collect();
        for id in ids {
            if let Some(surface) = self.state.surfaces.get_mut(&id) {
                surface.geometry = None;
            }
            self.state.frame_input(id, FrameInput::Invalidate);
        }
    }

    /// Close every surface and return from `Host::run` after this event.
    pub fn exit(&mut self) {
        ops::close_all(self);
        self.running = Running::Exit;
    }

    /// After every dispatch: poll woken documents, drain head queues, apply queued requests,
    /// step every surface's frame machine (render the dirty ones), arm or cancel the timers the
    /// handlers asked for, then flush the connection. Stops with `HostError::Protocol` once
    /// the connection has a protocol error.
    pub(crate) fn flush(&mut self) -> Result<Running> {
        flush::run(self)?;
        Ok(self.running)
    }
}

/// Ask the compositor for an activation token for surface `id`; `None` at once without
/// xdg-activation or for an unknown surface.
pub(crate) fn request_token(
    state: &mut HostState,
    id: SurfaceId,
    app_id: Option<String>,
    reply: oneshot::Sender<Option<String>>,
) {
    let surface = state
        .surfaces
        .get(&id)
        .map(|surface| surface.role.wl_surface().clone());
    let (Some(activation), Some(surface)) = (&state.activation, surface) else {
        // The receiver may be gone already; nobody is left to tell.
        let _ = reply.send(None);
        return;
    };
    let seat_and_serial = state
        .input
        .seat
        .clone()
        .map(|seat| (seat, state.input.last_serial));
    activation.request_token_with_data(
        &state.qh,
        ActivationRequest {
            app_id,
            seat_and_serial,
            surface: Some(surface),
            reply: std::sync::Mutex::new(Some(reply)),
        },
    );
}

impl<M> std::fmt::Debug for HostCtx<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCtx")
            .field("state", &self.state)
            .field("pending_app", &self.app.len())
            .field("running", &self.running)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}
