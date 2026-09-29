//! A `NetProvider` that serves local resources only: `data:` URIs and `file:` URLs. Blitz loads
//! `@font-face`, `mask-image` and background images only through a net provider; a shell surface
//! never fetches from the network, but the wallpaper is `background-image: url(file:///…)`.

use std::path::{Path, PathBuf};

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request, Url};

/// The local loader: `data:` decoded, `file:` read from disk. Anything else is left unanswered.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalNet;

/// What a request asks [`LocalNet`] for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSource {
    /// A `data:` URI, decoded in full.
    Data(String),
    /// An absolute path named by a `file:` URL (empty host or `localhost`).
    File(PathBuf),
    /// A scheme or a path this provider does not serve.
    Unservable,
}

impl LocalSource {
    /// Classify a request URL.
    pub fn of(url: &Url) -> LocalSource {
        match url.scheme() {
            "data" => LocalSource::Data(url.as_str().to_owned()),
            "file" => match url.to_file_path() {
                Ok(path) if path.is_absolute() => LocalSource::File(path),
                Ok(_) | Err(()) => LocalSource::Unservable,
            },
            _ => LocalSource::Unservable,
        }
    }
}

impl NetProvider for LocalNet {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let resolved = request.url.as_str().to_owned();
        // Both run off the surface's own poll, on the host runtime's blocking pool (the host
        // holds `rt.enter()` on the main thread), so a large asset never stalls a frame.
        match LocalSource::of(&request.url) {
            LocalSource::Data(raw) => {
                tokio::task::spawn_blocking(move || {
                    let bytes = decode_data_url(&raw).unwrap_or_default();
                    handler.bytes(resolved, Bytes::from(bytes));
                });
            }
            LocalSource::File(path) => {
                tokio::task::spawn_blocking(move || match read_file(&path) {
                    Some(bytes) => handler.bytes(resolved, Bytes::from(bytes)),
                    None => eprintln!("shell-host: cannot read {}", path.display()),
                });
            }
            // Nothing else is servable: leave the request unanswered rather than guess.
            LocalSource::Unservable => {}
        }
    }
}

/// The bytes of an absolute path; `None` when it cannot be read.
fn read_file(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Decode a `data:` URI's payload: `data:[<mediatype>][;base64],<data>` (RFC 2397). `None` for
/// anything that is not a well-formed `data:` URI.
fn decode_data_url(uri: &str) -> Option<Vec<u8>> {
    let rest = uri.strip_prefix("data:")?;
    let comma = rest.find(',')?;
    let (meta, payload) = rest.split_at(comma);
    let payload = &payload[1..];
    if meta.ends_with(";base64") {
        base64_decode(payload)
    } else {
        Some(percent_decode(payload))
    }
}

/// A minimal RFC 4648 base64 decoder (standard alphabet, `=` padding). Not in the pinned
/// dependency block, so hand-rolled rather than adding a crate for a handful of small assets.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    let (chunks, _remainder) = clean.as_chunks::<4>();
    for chunk in chunks {
        let pad = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        let mut sextets = [0u32; 4];
        for (slot, &byte) in sextets.iter_mut().zip(chunk) {
            *slot = if byte == b'=' { 0 } else { sextet(byte)? };
        }
        let word = (sextets[0] << 18) | (sextets[1] << 12) | (sextets[2] << 6) | sextets[3];
        let bytes = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Some(out)
}

/// Percent-decode a non-base64 payload (`%XX`); anything else passes through as raw bytes.
fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(value) = hex {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_known_vectors() {
        let cases: &[(&str, &[u8])] = &[
            ("", b""),
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("aGVsbG8=", b"hello"),
        ];
        for (encoded, want) in cases {
            assert_eq!(base64_decode(encoded).unwrap(), *want, "{encoded}");
        }
    }

    #[test]
    fn base64_rejects_malformed_input() {
        assert_eq!(base64_decode("a"), None);
        assert_eq!(base64_decode("a!=="), None);
    }

    #[test]
    fn percent_decode_unescapes_and_passes_through() {
        assert_eq!(percent_decode("Hello%2C%20world%21"), b"Hello, world!");
        assert_eq!(percent_decode("plain text"), b"plain text");
    }

    #[test]
    fn data_url_splits_metadata_from_payload() {
        assert_eq!(
            decode_data_url("data:text/plain;base64,aGVsbG8=").unwrap(),
            b"hello"
        );
        assert_eq!(
            decode_data_url("data:text/plain,hello%20world").unwrap(),
            b"hello world"
        );
        assert_eq!(decode_data_url("http://example.com"), None);
    }

    #[test]
    fn requests_classify_by_scheme_and_path() {
        let cases: &[(&str, LocalSource)] = &[
            (
                "data:text/plain,hi",
                LocalSource::Data("data:text/plain,hi".into()),
            ),
            (
                "file:///usr/share/backgrounds/a.png",
                LocalSource::File("/usr/share/backgrounds/a.png".into()),
            ),
            (
                "file://localhost/tmp/a%20b.png",
                LocalSource::File("/tmp/a b.png".into()),
            ),
            ("file://elsewhere/tmp/a.png", LocalSource::Unservable),
            ("https://example.com/a.png", LocalSource::Unservable),
        ];
        for (url, want) in cases {
            let parsed: Url = url.parse().expect(url);
            assert_eq!(LocalSource::of(&parsed), *want, "{url}");
        }
    }
}
