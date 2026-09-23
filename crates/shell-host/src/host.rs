//! `Host`: connecting, and running the event loop.
//!
//! One thread runs every document (Blitz documents are `Rc<RefCell<..>>`). The loop is calloop
//! with the Wayland source, the `HostHandle` channel, one ping shared by every waker and shell
//! provider, and per-surface watchdog timers; it dispatches with no timeout, so an idle host
//! makes no wakeups at all. A 2-worker tokio runtime is entered on the main thread for the
//! whole run, so `tokio::time` and `watch` work inside Dioxus tasks.

use std::collections::{BTreeMap, VecDeque};

use calloop::channel::{Channel, Event as ChannelEvent};
use calloop::ping::make_ping;
use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use tokio::runtime::Runtime;
use wayland_client::globals::registry_queue_init;
use wayland_client::{Connection, EventQueue};

use crate::config::HostConfig;
use crate::ctx::{HostCtx, Running};
use crate::error::{HostError, Result};
use crate::event::{HostEvent, widen};
use crate::handle::RequestQueue;
use crate::host_handle::HostHandle;
use crate::msg::{HostMsg, Request};
use crate::state::HostState;

/// Worker threads of the host's tokio runtime (plan "Event loop").
const TOKIO_WORKERS: usize = 2;

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
        let conn = Connection::connect_to_env().map_err(|e| HostError::Connect(e.to_string()))?;
        let (globals, mut queue) = registry_queue_init::<HostState>(&conn)
            .map_err(|e| HostError::Connect(e.to_string()))?;
        let event_loop = EventLoop::<HostCtx<M>>::try_new().map_err(loop_error)?;
        let handle = event_loop.handle();
        let (ping, ping_source) = make_ping().map_err(loop_error)?;
        // The ping only wakes the loop; the flush after every dispatch does the work.
        handle
            .insert_source(ping_source, |(), (), _| {})
            .map_err(loop_error)?;
        let requests = RequestQueue::new(ping);
        let first_mouse = config.first_mouse_window_ms.duration();
        let mut state = HostState::bind(
            conn.clone(),
            &globals,
            queue.handle(),
            requests,
            first_mouse,
        )?;
        // The initial outputs (and seats), so `HostEvent::Started` can see them.
        queue
            .roundtrip(&mut state)
            .map_err(|e| protocol_error(&conn, &e))?;
        insert_wayland(&handle, conn, queue)?;
        let (sender, channel) = calloop::channel::channel();
        insert_channel(&handle, channel)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(TOKIO_WORKERS)
            .enable_all()
            .build()
            .map_err(loop_error)?;
        let ctx = HostCtx {
            state,
            app: VecDeque::new(),
            handle,
            watchdogs: BTreeMap::new(),
            repeat: None,
            running: Running::Continue,
            backend: config.backend,
            adapter: config.adapter,
            instance: None,
        };
        Ok(Host {
            event_loop,
            ctx,
            sender,
            runtime,
        })
    }

    /// A sender for messages from other threads.
    pub fn handle(&self) -> HostHandle<M> {
        HostHandle::new(self.sender.clone())
    }

    /// Run until `HostCtx::exit`, `HostHandle::exit`, or a fatal error. `HostEvent::Started`
    /// comes first, after the initial outputs.
    pub fn run(
        self,
        mut on_event: impl FnMut(&mut HostCtx<M>, HostEvent<M>) + 'static,
    ) -> Result<()> {
        let Host {
            mut event_loop,
            mut ctx,
            sender,
            runtime,
        } = self;
        let result = {
            // Held for the whole run: Dioxus tasks use tokio's timers and channels.
            let _entered = runtime.enter();
            let result = drive(&mut event_loop, &mut ctx, &mut on_event);
            // Documents (and their tasks) go while the runtime is still entered; every wgpu
            // surface before its wl_surface (S1 finding 6).
            ctx.state.surfaces.clear();
            // The connection may be dead already; there is nothing left to tell it.
            let _ = ctx.state.conn.flush();
            result
        };
        drop(ctx);
        drop(event_loop);
        drop(sender);
        runtime.shutdown_background();
        result
    }
}

/// The loop: `Started`, then dispatch, deliver, flush until something asks to exit.
fn drive<M: 'static>(
    event_loop: &mut EventLoop<'static, HostCtx<M>>,
    ctx: &mut HostCtx<M>,
    on_event: &mut impl FnMut(&mut HostCtx<M>, HostEvent<M>),
) -> Result<()> {
    on_event(ctx, HostEvent::Started);
    loop {
        settle(ctx, on_event)?;
        if ctx.running == Running::Exit {
            return Ok(());
        }
        let dispatched = event_loop.dispatch(None, ctx);
        if let Some(error) = ctx.state.conn.protocol_error() {
            return Err(HostError::Protocol(format!("{error:?}")));
        }
        dispatched.map_err(|e| HostError::EventLoop(e.to_string()))?;
    }
}

/// Deliver pending events to the closure and flush, until a flush raises no new events (a
/// closure that closes a surface hears about it in the same iteration). Stops delivering once
/// the host is exiting.
fn settle<M: 'static>(
    ctx: &mut HostCtx<M>,
    on_event: &mut impl FnMut(&mut HostCtx<M>, HostEvent<M>),
) -> Result<()> {
    loop {
        while ctx.running == Running::Continue
            && let Some(event) = next_event(ctx)
        {
            on_event(ctx, event);
        }
        if ctx.flush()? == Running::Exit {
            return Ok(());
        }
        if ctx.state.events.is_empty() && ctx.app.is_empty() {
            return Ok(());
        }
    }
}

/// Handler events first (they describe the world the app messages were sent into), then app
/// messages, each in arrival order.
fn next_event<M>(ctx: &mut HostCtx<M>) -> Option<HostEvent<M>> {
    ctx.state
        .events
        .pop_front()
        .map(widen)
        .or_else(|| ctx.app.pop_front().map(HostEvent::App))
}

fn insert_wayland<M: 'static>(
    handle: &LoopHandle<'static, HostCtx<M>>,
    conn: Connection,
    queue: EventQueue<HostState>,
) -> Result<()> {
    // The queue dispatches into the non-generic `HostState` inside the context (FINDINGS F9).
    handle
        .insert_source(WaylandSource::new(conn, queue), |(), queue, ctx| {
            queue.dispatch_pending(&mut ctx.state)
        })
        .map(drop)
        .map_err(|e| HostError::EventLoop(e.to_string()))
}

fn insert_channel<M: 'static>(
    handle: &LoopHandle<'static, HostCtx<M>>,
    channel: Channel<HostMsg<M>>,
) -> Result<()> {
    handle
        .insert_source(channel, |event, (), ctx| match event {
            ChannelEvent::Msg(msg) => receive(ctx, msg),
            // Every `HostHandle` is gone; nothing more will arrive.
            ChannelEvent::Closed => {}
        })
        .map(drop)
        .map_err(|e| HostError::EventLoop(e.to_string()))
}

/// A `HostHandle` message. Ops and closes join the request queue, so they apply at the same
/// point of the flush as a document's own (after the polls, before paint).
fn receive<M: 'static>(ctx: &mut HostCtx<M>, msg: HostMsg<M>) {
    match msg {
        HostMsg::App(app) => ctx.app.push_back(app),
        HostMsg::Apply { id, op } => ctx.state.requests.push(Request::Apply { id, op }),
        HostMsg::Close(id) => ctx.state.requests.push(Request::Close(id)),
        HostMsg::Exit => ctx.exit(),
    }
}

fn loop_error(error: impl std::fmt::Display) -> HostError {
    HostError::EventLoop(error.to_string())
}

fn protocol_error(conn: &Connection, error: &impl std::fmt::Display) -> HostError {
    match conn.protocol_error() {
        Some(fatal) => HostError::Protocol(format!("{fatal:?}")),
        None => HostError::Connect(error.to_string()),
    }
}

impl<M> std::fmt::Debug for Host<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("ctx", &self.ctx)
            .finish_non_exhaustive()
    }
}
