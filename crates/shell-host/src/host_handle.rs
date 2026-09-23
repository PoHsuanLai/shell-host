//! `HostHandle`: sending to the host from any thread (a zbus task, a watcher thread).

use std::fmt;

use calloop::channel::Sender;

use crate::error::{HostError, Result};
use crate::id::SurfaceId;
use crate::msg::{HostMsg, SurfaceOp};

/// A cheap, cloneable sender into the host's event loop. `Send` whenever `M` is.
pub struct HostHandle<M> {
    sender: Sender<HostMsg<M>>,
}

impl<M> HostHandle<M> {
    /// A handle sending into `sender`. Built by `Host::connect`.
    pub(crate) fn new(sender: Sender<HostMsg<M>>) -> HostHandle<M> {
        HostHandle { sender }
    }

    /// Deliver `msg` to the event closure as `HostEvent::App`. Fails with `HostError::HostGone`
    /// after the host has exited.
    pub fn send(&self, msg: M) -> Result<()> {
        self.post(HostMsg::App(msg))
    }

    /// Apply `op` to surface `id`, as `HostCtx::apply` would.
    pub fn apply(&self, id: SurfaceId, op: SurfaceOp) -> Result<()> {
        self.post(HostMsg::Apply { id, op })
    }

    /// Close surface `id`.
    pub fn close(&self, id: SurfaceId) -> Result<()> {
        self.post(HostMsg::Close(id))
    }

    /// Close every surface and return from `Host::run`.
    pub fn exit(&self) -> Result<()> {
        self.post(HostMsg::Exit)
    }

    /// The channel is closed exactly when the loop (and with it the receiver) is gone.
    fn post(&self, msg: HostMsg<M>) -> Result<()> {
        self.sender.send(msg).map_err(|_| HostError::HostGone)
    }
}

impl<M> Clone for HostHandle<M> {
    fn clone(&self) -> Self {
        HostHandle {
            sender: self.sender.clone(),
        }
    }
}

impl<M> fmt::Debug for HostHandle<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostHandle").finish_non_exhaustive()
    }
}
