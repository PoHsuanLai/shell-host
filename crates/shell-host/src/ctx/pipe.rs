//! Filling the pipes other clients asked us to write (`wl_data_source.send`, for a drag's
//! payload and for our selection's text). A pipe is made non-blocking and written from the loop a
//! chunk at a time whenever it is writable, so a slow or stuck reader never stalls the Wayland
//! thread; the pipe closes when everything is written, the reader is gone, or it has stayed open
//! for [`WRITE_DEADLINE`].
//!
//! See FINDINGS F60.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};

use crate::ctx::HostCtx;
use crate::drag::pipe::{Step, WRITE_DEADLINE};
use crate::handlers::pipe_writes::PendingWrite;

/// Hand every pipe queued since the last flush to the loop.
pub(super) fn register_writes<M: 'static>(ctx: &mut HostCtx<M>) {
    for write in ctx.state.pipe_writes.take() {
        register(ctx, write);
    }
}

fn register<M: 'static>(
    ctx: &mut HostCtx<M>,
    PendingWrite {
        pipe,
        outgoing,
        slot,
    }: PendingWrite,
) {
    let file = match non_blocking(OwnedFd::from(pipe)) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("shell-host: pipe: {error}");
            return;
        }
    };
    let source = Generic::new(file, Interest::WRITE, Mode::Level);
    let mut outgoing = outgoing;
    let inserted = ctx.handle.insert_source(source, move |_, file, _| {
        // The place among the open pipes is held for as long as this source lives.
        let _held = &slot;
        let mut writer: &File = file;
        let result = writer.write(outgoing.chunk());
        Ok(match outgoing.wrote(result) {
            Step::Wait => PostAction::Continue,
            // Removing the source drops the file, which closes the pipe: the reader's EOF.
            Step::Done | Step::Abandon => PostAction::Remove,
        })
    });
    match inserted {
        Ok(token) => remove_after(&ctx.handle, token, WRITE_DEADLINE),
        Err(error) => eprintln!("shell-host: pipe: {error}"),
    }
}

/// Remove source `token` once `after` has passed. A source that is gone by then leaves a stale
/// token, which the loop ignores.
fn remove_after<M: 'static>(
    handle: &LoopHandle<'static, HostCtx<M>>,
    token: RegistrationToken,
    after: Duration,
) {
    let remover = handle.clone();
    let armed = handle.insert_source(Timer::from_duration(after), move |_, _, _| {
        remover.remove(token);
        TimeoutAction::Drop
    });
    if let Err(error) = armed {
        eprintln!("shell-host: pipe deadline: {error}");
    }
}

/// `fd` with `O_NONBLOCK` set, as a `File` to read or write through. std sets the flag only
/// through a socket type; `FIONBIO` (what `UnixStream::set_nonblocking` sends) sets it on any
/// open file description, a pipe's included, and nothing else about the descriptor changes.
pub(super) fn non_blocking(fd: OwnedFd) -> io::Result<File> {
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

    #[test]
    fn an_empty_pipe_answers_would_block_when_read() {
        let (reader, _writer) = std::io::pipe().expect("a pipe");
        let file = non_blocking(OwnedFd::from(reader)).expect("the flag is set");
        let mut reader: &File = &file;
        let error = reader.read(&mut [0u8; 8]).expect_err("nothing was written");
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }
}
