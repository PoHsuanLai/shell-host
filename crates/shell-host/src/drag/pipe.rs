//! The bytes a `wl_data_source.send` pipe still owes, and what each write's result means: for a
//! drag's payload and for our selection alike.
//!
//! The host writes on the event loop, a chunk each time the pipe is writable, on a
//! non-blocking descriptor (`ctx::pipe`); this is the bookkeeping, free of the descriptor.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The most one write offers, so a pipe that is barely writable costs one short write.
pub(crate) const CHUNK: usize = 64 * 1024;

/// A payload part-way through a pipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outgoing {
    bytes: Arc<[u8]>,
    sent: usize,
}

/// The most pipes the host fills at once, queued or open: a client that asks for the selection
/// in a loop and never reads cannot grow the host's descriptors without bound.
pub(crate) const MAX_OPEN_WRITES: usize = 8;

/// How long one pipe may stay open: a reader that stops draining it and never closes it is cut
/// off, so it cannot hold a slot for good.
pub(crate) const WRITE_DEADLINE: Duration = Duration::from_secs(10);

/// What to do after a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Wait for the pipe to be writable again.
    Wait,
    /// Everything is written: close the pipe, which tells the reader it has it all.
    Done,
    /// The reader went away or the pipe failed: close it and forget the rest.
    Abandon,
}

impl Outgoing {
    /// All of `bytes`, none sent yet.
    pub(crate) fn new(bytes: Arc<[u8]>) -> Outgoing {
        Outgoing { bytes, sent: 0 }
    }

    /// The next chunk to offer the pipe; empty once everything is sent.
    pub(crate) fn chunk(&self) -> &[u8] {
        let rest = &self.bytes[self.sent..];
        &rest[..rest.len().min(CHUNK)]
    }

    /// Account for one write's `result`, and say what follows.
    pub(crate) fn wrote(&mut self, result: io::Result<usize>) -> Step {
        match result {
            Ok(0) if !self.chunk().is_empty() => Step::Abandon,
            Ok(n) => {
                self.sent = (self.sent + n).min(self.bytes.len());
                if self.sent == self.bytes.len() {
                    Step::Done
                } else {
                    Step::Wait
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Step::Wait
            }
            Err(_) => Step::Abandon,
        }
    }
}

/// How many pipes are open, against a limit. Clones count together.
#[derive(Debug, Clone)]
pub(crate) struct WriteSlots {
    open: Arc<AtomicUsize>,
    limit: usize,
}

/// One open pipe's place among [`WriteSlots`], given back when it drops.
#[derive(Debug)]
pub(crate) struct Slot(Arc<AtomicUsize>);

impl WriteSlots {
    /// Room for `limit` pipes.
    pub(crate) fn with_limit(limit: usize) -> WriteSlots {
        WriteSlots {
            open: Arc::new(AtomicUsize::new(0)),
            limit,
        }
    }

    /// A place for one more pipe, or `None` when the limit is reached.
    pub(crate) fn take(&self) -> Option<Slot> {
        let before = self.open.fetch_add(1, Ordering::AcqRel);
        if before >= self.limit {
            self.open.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Slot(Arc::clone(&self.open)))
    }
}

impl Default for WriteSlots {
    fn default() -> WriteSlots {
        WriteSlots::with_limit(MAX_OPEN_WRITES)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_given_back_when_its_pipe_goes() {
        let slots = WriteSlots::with_limit(2);
        let first = slots.take().expect("room for one");
        let second = slots.take().expect("room for two");
        assert!(slots.take().is_none(), "the third is refused");
        drop(first);
        let third = slots.take().expect("room again once a pipe closed");
        assert!(slots.take().is_none(), "still two open");
        drop((second, third));
        assert!(slots.take().is_some(), "all given back");
    }

    fn outgoing(len: usize) -> Outgoing {
        Outgoing::new(Arc::from(vec![7u8; len]))
    }

    #[test]
    fn each_write_result_decides_the_next_step() {
        let kind = |kind| Err(io::Error::from(kind));
        let cases: Vec<(&str, usize, io::Result<usize>, Step, usize)> = vec![
            ("all at once", 10, Ok(10), Step::Done, 0),
            ("short write", 10, Ok(4), Step::Wait, 6),
            (
                "pipe full",
                10,
                kind(io::ErrorKind::WouldBlock),
                Step::Wait,
                10,
            ),
            (
                "signal",
                10,
                kind(io::ErrorKind::Interrupted),
                Step::Wait,
                10,
            ),
            (
                "reader gone",
                10,
                kind(io::ErrorKind::BrokenPipe),
                Step::Abandon,
                10,
            ),
            ("zero on a non-empty chunk", 10, Ok(0), Step::Abandon, 10),
            ("empty payload", 0, Ok(0), Step::Done, 0),
        ];
        for (name, len, result, step, left) in cases {
            let mut out = outgoing(len);
            assert_eq!(out.wrote(result), step, "{name}");
            if step != Step::Abandon {
                assert_eq!(out.chunk().len(), left, "{name}");
            }
        }
    }

    #[test]
    fn a_large_payload_goes_out_a_chunk_at_a_time() {
        let mut out = outgoing(CHUNK * 2 + 5);
        assert_eq!(out.chunk().len(), CHUNK);
        assert_eq!(out.wrote(Ok(CHUNK)), Step::Wait);
        assert_eq!(out.wrote(Ok(CHUNK)), Step::Wait);
        assert_eq!(out.chunk().len(), 5);
        assert_eq!(out.wrote(Ok(5)), Step::Done);
        assert!(out.chunk().is_empty());
    }
}
