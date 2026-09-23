//! `Host`: connecting, and running the event loop.
//!
//! One thread runs every document (Blitz documents are `Rc<RefCell<..>>`). The loop is calloop
//! with the Wayland source, the `HostHandle` channel, one ping shared by every waker and shell
//! provider, and per-surface watchdog timers; it dispatches with no timeout, so an idle host
//! makes no wakeups at all. A 2-worker tokio runtime is entered on the main thread for the
//! whole run, so `tokio::time` and `watch` work inside Dioxus tasks.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use calloop::EventLoop;
use tokio::runtime::Runtime;

use crate::config::HostConfig;
use crate::ctx::HostCtx;
use crate::error::Result;
use crate::event::HostEvent;
use crate::host_handle::HostHandle;
use crate::msg::HostMsg;

/// A connected host, before it runs.
pub struct Host<M> {
    event_loop: EventLoop<'static, HostCtx<M>>,
    ctx: HostCtx<M>,
    sender: calloop::channel::Sender<HostMsg<M>>,
    runtime: Runtime,
}

impl<M: Send + 'static> Host<M> {
    /// Connect to the Wayland display, bind the globals (layer-shell, blur, fractional scale,
    /// text input, cursor shape and activation are optional and degrade), and choose the
    /// backend. The GPU device is created on the first surface, validated by a first present.
    pub fn connect(config: HostConfig) -> Result<Host<M>> {
        todo!()
    }

    /// A sender for messages from other threads.
    pub fn handle(&self) -> HostHandle<M> {
        todo!()
    }

    /// Run until `HostCtx::exit`, `HostHandle::exit`, or a fatal error. `HostEvent::Started`
    /// comes first, after the initial outputs.
    pub fn run(self, on_event: impl FnMut(&mut HostCtx<M>, HostEvent<M>) + 'static) -> Result<()> {
        todo!()
    }
}

impl<M> std::fmt::Debug for Host<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("ctx", &self.ctx)
            .finish_non_exhaustive()
    }
}
