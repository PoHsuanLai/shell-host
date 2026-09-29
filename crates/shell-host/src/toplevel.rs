//! xdg toplevels: every app window (design/20-SURFACES.md, "All apps are xdg toplevels through
//! shell-host"). The configuration a window opens with, the window-management vocabulary a
//! client-decorated frame asks with (move, resize, zoom, minimize, fullscreen), and the state
//! the compositor answers with in each configure.

use smithay_client_toolkit::shell::xdg::window::{WindowConfigure, WindowDecorations};
use wayland_protocols::xdg::shell::client::xdg_toplevel;

use crate::scale::Size;

/// Everything `HostCtx::spawn_toplevel` needs besides the root component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToplevelConfig {
    /// The `xdg_toplevel.set_app_id` value; also the debug-command name of the window.
    pub app_id: String,
    pub title: String,
    /// The size of the visible frame to open at, in logical pixels, until the compositor
    /// configures another. A `Decorations::Client` window's surface is larger by its resize
    /// margin on every side.
    pub size: Size,
    /// The smallest frame the window accepts; `None` for no limit.
    pub min_size: Option<Size>,
    /// The largest frame the window accepts; `None` for no limit.
    pub max_size: Option<Size>,
    pub decorations: Decorations,
}

/// Who draws the window frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decorations {
    /// The document draws its own titlebar and traffic lights (ds windows); the host adds an
    /// invisible resize band of this width around it.
    Client(ResizeMargin),
    /// Ask the compositor for server-side decorations, falling back to none.
    Server,
}

impl Decorations {
    /// Client-side decorations with the default resize margin.
    pub const CLIENT: Decorations = Decorations::Client(ResizeMargin::DEFAULT);
}

/// The invisible grab band around a client-decorated window, in logical pixels: transparent
/// surface area outside the visible frame whose only purpose is to catch a press that starts a
/// resize, as macOS gives every window a few points of resize area outside its drawn edge. Our
/// frames draw no border wide enough to aim at, so without it a window could be resized only
/// through a compositor shortcut. The host owns the band: the document never sees input there,
/// and the band disappears while the window is maximized, fullscreen or tiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResizeMargin(pub u32);

impl ResizeMargin {
    /// 7 px: wide enough to hit with a mouse without hunting, narrow enough that a click just
    /// outside a window still reaches the one below it most of the time (GTK uses 10 px of
    /// shadow and resizes from all of it; macOS about 5 points).
    pub const DEFAULT: ResizeMargin = ResizeMargin(7);
}

/// The edge or corner a resize drags (`xdg_toplevel.resize_edge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    /// Every edge, for tables.
    pub const ALL: [ResizeEdge; 8] = [
        ResizeEdge::Top,
        ResizeEdge::Bottom,
        ResizeEdge::Left,
        ResizeEdge::Right,
        ResizeEdge::TopLeft,
        ResizeEdge::TopRight,
        ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight,
    ];
}

/// Whether a window is (or should be) maximized: the request of `SurfaceHandle::set_maximized`
/// and the `maximized` field of [`ToplevelState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Maximize {
    Set,
    #[default]
    Unset,
}

/// The wire form: sctk hands the configure's states over as bare `bool`s.
impl From<bool> for Maximize {
    fn from(wire: bool) -> Maximize {
        match wire {
            true => Maximize::Set,
            false => Maximize::Unset,
        }
    }
}

/// Whether a window is (or should be) fullscreen: the request of
/// `SurfaceHandle::set_fullscreen` and the `fullscreen` field of [`ToplevelState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Fullscreen {
    Set,
    #[default]
    Unset,
}

/// The wire form: sctk hands the configure's states over as bare `bool`s.
impl From<bool> for Fullscreen {
    fn from(wire: bool) -> Fullscreen {
        match wire {
            true => Fullscreen::Set,
            false => Fullscreen::Unset,
        }
    }
}

/// Whether a toplevel is the active window, from its last configure. Drives the first-mouse
/// filter (design/13 §13.3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Activation {
    Active,
    #[default]
    Inactive,
}

/// The wire form: sctk hands the configure's states over as bare `bool`s.
impl From<bool> for Activation {
    fn from(wire: bool) -> Activation {
        match wire {
            true => Activation::Active,
            false => Activation::Inactive,
        }
    }
}

/// Whether one edge of a window is tiled against a neighbour or the output edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tile {
    Tiled,
    #[default]
    Free,
}

/// The wire form: sctk hands the configure's states over as bare `bool`s.
impl From<bool> for Tile {
    fn from(wire: bool) -> Tile {
        match wire {
            true => Tile::Tiled,
            false => Tile::Free,
        }
    }
}

/// Which edges of a window the compositor has tiled (`xdg_toplevel.state` `tiled_*`, v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TileEdges {
    pub top: Tile,
    pub bottom: Tile,
    pub left: Tile,
    pub right: Tile,
}

impl TileEdges {
    /// Whether any edge is tiled.
    pub fn any(self) -> Tile {
        let edges = [self.top, self.bottom, self.left, self.right];
        if edges.contains(&Tile::Tiled) {
            Tile::Tiled
        } else {
            Tile::Free
        }
    }
}

/// A toplevel's state as of its last configure: what the frame draws (the zoom button's
/// state, no resize band when maximized) and what `use_toplevel_state` publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ToplevelState {
    pub maximized: Maximize,
    pub fullscreen: Fullscreen,
    pub activated: Activation,
    pub tiled: TileEdges,
}

/// The sctk request for `decorations`.
pub fn to_wire_decorations(decorations: Decorations) -> WindowDecorations {
    match decorations {
        Decorations::Client(_) => WindowDecorations::RequestClient,
        Decorations::Server => WindowDecorations::RequestServer,
    }
}

/// The `xdg_toplevel.resize_edge` for `edge`.
pub fn to_wire_edge(edge: ResizeEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        ResizeEdge::Top => xdg_toplevel::ResizeEdge::Top,
        ResizeEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        ResizeEdge::Left => xdg_toplevel::ResizeEdge::Left,
        ResizeEdge::Right => xdg_toplevel::ResizeEdge::Right,
        ResizeEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        ResizeEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        ResizeEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
}

/// The state a configure carries.
pub fn state_of(configure: &WindowConfigure) -> ToplevelState {
    ToplevelState {
        maximized: Maximize::from(configure.is_maximized()),
        fullscreen: Fullscreen::from(configure.is_fullscreen()),
        activated: Activation::from(configure.is_activated()),
        tiled: TileEdges {
            top: Tile::from(configure.is_tiled_top()),
            bottom: Tile::from(configure.is_tiled_bottom()),
            left: Tile::from(configure.is_tiled_left()),
            right: Tile::from(configure.is_tiled_right()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorations_map_to_sctk_requests() {
        let cases = [
            (Decorations::CLIENT, WindowDecorations::RequestClient),
            (
                Decorations::Client(ResizeMargin(0)),
                WindowDecorations::RequestClient,
            ),
            (Decorations::Server, WindowDecorations::RequestServer),
        ];
        for (ours, wire) in cases {
            assert_eq!(to_wire_decorations(ours), wire, "{ours:?}");
        }
    }

    #[test]
    fn every_edge_maps_to_its_xdg_resize_edge() {
        use xdg_toplevel::ResizeEdge as W;
        let want = [
            W::Top,
            W::Bottom,
            W::Left,
            W::Right,
            W::TopLeft,
            W::TopRight,
            W::BottomLeft,
            W::BottomRight,
        ];
        for (edge, wire) in ResizeEdge::ALL.into_iter().zip(want) {
            assert_eq!(to_wire_edge(edge), wire, "{edge:?}");
        }
    }

    #[test]
    fn one_tiled_edge_is_enough_to_count_as_tiled() {
        let free = TileEdges::default();
        assert_eq!(free.any(), Tile::Free);
        let left = TileEdges {
            left: Tile::Tiled,
            ..free
        };
        assert_eq!(left.any(), Tile::Tiled);
    }
}
