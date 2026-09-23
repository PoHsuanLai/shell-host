//! A `NetProvider` that resolves `data:` URIs and nothing else. Blitz loads `@font-face`,
//! `mask-image` and background images only through a net provider (quire FINDINGS S7, S8); a
//! shell surface never fetches from the network.

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};

/// The data: URI loader.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataUriNet;

impl NetProvider for DataUriNet {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        if request.url.scheme() != "data" {
            // Nothing else is servable: leave the request unanswered rather than guess.
            return;
        }
        let raw = request.url.as_str().to_owned();
        let resolved = raw.clone();
        // Decoding is cheap (small bundled assets), but runs off the surface's own poll so a
        // large data URI never stalls a frame; the host holds `rt.enter()` on the main thread.
        tokio::task::spawn_blocking(move || {
            let bytes = decode_data_url(&raw).unwrap_or_default();
            handler.bytes(resolved, Bytes::from(bytes));
        });
    }
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
}
