//! `HostHandle`: sending to the host from any thread (a zbus task, a watcher thread).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::fmt;

use calloop::channel::Sender;

use crate::error::Result;
use crate::id::SurfaceId;
use crate::msg::{HostMsg, SurfaceOp};

/// A cheap, cloneable sender into the host's event loop. `Send` whenever `M` is.
pub struct HostHandle<M> {
    sender: Sender<HostMsg<M>>,
}

impl<M> HostHandle<M> {
    /// A handle sending into `sender`. Built by `Host::connect`.
    pub(crate) fn new(sender: Sender<HostMsg<M>>) -> HostHandle<M> {
        todo!()
    }

    /// Deliver `msg` to the event closure as `HostEvent::App`. Fails with `HostError::HostGone`
    /// after the host has exited.
    pub fn send(&self, msg: M) -> Result<()> {
        todo!()
    }

    /// Apply `op` to surface `id`, as `HostCtx::apply` would.
    pub fn apply(&self, id: SurfaceId, op: SurfaceOp) -> Result<()> {
        todo!()
    }

    /// Close surface `id`.
    pub fn close(&self, id: SurfaceId) -> Result<()> {
        todo!()
    }

    /// Close every surface and return from `Host::run`.
    pub fn exit(&self) -> Result<()> {
        todo!()
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
