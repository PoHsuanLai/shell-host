//! The pipes other clients asked us to fill (`wl_data_source.send`): a drag's payload and our
//! selection's text. A handler queues the bytes here and returns; the loop writes them a chunk
//! at a time on a non-blocking descriptor (`ctx::pipe`), so a reader that stops reading cannot
//! stall the Wayland thread.

use std::sync::Arc;

use smithay_client_toolkit::data_device_manager::WritePipe;

use crate::drag::pipe::{MAX_DRAG_WRITES, MAX_SELECTION_WRITES, Outgoing, Slot, WriteSlots};

/// A pipe and what it is owed.
#[derive(Debug)]
pub(crate) struct PendingWrite {
    pub(crate) pipe: WritePipe,
    pub(crate) outgoing: Outgoing,
    /// Held until the pipe closes.
    pub(crate) slot: Slot,
}

/// What a pipe is filled for; each has its own room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipeUse {
    /// Our selection's text, for a paste.
    Selection,
    /// A drag's payload, for a drop.
    Drag,
}

/// Whether a pipe was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Queued {
    Yes,
    /// Too many pipes are open: this one was closed at once, which its reader sees as an empty
    /// answer.
    Refused,
}

/// The pipes waiting for the loop, and the room left for more of each use.
#[derive(Debug)]
pub(crate) struct PipeWrites {
    selection: WriteSlots,
    drag: WriteSlots,
    queue: Vec<PendingWrite>,
}

impl Default for PipeWrites {
    fn default() -> PipeWrites {
        PipeWrites {
            selection: WriteSlots::with_limit(MAX_SELECTION_WRITES),
            drag: WriteSlots::with_limit(MAX_DRAG_WRITES),
            queue: Vec::new(),
        }
    }
}

impl PipeWrites {
    /// Queue `bytes` for `pipe`, filled for `purpose`. Nothing is written here.
    pub(crate) fn push(&mut self, purpose: PipeUse, pipe: WritePipe, bytes: Arc<[u8]>) -> Queued {
        let room = match purpose {
            PipeUse::Selection => &self.selection,
            PipeUse::Drag => &self.drag,
        };
        let Some(slot) = room.take() else {
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
        assert_eq!(
            writes.push(PipeUse::Selection, writer, payload),
            Queued::Yes
        );
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
    fn pipes_past_a_uses_limit_are_refused_until_one_closes_and_the_other_use_is_not_starved() {
        let mut writes = PipeWrites::default();
        let payload: Arc<[u8]> = Arc::from(&b"x"[..]);
        let mut readers = Vec::new();
        for n in 0..MAX_SELECTION_WRITES {
            let (reader, writer) = pipe();
            readers.push(reader);
            assert_eq!(
                writes.push(PipeUse::Selection, writer, payload.clone()),
                Queued::Yes,
                "pipe {n}"
            );
        }
        let (_extra, writer) = pipe();
        assert_eq!(
            writes.push(PipeUse::Selection, writer, payload.clone()),
            Queued::Refused
        );
        // A client that holds every selection place leaves the drag its own.
        for n in 0..MAX_DRAG_WRITES {
            let (reader, writer) = pipe();
            readers.push(reader);
            assert_eq!(
                writes.push(PipeUse::Drag, writer, payload.clone()),
                Queued::Yes,
                "drag pipe {n}"
            );
        }
        let (_over, writer) = pipe();
        assert_eq!(
            writes.push(PipeUse::Drag, writer, payload.clone()),
            Queued::Refused
        );
        drop(writes.take());
        let (_again, writer) = pipe();
        assert_eq!(
            writes.push(PipeUse::Selection, writer, payload),
            Queued::Yes,
            "the closed pipes gave their places back"
        );
    }
}
