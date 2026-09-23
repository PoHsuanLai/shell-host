//! `HostCtx`: what the shell's event closure uses to manage surfaces. It is the event loop's
//! data: the Wayland state plus the loop-level pieces that depend on the app message type.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::collections::{BTreeMap, VecDeque};

use blitz_traits::shell::ColorScheme;
use calloop::{LoopHandle, RegistrationToken};
use tokio::sync::oneshot;

use crate::debug::FrameStats;
use crate::error::Result;
use crate::id::SurfaceId;
use crate::layer::LayerConfig;
use crate::msg::{SurfaceOp, Visibility};
use crate::output::OutputInfo;
use crate::root::Root;
use crate::state::HostState;
use crate::toplevel::ToplevelConfig;

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
    pub(crate) repeat: Option<RegistrationToken>,
    pub(crate) running: Running,
}

impl<M: 'static> HostCtx<M> {
    /// The outputs as of now.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        todo!()
    }

    /// Create a layer surface rendering `root`. `HostError::Unsupported` without layer-shell.
    pub fn spawn_layer(&mut self, config: LayerConfig, root: Root) -> Result<SurfaceId> {
        todo!()
    }

    /// Create an xdg toplevel rendering `root`.
    pub fn spawn_toplevel(&mut self, config: ToplevelConfig, root: Root) -> Result<SurfaceId> {
        todo!()
    }

    /// Close a surface and its popups; `HostEvent::SurfaceClosed{reason: Requested}` follows.
    pub fn close(&mut self, id: SurfaceId) {
        todo!()
    }

    /// Hide or show a surface. Hidden keeps its `VirtualDom` warm.
    pub fn set_visibility(&mut self, id: SurfaceId, visibility: Visibility) {
        todo!()
    }

    /// Apply `op` to surface `id` before its next paint.
    pub fn apply(&mut self, id: SurfaceId, op: SurfaceOp) {
        todo!()
    }

    /// Request an xdg-activation token on behalf of surface `id`, for launching `app_id`. The
    /// receiver yields `None` without xdg-activation or when refused.
    pub fn activation_token(
        &mut self,
        id: SurfaceId,
        app_id: Option<String>,
    ) -> oneshot::Receiver<Option<String>> {
        todo!()
    }

    /// Surface `id`'s frame statistics over the samples since the last debug `stats` report;
    /// `None` for an unknown id.
    pub fn stats(&self, id: SurfaceId) -> Option<FrameStats> {
        todo!()
    }

    /// Set every document's `prefers-color-scheme`.
    pub fn set_color_scheme(&mut self, scheme: ColorScheme) {
        todo!()
    }

    /// Close every surface and return from `Host::run` after this event.
    pub fn exit(&mut self) {
        todo!()
    }

    /// After every dispatch: poll woken documents, drain head queues, apply queued requests,
    /// step every surface's frame machine (render the dirty ones), arm or cancel the timers the
    /// handlers asked for, then flush the connection. Stops with `HostError::Protocol` once
    /// the connection has a protocol error (SPIKE.md S1 finding 7).
    pub(crate) fn flush(&mut self) -> Result<Running> {
        todo!()
    }
}

impl<M> std::fmt::Debug for HostCtx<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCtx")
            .field("state", &self.state)
            .field("pending_app", &self.app.len())
            .field("running", &self.running)
            .finish_non_exhaustive()
    }
}
