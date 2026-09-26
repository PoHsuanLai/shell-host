//! Phase 4 of the flush, the palmrest part: when a connection attempt is due (at start, and
//! after a pointer enter while unconnected), connect and register the socket's reading side
//! with the event loop. Each readable wakeup drains every datagram into
//! `HostState::palmrest_message`; the daemon's end closing removes the source.

use calloop::PostAction;
use calloop::generic::Generic;
use calloop::{Interest, Mode};

use crate::ctx::HostCtx;
use crate::input::palmrest::{Retry, Stream, read_all};

/// Connect to palmrest if an attempt is due and the socket exists.
pub(super) fn connect_if_due<M: 'static>(ctx: &mut HostCtx<M>) {
    if ctx.state.palmrest.retry() != Retry::Due {
        return;
    }
    let Some(reader) = ctx.state.palmrest.connect() else {
        return;
    };
    let source = Generic::new(reader, Interest::READ, Mode::Level);
    let inserted = ctx.handle.insert_source(source, |_, fd, ctx| {
        let (messages, stream) = read_all(&**fd);
        for message in messages {
            ctx.state.palmrest_message(message);
        }
        match stream {
            Stream::Open => Ok(PostAction::Continue),
            Stream::Closed => {
                ctx.state.palmrest.disconnected();
                Ok(PostAction::Remove)
            }
            Stream::Incompatible { version } => {
                eprintln!(
                    "shell-host: palmrest speaks protocol {version}, this host {}; disconnecting",
                    crate::input::palmrest::wire::PROTOCOL_VERSION
                );
                ctx.state.palmrest.disconnected();
                Ok(PostAction::Remove)
            }
        }
    });
    if let Err(error) = inserted {
        eprintln!("shell-host: palmrest socket source: {error}");
        ctx.state.palmrest.disconnected();
    }
}
