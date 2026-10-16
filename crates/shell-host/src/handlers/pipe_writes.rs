//! The pipes other clients asked us to fill (`wl_data_source.send`): a drag's payload and our
//! selection's text. A handler queues the bytes here and returns; the loop writes them a chunk
//! at a time on a non-blocking descriptor (`ctx::pipe`), so a reader that stops reading cannot
//! stall the Wayland thread.

use std::sync::Arc;

use smithay_client_toolkit::data_device_manager::WritePipe;

use crate::drag::pipe::{Outgoing, Slot, WriteSlots};

/// A pipe and what it is owed.
#[derive(Debug)]
pub(crate) struct PendingWrite {
    pub(crate) pipe: WritePipe,
    pub(crate) outgoing: Outgoing,
    /// Held until the pipe closes.
    pub(crate) slot: Slot,
}

/// Whether a pipe was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Queued {
    Yes,
    /// Too many pipes are open: this one was closed at once, which its reader sees as an empty
    /// answer.
    Refused,
}

/// The pipes waiting for the loop, and the room left for more.
#[derive(Debug, Default)]
pub(crate) struct PipeWrites {
    slots: WriteSlots,
    queue: Vec<PendingWrite>,
}

impl PipeWrites {
    /// Queue `bytes` for `pipe`. Nothing is written here.
    pub(crate) fn push(&mut self, pipe: WritePipe, bytes: Arc<[u8]>) -> Queued {
        let Some(slot) = self.slots.take() else {
            return Queued::Refused;
        };
        self.queue.push(PendingWrite {
            pipe,
            outgoing: Outgoing::new(bytes),
            slot,
        });
        Queued::Yes
    }

    /// The pipes queued since the last call, for the loop to register.
    pub(crate) fn take(&mut self) -> Vec<PendingWrite> {
        std::mem::take(&mut self.queue)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::fd::OwnedFd;

    use super::*;
    use crate::drag::pipe::MAX_OPEN_WRITES;

    fn pipe() -> (std::io::PipeReader, WritePipe) {
        let (reader, writer) = std::io::pipe().expect("a pipe");
        (reader, WritePipe::from(OwnedFd::from(writer)))
    }

    #[test]
    fn a_large_payload_for_a_reader_that_never_reads_is_queued_not_written() {
        let mut writes = PipeWrites::default();
        let (mut reader, writer) = pipe();
        // Far over one pipe buffer: a blocking write of it would never return here.
        let payload: Arc<[u8]> = Arc::from(vec![9u8; 4 * 1024 * 1024]);
        assert_eq!(writes.push(writer, payload), Queued::Yes);
        let queued = writes.take();
        assert_eq!(queued.len(), 1);
        assert!(writes.take().is_empty(), "taken once");
        drop(queued);
        // Closing the queued pipe is all the reader sees: nothing was written.
        let mut seen = Vec::new();
        reader.read_to_end(&mut seen).expect("EOF after the close");
        assert!(seen.is_empty());
    }

    #[test]
    fn pipes_past_the_limit_are_refused_until_one_closes() {
        let mut writes = PipeWrites::default();
        let payload: Arc<[u8]> = Arc::from(&b"x"[..]);
        let mut readers = Vec::new();
        for n in 0..MAX_OPEN_WRITES {
            let (reader, writer) = pipe();
            readers.push(reader);
            assert_eq!(
                writes.push(writer, payload.clone()),
                Queued::Yes,
                "pipe {n}"
            );
        }
        let (_extra, writer) = pipe();
        assert_eq!(writes.push(writer, payload.clone()), Queued::Refused);
        drop(writes.take());
        let (_again, writer) = pipe();
        assert_eq!(
            writes.push(writer, payload),
            Queued::Yes,
            "the closed pipes gave their places back"
        );
    }
}
