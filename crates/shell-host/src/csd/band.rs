//! The resize band: its input-region strips, the zone a pointer is in, and the cursor for it.

use cursor_icon::CursorIcon;

use super::Inset;
use crate::input::pointer::PointerPos;
use crate::region::Rect;
use crate::scale::Size;
use crate::toplevel::ResizeEdge;

/// Where a pointer on the surface is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zone {
    /// On the frame: the document's.
    Document,
    /// In the band, over the edge or corner a press there resizes.
    Edge(ResizeEdge),
}

/// The band as input-region strips: full-width top and bottom (so the corners are covered),
/// left and right between them. Empty without a band.
pub(crate) fn band(surface: Size, inset: Inset) -> Vec<Rect> {
    let (w, h, i) = (surface.width, surface.height, inset.0);
    let strips = [
        Rect {
            x: 0,
            y: 0,
            width: w,
            height: i,
        },
        Rect {
            x: 0,
            y: h - i,
            width: w,
            height: i,
        },
        Rect {
            x: 0,
            y: i,
            width: i,
            height: h - 2 * i,
        },
        Rect {
            x: w - i,
            y: i,
            width: i,
            height: h - 2 * i,
        },
    ];
    strips.into_iter().filter(|rect| rect.has_area()).collect()
}

/// Where surface-local `pos` is on a surface of `surface` with band `inset`. A corner reaches
/// one band width further along each side than the band's own corner square, since a square
/// the band's width on a side is too small a target to hit reliably.
pub(crate) fn zone_at(surface: Size, inset: Inset, pos: PointerPos) -> Zone {
    let i = f64::from(inset.0);
    let (w, h) = (f64::from(surface.width), f64::from(surface.height));
    let (in_left, in_right) = (pos.x < i, pos.x >= w - i);
    let (in_top, in_bottom) = (pos.y < i, pos.y >= h - i);
    if inset == Inset::NONE || !(in_left || in_right || in_top || in_bottom) {
        return Zone::Document;
    }
    let reach = 2.0 * i;
    let sides = in_left || in_right;
    let ends = in_top || in_bottom;
    let top = in_top || (sides && pos.y < reach);
    let bottom = !top && (in_bottom || (sides && pos.y >= h - reach));
    let left = in_left || (ends && pos.x < reach);
    let right = !left && (in_right || (ends && pos.x >= w - reach));
    let edge = match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, _, _, _) => ResizeEdge::Top,
        (_, true, _, _) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        _ => ResizeEdge::Right,
    };
    Zone::Edge(edge)
}

/// The resize cursor for `edge` (CSS names: `ns-resize`, `ew-resize`, `nwse-resize`,
/// `nesw-resize`); the bidirectional shapes, as a resize can grow or shrink.
pub(crate) fn cursor_for(edge: ResizeEdge) -> CursorIcon {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorIcon::NsResize,
        ResizeEdge::Left | ResizeEdge::Right => CursorIcon::EwResize,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorIcon::NwseResize,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorIcon::NeswResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: Size = Size {
        width: 414,
        height: 314,
    };
    const BAND: Inset = Inset(7);

    fn at(x: f64, y: f64) -> PointerPos {
        PointerPos { x, y }
    }

    #[test]
    fn the_band_covers_the_whole_ring_and_nothing_inside() {
        let strips = band(SURFACE, BAND);
        let area: i32 = strips.iter().map(|r| r.width * r.height).sum();
        assert_eq!(area, 414 * 314 - 400 * 300);
        for corner in [(0, 0), (413, 0), (0, 313), (413, 313)] {
            assert!(
                strips.iter().any(|r| r.x <= corner.0
                    && corner.0 < r.x + r.width
                    && r.y <= corner.1
                    && corner.1 < r.y + r.height),
                "corner {corner:?} is covered"
            );
        }
        assert!(band(SURFACE, Inset::NONE).is_empty());
    }

    #[test]
    fn each_part_of_the_band_resizes_its_edge() {
        use ResizeEdge as E;
        let cases = [
            ("frame", at(200.0, 150.0), Zone::Document),
            ("frame's first pixel", at(7.0, 7.0), Zone::Document),
            ("top", at(200.0, 3.0), Zone::Edge(E::Top)),
            ("bottom", at(200.0, 310.0), Zone::Edge(E::Bottom)),
            ("left", at(2.0, 150.0), Zone::Edge(E::Left)),
            ("right", at(413.0, 150.0), Zone::Edge(E::Right)),
            ("top-left square", at(1.0, 1.0), Zone::Edge(E::TopLeft)),
            ("top-right square", at(410.0, 2.0), Zone::Edge(E::TopRight)),
            (
                "bottom-left square",
                at(3.0, 311.0),
                Zone::Edge(E::BottomLeft),
            ),
            (
                "bottom-right square",
                at(412.0, 312.0),
                Zone::Edge(E::BottomRight),
            ),
            (
                "top band, within reach of the left",
                at(12.0, 2.0),
                Zone::Edge(E::TopLeft),
            ),
            (
                "right band, within reach of the bottom",
                at(411.0, 302.0),
                Zone::Edge(E::BottomRight),
            ),
            (
                "top band, past the reach",
                at(15.0, 2.0),
                Zone::Edge(E::Top),
            ),
            (
                "outside the surface (a grab in progress)",
                at(-3.0, 150.0),
                Zone::Edge(E::Left),
            ),
        ];
        for (name, pos, want) in cases {
            assert_eq!(zone_at(SURFACE, BAND, pos), want, "{name}");
        }
        assert_eq!(
            zone_at(SURFACE, Inset::NONE, at(1.0, 1.0)),
            Zone::Document,
            "no band"
        );
    }

    #[test]
    fn corners_take_the_diagonal_cursors() {
        let cases = [
            (ResizeEdge::Top, CursorIcon::NsResize),
            (ResizeEdge::Left, CursorIcon::EwResize),
            (ResizeEdge::BottomRight, CursorIcon::NwseResize),
            (ResizeEdge::TopRight, CursorIcon::NeswResize),
        ];
        for (edge, want) in cases {
            assert_eq!(cursor_for(edge), want, "{edge:?}");
        }
    }
}
