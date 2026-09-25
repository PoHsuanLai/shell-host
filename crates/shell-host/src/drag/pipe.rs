//! The bytes a `wl_data_source.send` pipe still owes, and what each write's result means.
//!
//! The host writes on the event loop, a chunk each time the pipe is writable, on a
//! non-blocking descriptor (`ctx::drag`); this is the bookkeeping, free of the descriptor.

use std::io;
use std::sync::Arc;

/// The most one write offers, so a pipe that is barely writable costs one short write.
pub const CHUNK: usize = 64 * 1024;

/// A payload part-way through a pipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    bytes: Arc<[u8]>,
    sent: usize,
}

/// What to do after a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Wait for the pipe to be writable again.
    Wait,
    /// Everything is written: close the pipe, which tells the reader it has it all.
    Done,
    /// The reader went away or the pipe failed: close it and forget the rest.
    Abandon,
}

impl Outgoing {
    /// All of `bytes`, none sent yet.
    pub fn new(bytes: Arc<[u8]>) -> Outgoing {
        Outgoing { bytes, sent: 0 }
    }

    /// The next chunk to offer the pipe; empty once everything is sent.
    pub fn chunk(&self) -> &[u8] {
        let rest = &self.bytes[self.sent..];
        &rest[..rest.len().min(CHUNK)]
    }

    /// Account for one write's `result`, and say what follows.
    pub fn wrote(&mut self, result: io::Result<usize>) -> Step {
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

#[cfg(test)]
mod tests {
    use super::*;

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
