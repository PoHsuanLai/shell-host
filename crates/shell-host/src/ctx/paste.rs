//! Reading the selection offer the seat is following (`Clipboard::follow`): the pipe is made
//! non-blocking and read from the loop a chunk at a time, up to `paste::MAX_PASTE_BYTES` and
//! `paste::PASTE_TIMEOUT`, then closed. One read runs at a time: a newer selection cancels the
//! one in flight, so a client that re-offers the selection in a loop, or never answers, holds
//! one descriptor for at most the timeout.
//!
//! See FINDINGS F60.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, Mode, PostAction, RegistrationToken};

use crate::clipboard::{Generation, PendingRead};
use crate::ctx::HostCtx;
use crate::ctx::pipe::non_blocking;
use crate::paste::{Arrival, Incoming, PASTE_TIMEOUT, READ_CHUNK};

/// The read in flight.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Reading {
    generation: Generation,
    source: RegistrationToken,
}

/// Start reading the pipe `Clipboard::follow` queued since the last flush, if any.
pub(super) fn register_read<M: 'static>(ctx: &mut HostCtx<M>) {
    let Some(PendingRead { pipe, generation }) = ctx.state.clipboard.take_read() else {
        return;
    };
    cancel(ctx);
    let file = match non_blocking(OwnedFd::from(pipe)) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("shell-host: paste pipe: {error}");
            ctx.state.clipboard.selection.publish(generation, None);
            return;
        }
    };
    let source = Generic::new(file, Interest::READ, Mode::Level);
    let mut incoming = Incoming::default();
    let inserted = ctx.handle.insert_source(source, move |_, file, ctx| {
        let mut chunk = [0u8; READ_CHUNK];
        let mut reader: &File = file;
        let result = reader.read(&mut chunk);
        Ok(match incoming.received(&chunk, result) {
            Arrival::Wait => PostAction::Continue,
            Arrival::Finished(text) => {
                finish(ctx, generation, text);
                PostAction::Remove
            }
        })
    });
    match inserted {
        Ok(source) => {
            ctx.reading = Some(Reading { generation, source });
            arm_timeout(ctx, generation);
        }
        Err(error) => {
            eprintln!("shell-host: paste pipe: {error}");
            ctx.state.clipboard.selection.publish(generation, None);
        }
    }
}

/// Stop the read in flight, if there is one. Its pipe closes; its selection is already stale.
fn cancel<M: 'static>(ctx: &mut HostCtx<M>) {
    if let Some(Reading { source, .. }) = ctx.reading.take() {
        ctx.handle.remove(source);
    }
}

/// The read of `generation` ended with `text`: publish it and free the slot.
fn finish<M: 'static>(ctx: &mut HostCtx<M>, generation: Generation, text: Option<String>) {
    if ctx
        .reading
        .is_some_and(|reading| reading.generation == generation)
    {
        ctx.reading = None;
    }
    ctx.state.clipboard.selection.publish(generation, text);
}

/// Give up on the read of `generation` after the timeout, unless it has ended by then.
fn arm_timeout<M: 'static>(ctx: &mut HostCtx<M>, generation: Generation) {
    let armed = ctx
        .handle
        .insert_source(Timer::from_duration(PASTE_TIMEOUT), move |_, _, ctx| {
            let running = ctx
                .reading
                .is_some_and(|reading| reading.generation == generation);
            if running {
                cancel(ctx);
                ctx.state.clipboard.selection.publish(generation, None);
            }
            TimeoutAction::Drop
        });
    if let Err(error) = armed {
        eprintln!("shell-host: paste deadline: {error}");
    }
}
