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

mod band;
mod geometry;

pub(crate) use band::{Zone, band, cursor_for, zone_at};
pub(crate) use geometry::{
    content_px, frame_of, surface_of, to_document, to_surface, window_geometry,
};

use crate::region::Px;
use crate::toplevel::{Decorations, Fullscreen, Maximize, Tile, ToplevelState};

/// The band's width on each side, in logical pixels; [`Inset::NONE`] for every surface but a
/// floating client-decorated toplevel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct Inset(pub Px);

impl Inset {
    pub(crate) const NONE: Inset = Inset(0);
}

/// The band a window with `decorations` has in `state`.
pub(crate) fn inset_for(decorations: Decorations, state: ToplevelState) -> Inset {
    let floating = state.maximized == Maximize::Unset
        && state.fullscreen == Fullscreen::Unset
        && state.tiled.any() == Tile::Free;
    match decorations {
        Decorations::Client(margin) if floating => Inset(Px::try_from(margin.0).unwrap_or(Px::MAX)),
        Decorations::Client(_) | Decorations::Server => Inset::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toplevel::{ResizeMargin, TileEdges};

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
}
