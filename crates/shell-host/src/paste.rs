//! Text arriving through a selection offer's pipe, and when to stop taking it.
//!
//! The host reads on the event loop, a chunk each time the pipe is readable, on a non-blocking
//! descriptor (`ctx::paste`); this is the bookkeeping, free of the descriptor. A selection owner
//! can write without end or never close, so the read has a size cap here and a deadline there.

use std::io;
use std::time::Duration;

/// How long reading a selection offer waits for its owner to finish writing.
pub(crate) const PASTE_TIMEOUT: Duration = Duration::from_millis(200);

/// The most text one paste takes. A selection larger than this has no text to paste, rather
/// than a cut-off one that would be pasted as if whole.
pub(crate) const MAX_PASTE_BYTES: usize = 1024 * 1024;

/// How much one read asks the pipe for.
pub(crate) const READ_CHUNK: usize = 16 * 1024;

/// What the read does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Arrival {
    /// Wait for the pipe to be readable again.
    Wait,
    /// Stop reading and close the pipe. `None`: the owner sent no usable text (too much of it,
    /// invalid UTF-8, or the pipe failed).
    Finished(Option<String>),
}

/// The bytes received so far.
#[derive(Debug, Default)]
pub(crate) struct Incoming {
    bytes: Vec<u8>,
}

impl Incoming {
    /// Account for one read's `result`, which filled the start of `chunk`.
    pub(crate) fn received(&mut self, chunk: &[u8], result: io::Result<usize>) -> Arrival {
        match result {
            Ok(0) => Arrival::Finished(String::from_utf8(std::mem::take(&mut self.bytes)).ok()),
            Ok(n) => {
                let Some(part) = chunk.get(..n) else {
                    return Arrival::Finished(None);
                };
                if self.bytes.len().saturating_add(n) > MAX_PASTE_BYTES {
                    return Arrival::Finished(None);
                }
                self.bytes.extend_from_slice(part);
                Arrival::Wait
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Arrival::Wait
            }
            Err(_) => Arrival::Finished(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    fn kind(kind: io::ErrorKind) -> io::Result<usize> {
        Err(io::Error::from(kind))
    }

    #[test]
    fn each_read_result_decides_the_next_step() {
        let cases: Vec<(&str, &[u8], io::Result<usize>, Arrival)> = vec![
            ("some text", &b"hi"[..], Ok(2), Arrival::Wait),
            (
                "pipe empty for now",
                &b""[..],
                kind(io::ErrorKind::WouldBlock),
                Arrival::Wait,
            ),
            (
                "signal",
                &b""[..],
                kind(io::ErrorKind::Interrupted),
                Arrival::Wait,
            ),
            (
                "pipe failed",
                &b""[..],
                kind(io::ErrorKind::BrokenPipe),
                Arrival::Finished(None),
            ),
            (
                "a count past the chunk",
                &b"hi"[..],
                Ok(3),
                Arrival::Finished(None),
            ),
        ];
        for (name, chunk, result, want) in cases {
            assert_eq!(Incoming::default().received(chunk, result), want, "{name}");
        }
    }

    #[test]
    fn the_owner_closing_the_pipe_publishes_what_it_sent() {
        let mut incoming = Incoming::default();
        assert_eq!(incoming.received(b"hel", Ok(3)), Arrival::Wait);
        assert_eq!(incoming.received(b"lo", Ok(2)), Arrival::Wait);
        assert_eq!(
            incoming.received(b"", Ok(0)),
            Arrival::Finished(Some("hello".into()))
        );
        assert_eq!(
            Incoming::default().received(b"", Ok(0)),
            Arrival::Finished(Some(String::new())),
            "an owner that sends nothing offers empty text"
        );
        let mut bad = Incoming::default();
        assert_eq!(bad.received(&[0xff, 0xfe], Ok(2)), Arrival::Wait);
        assert_eq!(
            bad.received(b"", Ok(0)),
            Arrival::Finished(None),
            "bytes that are not text"
        );
    }

    #[test]
    fn an_owner_that_never_stops_is_cut_off_at_the_cap() {
        let (mut reader, mut writer) = std::io::pipe().expect("a pipe");
        let writing = std::thread::spawn(move || {
            let chunk = [b'a'; READ_CHUNK];
            // Writes until the reader closes its end (BrokenPipe), however far that is.
            while writer.write_all(&chunk).is_ok() {}
        });
        let mut incoming = Incoming::default();
        let mut buf = [0u8; READ_CHUNK];
        let mut taken = 0usize;
        let outcome = loop {
            let result = reader.read(&mut buf);
            taken += *result.as_ref().unwrap_or(&0);
            match incoming.received(&buf, result) {
                Arrival::Wait => {}
                finished => break finished,
            }
        };
        drop(reader);
        writing
            .join()
            .expect("the writer stops once the reader closes");
        assert_eq!(outcome, Arrival::Finished(None));
        assert!(
            taken <= MAX_PASTE_BYTES + READ_CHUNK,
            "{taken} bytes were read before the cut"
        );
    }
}
