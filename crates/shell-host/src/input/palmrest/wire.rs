//! The palmrest socket's messages (design/12 §12.3.10), re-declared here so shell-host does not
//! depend on the daemon's crate. One message per `SOCK_SEQPACKET` datagram, encoded as JSON in
//! serde's default (externally tagged) form of these exact declarations: a variant is
//! `{"Scroll":{...}}`, a unit variant `"DeviceGone"`, `Option` is `null`. design/12 names the
//! types but not an encoding; this one is proposed back to it (FINDINGS F64).

use serde::{Deserialize, Serialize};

/// A gesture, stable from MayBegin to Ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GestureId(pub u64);

/// The phases palmrest emits (design/12 §12.3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    MayBegin,
    Began,
    Changed,
    Ended,
    Cancelled,
}

/// A locked scroll axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
}

/// Which tap fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapKind {
    One,
    Two,
}

/// Whether any finger is on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveTouch {
    Down,
    Up,
}

/// Whether the pointer is over one of this subscriber's surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Over {
    Ours,
    NotOurs,
}

/// Which events a subscriber wants (design/12 leaves it open; unused by the host, which takes
/// everything).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    All,
}

/// The device palmrest reads (design/12 names it without fields; every field defaults, so a
/// daemon sending fewer or more still parses).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceInfo {
    pub name: String,
    pub vendor: u16,
    pub product: u16,
}

/// palmrest to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToDaemon {
    PointerOver { over: Over },
    Subscribe { filter: Filter },
}

/// One datagram's message; `None` for bytes that are not one (a newer daemon's message).
pub fn decode(bytes: &[u8]) -> Option<ToHost> {
    serde_json::from_slice(bytes).ok()
}

/// One datagram for `message`.
pub fn encode(message: &ToDaemon) -> Vec<u8> {
    // A plain enum of plain fields: serialization cannot fail.
    serde_json::to_vec(message).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_form_is_serdes_default_of_12_3_10() {
        let cases: &[(&str, ToHost)] = &[
            (
                r#"{"Scroll":{"gesture":7,"phase":"Changed","dx_px":0.0,"dy_px":12.5,"fingers":1,"axis":"Y","t_ns":1000}}"#,
                ToHost::Scroll {
                    gesture: GestureId(7),
                    phase: Phase::Changed,
                    dx_px: 0.0,
                    dy_px: 12.5,
                    fingers: 1,
                    axis: Some(Axis::Y),
                    t_ns: 1000,
                },
            ),
            (
                r#"{"Touches":{"active":"Down"}}"#,
                ToHost::Touches {
                    active: ActiveTouch::Down,
                },
            ),
            (r#""DeviceGone""#, ToHost::DeviceGone),
            (
                r#"{"Hello":{"version":1,"device":{"name":"Magic Mouse"}}}"#,
                ToHost::Hello {
                    version: 1,
                    device: DeviceInfo {
                        name: "Magic Mouse".into(),
                        ..DeviceInfo::default()
                    },
                },
            ),
        ];
        for (json, want) in cases {
            assert_eq!(decode(json.as_bytes()).as_ref(), Some(want), "{json}");
            let back = serde_json::to_string(want).expect("encodes");
            assert_eq!(
                decode(back.as_bytes()).as_ref(),
                Some(want),
                "round trip {json}"
            );
        }
        assert_eq!(
            decode(b"{\"Pinch\":{}}"),
            None,
            "an unknown message is skipped"
        );
        assert_eq!(
            encode(&ToDaemon::PointerOver { over: Over::Ours }),
            br#"{"PointerOver":{"over":"Ours"}}"#.to_vec()
        );
    }
}
