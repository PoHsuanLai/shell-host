//! The mime types a drag's source offers, and the bytes it writes for each
//! (`wl_data_source.offer`, then `send`).

use std::sync::Arc;

use crate::clipboard::TEXT_MIME;
use crate::drag::spec::DragPayload;

/// The mime type a `DragPayload::Uris` is offered under first.
pub const URI_LIST_MIME: &str = "text/uri-list";

/// One offered mime type and the bytes a target asking for it receives. Shared, so a target
/// that asks twice (or two targets) costs no copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub mime: String,
    pub bytes: Arc<[u8]>,
}

/// The mime types `payload` is offered under, most specific first, with each one's bytes.
pub fn offers(payload: &DragPayload) -> Vec<Offered> {
    match payload {
        DragPayload::Uris(uris) => {
            let list: String = uris.iter().map(|uri| format!("{uri}\r\n")).collect();
            let text = uris.join("\n");
            let mut offered = vec![Offered {
                mime: URI_LIST_MIME.to_owned(),
                bytes: Arc::from(list.as_bytes()),
            }];
            offered.extend(text_offers(&text));
            offered
        }
        DragPayload::Text(text) => text_offers(text),
        DragPayload::Bytes { mime, bytes } => vec![Offered {
            mime: mime.as_str().to_owned(),
            bytes: Arc::from(bytes.as_slice()),
        }],
    }
}

/// `text` under every type a paste accepts.
fn text_offers(text: &str) -> Vec<Offered> {
    let bytes: Arc<[u8]> = Arc::from(text.as_bytes());
    TEXT_MIME
        .iter()
        .map(|mime| Offered {
            mime: (*mime).to_owned(),
            bytes: Arc::clone(&bytes),
        })
        .collect()
}

/// The bytes for `mime`, compared whole; `None` for a type that was never offered (a target
/// may ask for anything).
pub fn bytes_for(offered: &[Offered], mime: &str) -> Option<Arc<[u8]>> {
    offered
        .iter()
        .find(|offer| offer.mime == mime)
        .map(|offer| Arc::clone(&offer.bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag::spec::Mime;

    fn mimes(payload: &DragPayload) -> Vec<String> {
        offers(payload)
            .into_iter()
            .map(|offer| offer.mime)
            .collect()
    }

    #[test]
    fn each_payload_is_offered_under_its_types() {
        let png = Mime::parse("image/png").expect("a valid literal");
        let cases = [
            (
                "uris",
                DragPayload::Uris(vec!["file:///tmp/a.png".into()]),
                vec![
                    "text/uri-list",
                    "text/plain;charset=utf-8",
                    "UTF8_STRING",
                    "text/plain",
                ],
            ),
            (
                "text",
                DragPayload::Text("hi".into()),
                vec!["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"],
            ),
            (
                "bytes",
                DragPayload::Bytes {
                    mime: png,
                    bytes: vec![0x89, b'P'],
                },
                vec!["image/png"],
            ),
        ];
        for (name, payload, want) in cases {
            assert_eq!(mimes(&payload), want, "{name}");
        }
    }

    #[test]
    fn a_uri_list_is_crlf_lines_and_its_text_is_newline_joined() {
        let payload = DragPayload::Uris(vec!["file:///tmp/a".into(), "file:///tmp/b".into()]);
        let offered = offers(&payload);
        let list = bytes_for(&offered, "text/uri-list").expect("offered");
        assert_eq!(&*list, b"file:///tmp/a\r\nfile:///tmp/b\r\n");
        let text = bytes_for(&offered, "text/plain").expect("offered");
        assert_eq!(&*text, b"file:///tmp/a\nfile:///tmp/b");
    }

    #[test]
    fn a_type_never_offered_has_no_bytes() {
        let offered = offers(&DragPayload::Text("hi".into()));
        assert_eq!(bytes_for(&offered, "text/uri-list"), None);
        assert_eq!(
            bytes_for(&offered, "text/plain;charset"),
            None,
            "whole types only"
        );
        assert_eq!(
            bytes_for(&offered, "UTF8_STRING").as_deref(),
            Some(&b"hi"[..])
        );
    }
}
