//! What a component asks to drag (`SurfaceHandle::start_drag`).

use std::fmt;

use wayland_client::protocol::wl_data_device_manager::DndAction;

use crate::root::HostRoot;
use crate::scale::Size;

/// One drag: what it carries, what a target may do with it, and what follows the pointer.
#[derive(Debug)]
pub struct DragSpec {
    pub payload: DragPayload,
    pub actions: DragActions,
    pub icon: DragIcon,
}

/// What a drop target receives, offered under the mime types that describe it
/// (`drag::offer::offers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragPayload {
    /// Files or other resources, as URIs (`file:///tmp/shot.png`): offered as `text/uri-list`
    /// (CRLF-terminated lines, RFC 2483) and as plain text, one URI per line.
    Uris(Vec<String>),
    /// Text, offered under the same types a paste accepts (`clipboard::TEXT_MIME`).
    Text(String),
    /// Anything else, offered under exactly one mime type.
    Bytes { mime: Mime, bytes: Vec<u8> },
}

/// A mime type, `type/subtype` with optional parameters (`text/plain;charset=utf-8`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mime(String);

/// Why a string is not a mime type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a mime type: {0:?}")]
pub struct BadMime(pub String);

impl Mime {
    /// `text` as a mime type: a non-empty type and subtype around one `/`, and no whitespace
    /// or control characters (a `wl_data_source.offer` string is matched whole by the target).
    pub fn parse(text: &str) -> Result<Mime, BadMime> {
        let essence = text.split(';').next().unwrap_or_default();
        let well_formed = matches!(
            essence.split_once('/'),
            Some((kind, sub)) if !kind.is_empty() && !sub.is_empty() && !sub.contains('/')
        ) && !text.chars().any(|c| c.is_whitespace() || c.is_control());
        if well_formed {
            Ok(Mime(text.to_owned()))
        } else {
            Err(BadMime(text.to_owned()))
        }
    }

    /// The mime type as offered on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a drop target may do with the payload (`wl_data_source.set_actions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragActions {
    /// The source keeps its data.
    Copy,
    /// The target takes the data and the source deletes its own (when the outcome says so).
    Move,
    /// The target (or its user) chooses.
    CopyOrMove,
}

impl DragActions {
    /// The wire action set.
    pub fn to_wire(self) -> DndAction {
        match self {
            DragActions::Copy => DndAction::Copy,
            DragActions::Move => DndAction::Move,
            DragActions::CopyOrMove => DndAction::Copy | DndAction::Move,
        }
    }
}

/// What follows the pointer during the drag.
#[derive(Debug)]
pub enum DragIcon {
    /// Nothing: the compositor shows only its drag cursor.
    None,
    /// `root` rendered at `size` (logical px) on a `wl_surface` with the `dnd_icon` role, its
    /// top-left corner at the pointer's hotspot.
    Root { root: HostRoot, size: Size },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mime_type_has_a_type_and_a_subtype() {
        const CASES: &[(&str, bool)] = &[
            ("text/uri-list", true),
            ("text/plain;charset=utf-8", true),
            ("application/x-sill-tile", true),
            ("UTF8_STRING", false),
            ("text/", false),
            ("/plain", false),
            ("text/plain/extra", false),
            ("text/plain; charset=utf-8", false),
            ("text/plain\n", false),
            ("", false),
        ];
        for &(text, valid) in CASES {
            assert_eq!(Mime::parse(text).is_ok(), valid, "{text:?}");
        }
        assert_eq!(
            Mime::parse("image/png").map(|mime| mime.to_string()),
            Ok("image/png".to_owned())
        );
    }

    #[test]
    fn actions_become_the_wire_set() {
        let cases = [
            (DragActions::Copy, DndAction::Copy),
            (DragActions::Move, DndAction::Move),
            (DragActions::CopyOrMove, DndAction::Copy | DndAction::Move),
        ];
        for (actions, wire) in cases {
            assert_eq!(actions.to_wire(), wire, "{actions:?}");
        }
    }
}
