//! Starting a drag out of a surface, and filling the pipes its targets ask for.
//!
//! A start is checked up front (`drag::serial`, the data device, one drag at a time) and
//! answered `Unsupported` at once when it cannot run. A pipe a target hands us
//! (`wl_data_source.send`) is made non-blocking and written from the loop a chunk at a time
//! whenever it is writable, so a slow or stuck reader never stalls the Wayland thread; the
//! pipe closes when everything is written or the reader is gone.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;

use calloop::PostAction;
use calloop::generic::Generic;
use calloop::{Interest, Mode};
use tokio::sync::oneshot;
use wayland_client::Proxy;

use crate::ctx::HostCtx;
use crate::drag::offer::offers;
use crate::drag::outcome::{DragOutcome, DragRefused};
use crate::drag::pipe::Step;
use crate::drag::serial::drag_serial;
use crate::drag::spec::DragSpec;
use crate::handlers::drag_source::{ActiveDrag, PendingWrite};
use crate::id::SurfaceId;

/// `wl_data_source.dnd_drop_performed`, `dnd_finished` and `action` exist from v3.
const DND_EVENTS_SINCE: u32 = 3;

/// Start dragging `spec` out of surface `from`; `reply` gets the outcome.
pub(super) fn start<M: 'static>(
    ctx: &mut HostCtx<M>,
    from: SurfaceId,
    spec: DragSpec,
    reply: oneshot::Sender<DragOutcome>,
) {
    if let Err(why) = check(ctx, from) {
        // The caller may have stopped waiting; nobody is left to tell then.
        let _ = reply.send(DragOutcome::Unsupported { why });
        return;
    }
    let icon = super::drag_icon::open(ctx, from, spec.icon);
    let state = &mut ctx.state;
    let (Some(manager), Some(device), Some(origin), Ok(serial)) = (
        &state.data_device,
        &state.input.data_device,
        state.surfaces.get(&from),
        drag_serial(state.input.grab.get(), from),
    ) else {
        // `check` saw all four a moment ago, on the same thread.
        return;
    };
    let offered = offers(&spec.payload);
    let source = manager.create_drag_and_drop_source(
        &state.qh,
        offered.iter().map(|offer| offer.mime.as_str()),
        spec.actions.to_wire(),
    );
    let icon_surface = icon.and_then(|id| state.surfaces.get(&id));
    source.start_drag(
        device,
        origin.role.wl_surface(),
        icon_surface.map(|icon| icon.role.wl_surface()),
        serial,
    );
    state.drag.active = Some(ActiveDrag {
        source,
        offered,
        reply,
        chosen: None,
        icon,
    });
}

/// Why a drag from `from` cannot start now, if it cannot.
fn check<M: 'static>(ctx: &HostCtx<M>, from: SurfaceId) -> Result<(), DragRefused> {
    let state = &ctx.state;
    if state.drag.active.is_some() {
        return Err(DragRefused::InProgress);
    }
    if !state.surfaces.contains_key(&from) {
        return Err(DragRefused::NoSurface);
    }
    let manager = state
        .data_device
        .as_ref()
        .ok_or(DragRefused::NoDataDevice)?;
    if manager.data_device_manager().version() < DND_EVENTS_SINCE {
        return Err(DragRefused::OldProtocol);
    }
    state
        .input
        .data_device
        .as_ref()
        .ok_or(DragRefused::NoDataDevice)?;
    drag_serial(state.input.grab.get(), from).map(drop)
}

/// Hand every pipe a target asked for since the last flush to the loop.
pub(super) fn register_writes<M: 'static>(ctx: &mut HostCtx<M>) {
    for PendingWrite { pipe, outgoing } in std::mem::take(&mut ctx.state.drag.writes) {
        let file = match non_blocking(OwnedFd::from(pipe)) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("shell-host: drag pipe: {error}");
                continue;
            }
        };
        let source = Generic::new(file, Interest::WRITE, Mode::Level);
        let mut outgoing = outgoing;
        let inserted = ctx.handle.insert_source(source, move |_, file, _| {
            let mut writer: &File = file;
            let result = writer.write(outgoing.chunk());
            Ok(match outgoing.wrote(result) {
                Step::Wait => PostAction::Continue,
                // Removing the source drops the file, which closes the pipe: the reader's EOF.
                Step::Done | Step::Abandon => PostAction::Remove,
            })
        });
        if let Err(error) = inserted {
            eprintln!("shell-host: drag pipe: {error}");
        }
    }
}

/// `fd` with `O_NONBLOCK` set, as a `File` to write through. std sets the flag only through a
/// socket type; `FIONBIO` (what `UnixStream::set_nonblocking` sends) sets it on any open file
/// description, a pipe's included, and nothing else about the descriptor changes.
fn non_blocking(fd: OwnedFd) -> io::Result<File> {
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(true)?;
    Ok(File::from(OwnedFd::from(stream)))
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn a_full_pipe_answers_would_block_instead_of_stalling() {
        let (mut reader, writer) = std::io::pipe().expect("a pipe");
        let file = non_blocking(OwnedFd::from(writer)).expect("the flag is set");
        let mut writer: &File = &file;
        let chunk = [1u8; 64 * 1024];
        // No default pipe holds 64 MiB: a blocking descriptor would hang in this loop.
        let full = (0..1024).find_map(|_| match writer.write(&chunk) {
            Ok(_) => None,
            Err(error) => Some(error.kind()),
        });
        assert_eq!(full, Some(io::ErrorKind::WouldBlock));
        drop(file);
        let mut first = [0u8; 16];
        reader
            .read_exact(&mut first)
            .expect("the written part arrives");
        assert_eq!(first, [1u8; 16]);
    }
}
