//! The palmrest socket's messages (design/12 §12.3.10), re-declared from palmrest's
//! `crates/palmrest/src/protocol.rs` (master 5baeb41) so shell-host does not depend on the
//! daemon's crate. The serde attributes are palmrest's exactly: one JSON message per
//! `SOCK_SEQPACKET` packet, enums adjacently tagged (`{"kind":"scroll","v":{...}}`), every
//! name `snake_case`, at most [`MAX_PACKET`] bytes. The first packet of a connection is
//! `Hello`, whose `version` must be [`PROTOCOL_VERSION`]. The tests hold byte strings produced
//! by palmrest's own types: a change on either side fails them.

use serde::{Deserialize, Serialize};

/// The protocol version this host speaks (`Hello.version`).
pub const PROTOCOL_VERSION: u16 = 1;
/// No message is larger than this; a longer packet is not one.
pub const MAX_PACKET: usize = 4096;

/// A gesture, stable from MayBegin to Ended or Cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GestureId(pub u32);

/// The phases palmrest emits (design/12 §12.3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    MayBegin,
    Began,
    Changed,
    Ended,
    Cancelled,
}

/// A locked scroll axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
}

/// Which double-tap fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapKind {
    One,
    Two,
}

/// Whether any finger is on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTouch {
    Down,
    Up,
}

/// Whether the pointer is over one of this subscriber's surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Over {
    Ours,
    NotOurs,
}

/// The mouse palmrest reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub path: String,
    pub vendor: u16,
    pub product: u16,
}

/// A kind of message a subscriber may ask for (`Hello` and `DeviceGone` always arrive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Scroll,
    Swipe,
    Tap,
    Touches,
}

/// What a subscriber receives. The host never narrows it (the default is everything).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Filter {
    All,
    Only(Vec<Topic>),
}

/// palmrest to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToHost {
    Hello {
        version: u16,
        device: DeviceInfo,
    },
    /// Finger-direction px, gains applied; `t_ns` is `CLOCK_MONOTONIC`.
    Scroll {
        gesture: GestureId,
        phase: Phase,
        dx_px: f32,
        dy_px: f32,
        fingers: u8,
        axis: Option<Axis>,
        t_ns: u64,
    },
    Swipe {
        gesture: GestureId,
        phase: Phase,
        fingers: u8,
        dx_units: f32,
        t_ns: u64,
    },
    Tap {
        kind: TapKind,
        t_ns: u64,
    },
    Touches {
        active: ActiveTouch,
    },
    DeviceGone,
}

/// Host to palmrest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToDaemon {
    PointerOver { over: Over },
    Subscribe { filter: Filter },
}

/// One packet's message; `None` for a packet over [`MAX_PACKET`] or not a message of this
/// protocol (a newer daemon's).
pub fn decode(bytes: &[u8]) -> Option<ToHost> {
    if bytes.len() > MAX_PACKET {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}

/// One packet for `message`.
pub fn encode(message: &ToDaemon) -> Vec<u8> {
    // Plain data with string keys: serialization cannot fail.
    serde_json::to_vec(message).unwrap_or_default()
}

/// Whether a connection that opened with `message` may be read on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compat {
    Compatible,
    /// A `Hello` of another protocol version.
    Incompatible {
        version: u16,
    },
}

/// `message`'s verdict on the connection: only a `Hello` of another version is incompatible.
pub fn compat(message: &ToHost) -> Compat {
    match message {
        ToHost::Hello { version, .. } if *version != PROTOCOL_VERSION => {
            Compat::Incompatible { version: *version }
        }
        _ => Compat::Compatible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte strings printed by palmrest's own `protocol::encode` from a throwaway
    /// binary depending on the palmrest crate.
    const FROM_PALMREST: &[&str] = &[
        r#"{"kind":"hello","v":{"version":1,"device":{"name":"Magic Mouse","path":"/dev/hidraw3","vendor":76,"product":617}}}"#,
        r#"{"kind":"scroll","v":{"gesture":7,"phase":"changed","dx_px":0.0,"dy_px":-12.25,"fingers":1,"axis":"y","t_ns":123}}"#,
        r#"{"kind":"scroll","v":{"gesture":7,"phase":"may_begin","dx_px":0.0,"dy_px":0.0,"fingers":2,"axis":null,"t_ns":1}}"#,
        r#"{"kind":"swipe","v":{"gesture":8,"phase":"began","fingers":2,"dx_units":3.5,"t_ns":5}}"#,
        r#"{"kind":"tap","v":{"kind":"two","t_ns":9}}"#,
        r#"{"kind":"touches","v":{"active":"down"}}"#,
        r#"{"kind":"touches","v":{"active":"up"}}"#,
        r#"{"kind":"device_gone"}"#,
    ];

    fn ours() -> Vec<ToHost> {
        vec![
            ToHost::Hello {
                version: 1,
                device: DeviceInfo {
                    name: "Magic Mouse".into(),
                    path: "/dev/hidraw3".into(),
                    vendor: 0x4c,
                    product: 0x269,
                },
            },
            ToHost::Scroll {
                gesture: GestureId(7),
                phase: Phase::Changed,
                dx_px: 0.0,
                dy_px: -12.25,
                fingers: 1,
                axis: Some(Axis::Y),
                t_ns: 123,
            },
            ToHost::Scroll {
                gesture: GestureId(7),
                phase: Phase::MayBegin,
                dx_px: 0.0,
                dy_px: 0.0,
                fingers: 2,
                axis: None,
                t_ns: 1,
            },
            ToHost::Swipe {
                gesture: GestureId(8),
                phase: Phase::Began,
                fingers: 2,
                dx_units: 3.5,
                t_ns: 5,
            },
            ToHost::Tap {
                kind: TapKind::Two,
                t_ns: 9,
            },
            ToHost::Touches {
                active: ActiveTouch::Down,
            },
            ToHost::Touches {
                active: ActiveTouch::Up,
            },
            ToHost::DeviceGone,
        ]
    }

    #[test]
    fn palmrests_bytes_decode_into_these_types() {
        for (json, want) in FROM_PALMREST.iter().zip(ours()) {
            assert_eq!(decode(json.as_bytes()), Some(want.clone()), "{json}");
            let back = serde_json::to_string(&want).expect("encodes");
            assert_eq!(&back, json, "and encode back byte for byte");
        }
    }

    #[test]
    fn to_daemon_encodes_byte_identically_to_palmrest() {
        let cases = [
            (
                ToDaemon::PointerOver { over: Over::Ours },
                r#"{"kind":"pointer_over","v":{"over":"ours"}}"#,
            ),
            (
                ToDaemon::PointerOver {
                    over: Over::NotOurs,
                },
                r#"{"kind":"pointer_over","v":{"over":"not_ours"}}"#,
            ),
            (
                ToDaemon::Subscribe {
                    filter: Filter::All,
                },
                r#"{"kind":"subscribe","v":{"filter":{"kind":"all"}}}"#,
            ),
            (
                ToDaemon::Subscribe {
                    filter: Filter::Only(vec![Topic::Scroll, Topic::Touches]),
                },
                r#"{"kind":"subscribe","v":{"filter":{"kind":"only","v":["scroll","touches"]}}}"#,
            ),
        ];
        for (message, want) in cases {
            assert_eq!(encode(&message), want.as_bytes(), "{want}");
        }
    }

    #[test]
    fn junk_oversized_and_other_versions() {
        assert_eq!(decode(b"{\"kind\":\"pinch\"}"), None);
        let big = format!(
            r#"{{"kind":"hello","v":{{"version":1,"device":{{"name":"{}","path":"","vendor":0,"product":0}}}}}}"#,
            "m".repeat(MAX_PACKET)
        );
        assert_eq!(decode(big.as_bytes()), None);
        let hello = |version| ToHost::Hello {
            version,
            device: DeviceInfo {
                name: String::new(),
                path: String::new(),
                vendor: 0,
                product: 0,
            },
        };
        assert_eq!(compat(&hello(1)), Compat::Compatible);
        assert_eq!(compat(&hello(2)), Compat::Incompatible { version: 2 });
        assert_eq!(compat(&ToHost::DeviceGone), Compat::Compatible);
    }
}
