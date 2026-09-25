//! The geometry of a client-decorated toplevel: the visible frame (the document) inside a
//! surface that is larger by an invisible resize band on every side. Pure; the configure
//! handler, the paint and the pointer handler apply it.
//!
//! - The surface (and so the buffer) is the frame plus the band: the band is transparent pixels
//!   the host owns, so a press there can start a resize without a second surface.
//! - `xdg_surface.set_window_geometry` is the frame alone. xdg-shell measures a toplevel by its
//!   window geometry: configure sizes, min and max sizes, maximize, tiling, snapping and the
//!   compositor's shadow all refer to it, so the band must stay outside it or a maximized window
//!   would keep a 7 px gap and a tiled one would overlap its neighbour by 14.
//! - The document lays out and paints in the frame only: its viewport is the frame's size and
//!   its origin is the frame's corner, so element rectangles are window-geometry coordinates,
//!   which is what `xdg_positioner` anchors popups against.
//! - The band exists only while the window floats; maximized, fullscreen or tiled, the surface
//!   is the frame (there is nothing to resize against an output edge or a neighbour).

use cursor_icon::CursorIcon;

use crate::input::pointer::PointerPos;
use crate::region::{Px, Rect};
use crate::scale::{Scale, Size, logical_to_physical};
use crate::toplevel::{Decorations, Fullscreen, Maximize, ResizeEdge, Tile, ToplevelState};

/// The band's width on each side, in logical pixels; [`Inset::NONE`] for every surface but a
/// floating client-decorated toplevel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Inset(pub Px);

impl Inset {
    pub const NONE: Inset = Inset(0);
}

/// Where a pointer on the surface is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// On the frame: the document's.
    Document,
    /// In the band, over the edge or corner a press there resizes.
    Edge(ResizeEdge),
}

/// The band a window with `decorations` has in `state`.
pub fn inset_for(decorations: Decorations, state: ToplevelState) -> Inset {
    let floating = state.maximized == Maximize::Unset
        && state.fullscreen == Fullscreen::Unset
        && state.tiled.any() == Tile::Free;
    match decorations {
        Decorations::Client(margin) if floating => Inset(Px::try_from(margin.0).unwrap_or(Px::MAX)),
        Decorations::Client(_) | Decorations::Server => Inset::NONE,
    }
}

/// The surface around a frame of `frame`.
pub fn surface_of(frame: Size, inset: Inset) -> Size {
    Size {
        width: frame.width + 2 * inset.0,
        height: frame.height + 2 * inset.0,
    }
}

/// The frame inside a surface of `surface`.
pub fn frame_of(surface: Size, inset: Inset) -> Size {
    Size {
        width: (surface.width - 2 * inset.0).max(0),
        height: (surface.height - 2 * inset.0).max(0),
    }
}

/// `xdg_surface.set_window_geometry` for a surface of `surface`: the frame, in surface
/// coordinates.
pub fn window_geometry(surface: Size, inset: Inset) -> Rect {
    let frame = frame_of(surface, inset);
    Rect {
        x: inset.0,
        y: inset.0,
        width: frame.width,
        height: frame.height,
    }
}

/// Where the document paints in a buffer of `buffer` physical pixels: the frame, in physical
/// pixels. The offset is the band rounded to whole device pixels (blitz paints at an integer
/// offset); the size is what is left, so the band is equally wide on both sides.
pub fn content_px(buffer: Size, inset: Inset, scale: Scale) -> Rect {
    let offset = logical_to_physical(inset.0, scale);
    Rect {
        x: offset,
        y: offset,
        width: (buffer.width - 2 * offset).max(0),
        height: (buffer.height - 2 * offset).max(0),
    }
}

/// A surface-local pointer position in the document's coordinates: less the painted offset,
/// so a hit test agrees with the pixels to the device pixel.
pub fn to_document(pos: PointerPos, inset: Inset, scale: Scale) -> PointerPos {
    let offset = f64::from(logical_to_physical(inset.0, scale)) / scale.as_f64();
    PointerPos {
        x: pos.x - offset,
        y: pos.y - offset,
    }
}

/// Document-coordinate rectangles in surface coordinates.
pub fn to_surface(rects: Vec<Rect>, inset: Inset) -> Vec<Rect> {
    rects
        .into_iter()
        .map(|rect| Rect {
            x: rect.x + inset.0,
            y: rect.y + inset.0,
            ..rect
        })
        .collect()
}

/// The band as input-region strips: full-width top and bottom (so the corners are covered),
/// left and right between them. Empty without a band.
pub fn band(surface: Size, inset: Inset) -> Vec<Rect> {
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
pub fn zone_at(surface: Size, inset: Inset, pos: PointerPos) -> Zone {
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
pub fn cursor_for(edge: ResizeEdge) -> CursorIcon {
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
    use crate::toplevel::{ResizeMargin, TileEdges};

    const SURFACE: Size = Size {
        width: 414,
        height: 314,
    };
    const BAND: Inset = Inset(7);

    fn at(x: f64, y: f64) -> PointerPos {
        PointerPos { x, y }
    }

    #[test]
    fn only_a_floating_client_window_has_a_band() {
        let floating = ToplevelState::default();
        let cases = [
            ("floating CSD", Decorations::CLIENT, floating, Inset(7)),
            (
                "custom margin",
                Decorations::Client(ResizeMargin(12)),
                floating,
                Inset(12),
            ),
            (
                "server decorations",
                Decorations::Server,
                floating,
                Inset(0),
            ),
            (
                "maximized",
                Decorations::CLIENT,
                ToplevelState {
                    maximized: Maximize::Set,
                    ..floating
                },
                Inset(0),
            ),
            (
                "fullscreen",
                Decorations::CLIENT,
                ToplevelState {
                    fullscreen: Fullscreen::Set,
                    ..floating
                },
                Inset(0),
            ),
            (
                "tiled left",
                Decorations::CLIENT,
                ToplevelState {
                    tiled: TileEdges {
                        left: Tile::Tiled,
                        ..TileEdges::default()
                    },
                    ..floating
                },
                Inset(0),
            ),
        ];
        for (name, decorations, state, want) in cases {
            assert_eq!(inset_for(decorations, state), want, "{name}");
        }
    }

    #[test]
    fn the_window_geometry_is_the_frame_and_excludes_the_band() {
        let frame = Size {
            width: 400,
            height: 300,
        };
        let surface = surface_of(frame, BAND);
        assert_eq!(surface, SURFACE);
        assert_eq!(frame_of(surface, BAND), frame);
        assert_eq!(
            window_geometry(surface, BAND),
            Rect {
                x: 7,
                y: 7,
                width: 400,
                height: 300
            }
        );
        assert_eq!(
            window_geometry(frame, Inset::NONE),
            Rect {
                x: 0,
                y: 0,
                width: 400,
                height: 300
            },
            "no band: the whole surface"
        );
    }

    #[test]
    fn the_content_is_inset_by_whole_device_pixels_on_both_sides() {
        let cases = [
            (Scale::ONE, 414, 7, 400),
            (Scale(180), 621, 11, 599),
            (Scale(240), 828, 14, 800),
        ];
        for (scale, buffer_width, offset, width) in cases {
            let buffer = Size {
                width: buffer_width,
                height: buffer_width,
            };
            let content = content_px(buffer, BAND, scale);
            assert_eq!((content.x, content.width), (offset, width), "{scale:?}");
        }
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
    fn pointer_and_rects_move_between_surface_and_document_coordinates() {
        let doc = to_document(at(17.0, 27.0), BAND, Scale::ONE);
        assert_eq!(doc, at(10.0, 20.0));
        let back = to_surface(
            vec![Rect {
                x: 10,
                y: 20,
                width: 5,
                height: 5,
            }],
            BAND,
        );
        assert_eq!((back[0].x, back[0].y), (17, 27));
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
