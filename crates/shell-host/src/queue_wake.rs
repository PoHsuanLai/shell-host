//! The host's Wayland queue in the loop, dispatched however its events arrive.
//!
//! The host's thread is not the only reader of its connection: a program that shares it (sill's
//! desktop-protocol thread) and Mesa's own reads for a GPU surface read the socket too, and such a
//! read moves the host's events into the host's queue at once. calloop's Wayland source wakes only
//! on the socket and on libwayland's queue, so those events sat there until some unrelated socket
//! traffic came (a sheet mapped while the desktop was idle stayed unconfigured for 22 s, until the
//! bar's minute tick). `poll_dispatch_pending` leaves a waker in the queue once it is empty; the
//! queue wakes it when any thread's read adds an event, and the waker's ping dispatches the queue
//! on this thread.

use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use calloop::ping::{Ping, make_ping};
use calloop::{Dispatcher, LoopHandle};
use calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, DispatchError, EventQueue};

use crate::ctx::HostCtx;
use crate::error::{HostError, Result};
use crate::state::HostState;

/// Pings the loop when a read on another thread queues an event for the host.
struct QueueWaker(Ping);

impl Wake for QueueWaker {
    fn wake(self: Arc<Self>) {
        self.0.ping();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.ping();
    }
}

/// The queue's socket source, and the ping its waker sends.
pub(crate) fn insert_wayland<M: 'static>(
    handle: &LoopHandle<'static, HostCtx<M>>,
    conn: Connection,
    queue: EventQueue<HostState>,
) -> Result<()> {
    let (ping, ping_source) = make_ping().map_err(loop_error)?;
    let waker = Waker::from(Arc::new(QueueWaker(ping)));
    let on_socket = waker.clone();
    // The queue dispatches into the non-generic `HostState` inside the context.
    let wayland = Dispatcher::new(
        WaylandSource::new(conn, queue),
        move |(), queue: &mut EventQueue<HostState>, ctx: &mut HostCtx<M>| {
            dispatch(queue, &mut ctx.state, &on_socket)
        },
    );
    handle
        .register_dispatcher(wayland.clone())
        .map_err(loop_error)?;
    handle
        .insert_source(ping_source, move |(), (), ctx| {
            let mut source = wayland.as_source_mut();
            if let Err(error) = dispatch(source.queue(), &mut ctx.state, &waker) {
                // The flush after this dispatch ends the run with it (`drive` reports a protocol
                // error from the connection first).
                ctx.failed
                    .get_or_insert_with(|| HostError::EventLoop(error.to_string()));
            }
        })
        .map(drop)
        .map_err(loop_error)
}

/// Everything queued, then the waker left for the next event: `Ok(0)` tells the Wayland source
/// the queue is empty.
fn dispatch(
    queue: &mut EventQueue<HostState>,
    state: &mut HostState,
    waker: &Waker,
) -> std::result::Result<usize, DispatchError> {
    match queue.poll_dispatch_pending(&mut Context::from_waker(waker), state) {
        Poll::Pending => Ok(0),
        Poll::Ready(Err(error)) => Err(error),
        Poll::Ready(Ok(never)) => match never {},
    }
}

fn loop_error(error: impl std::fmt::Display) -> HostError {
    HostError::EventLoop(error.to_string())
}
