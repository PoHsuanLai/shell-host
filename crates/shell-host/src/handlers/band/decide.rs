//! The decisions behind the pointer handler's half of a client-decorated window: which zone the
//! pointer is in, where a motion goes and which cursor shows.

use blitz_traits::events::MouseEventButtons;
use cursor_icon::CursorIcon;

use crate::csd::{Zone, cursor_for};

/// The zone the pointer is in after an event: a fresh reading unless a button is held, when a
/// drag keeps the zone it started in (the implicit grab).
pub(super) fn zone_while(held: MouseEventButtons, current: Zone, fresh: Zone) -> Zone {
    match held {
        MouseEventButtons::None => fresh,
        _ => current,
    }
}

/// Where a motion goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MotionRoute {
    /// To the document, as a move.
    Document,
    /// The pointer just crossed into the band: the document is told it left.
    LeftDocument,
    /// Inside the band: the document hears nothing.
    Band,
}

/// The route of a motion into `zone` from `was`.
pub(super) fn motion_route(zone: Zone, was: Zone) -> MotionRoute {
    match (zone, was) {
        (Zone::Document, _) => MotionRoute::Document,
        (Zone::Edge(_), Zone::Document) => MotionRoute::LeftDocument,
        (Zone::Edge(_), Zone::Edge(_)) => MotionRoute::Band,
    }
}

/// The cursor for the pointer in `zone`: the band's resize cursor, or `document`'s own.
pub(super) fn cursor_in(zone: Zone, document: Option<CursorIcon>) -> Option<CursorIcon> {
    match zone {
        Zone::Edge(edge) => Some(cursor_for(edge)),
        Zone::Document => document,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toplevel::ResizeEdge;

    const EDGE: Zone = Zone::Edge(ResizeEdge::Left);
    const OTHER_EDGE: Zone = Zone::Edge(ResizeEdge::Top);

    #[test]
    fn a_held_button_keeps_the_zone_a_drag_started_in() {
        let cases: &[(&str, MouseEventButtons, Zone, Zone, Zone)] = &[
            (
                "no button: the fresh reading",
                MouseEventButtons::None,
                Zone::Document,
                EDGE,
                EDGE,
            ),
            (
                "a drag from the document stays the document's over the band",
                MouseEventButtons::Primary,
                Zone::Document,
                EDGE,
                Zone::Document,
            ),
            (
                "a drag from the band stays the band's over the document",
                MouseEventButtons::Primary,
                EDGE,
                Zone::Document,
                EDGE,
            ),
            (
                "any held button keeps it",
                MouseEventButtons::Secondary,
                EDGE,
                OTHER_EDGE,
                EDGE,
            ),
        ];
        for (name, held, current, fresh, want) in cases {
            assert_eq!(zone_while(*held, *current, *fresh), *want, "{name}");
        }
    }

    #[test]
    fn a_motion_reaches_the_document_or_tells_it_the_pointer_left() {
        let cases: &[(&str, Zone, Zone, MotionRoute)] = &[
            (
                "document to document",
                Zone::Document,
                Zone::Document,
                MotionRoute::Document,
            ),
            (
                "band back to document",
                Zone::Document,
                EDGE,
                MotionRoute::Document,
            ),
            (
                "document into the band",
                EDGE,
                Zone::Document,
                MotionRoute::LeftDocument,
            ),
            ("along the band", EDGE, OTHER_EDGE, MotionRoute::Band),
            ("within one edge", EDGE, EDGE, MotionRoute::Band),
        ];
        for (name, zone, was, want) in cases {
            assert_eq!(motion_route(*zone, *was), *want, "{name}");
        }
    }

    #[test]
    fn the_cursor_is_the_bands_over_the_band_and_the_documents_elsewhere() {
        let cases: &[(&str, Zone, Option<CursorIcon>, Option<CursorIcon>)] = &[
            (
                "left edge resizes horizontally",
                EDGE,
                Some(CursorIcon::Pointer),
                Some(CursorIcon::EwResize),
            ),
            (
                "top edge resizes vertically",
                OTHER_EDGE,
                None,
                Some(CursorIcon::NsResize),
            ),
            (
                "document shows what the document asked for",
                Zone::Document,
                Some(CursorIcon::Pointer),
                Some(CursorIcon::Pointer),
            ),
            (
                "document that asked to hide it hides it",
                Zone::Document,
                None,
                None,
            ),
        ];
        for (name, zone, document, want) in cases {
            assert_eq!(cursor_in(*zone, *document), *want, "{name}");
        }
    }
}
