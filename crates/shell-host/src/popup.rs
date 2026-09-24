//! xdg_popup positioning: the pure translation from a `PopupConfig` plus a resolved anchor rect
//! and content size into the values an `xdg_positioner` is configured with.
//!
//! Building and committing the actual `xdg_positioner` object is an effect (it is a Wayland
//! request against a live connection) and belongs to the caller; this module only computes what
//! to send.

use crate::region::{ElementId, Px, Rect};
use crate::scale::Size;
use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity as WireGravity,
};

/// Where a popup is anchored: an explicit rect, or a DOM element resolved after layout.
///
/// Not `Copy`: `ElementId` owns a `String` (FINDINGS.md F3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupAnchor {
    Rect,
    Element(ElementId),
}

/// How a popup's size is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupSize {
    /// Use the size passed to `positioner_params` as-is.
    Fixed,
    /// Clamp the passed-in size to `max` on each axis (content sizes itself, up to a cap).
    FitContent { max: Px },
}

/// Which point of the anchor rectangle the popup is positioned relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    BottomLeft,
    TopRight,
    BottomRight,
}

/// Which direction the popup grows away from its anchor point. Shares `Edge`'s nine positions
/// but is a distinct type: an anchor edge and a gravity direction are never interchangeable,
/// and mixing them up should be a type error, not a rendering bug.
pub type Gravity = Edge;

/// A single-axis toggle. Used instead of `bool` so `Adjust`'s fields read as intent
/// (`slide_x: Toggle::On`) rather than an unlabelled `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Toggle {
    #[default]
    Off,
    On,
}

/// The `xdg_positioner` constraint-adjustment flags: how the compositor may move or resize the
/// popup if its unadjusted position would be (partly) off-screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Adjust {
    pub slide_x: Toggle,
    pub slide_y: Toggle,
    pub flip_x: Toggle,
    pub flip_y: Toggle,
    pub resize_x: Toggle,
    pub resize_y: Toggle,
}

impl Adjust {
    /// No adjustment: the popup stays exactly where positioned, even off-screen.
    pub const NONE: Adjust = Adjust {
        slide_x: Toggle::Off,
        slide_y: Toggle::Off,
        flip_x: Toggle::Off,
        flip_y: Toggle::Off,
        resize_x: Toggle::Off,
        resize_y: Toggle::Off,
    };

    /// Slide on both axes, flip on both axes, and never resize: the common "menu" behaviour.
    pub const SLIDE_AND_FLIP: Adjust = Adjust {
        slide_x: Toggle::On,
        slide_y: Toggle::On,
        flip_x: Toggle::On,
        flip_y: Toggle::On,
        resize_x: Toggle::Off,
        resize_y: Toggle::Off,
    };
}

/// Whether a popup takes an exclusive input grab (dismissing it, and any popups nested inside
/// it, when the compositor sees a click outside the stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    Ungrabbed,
    Grabbed,
}

/// A shift of the popup from where its anchor edge and gravity put it, in the parent's logical
/// pixels: design/13 §13.3.2 places a bar menu at (-8, +6) from its title button. Applied to
/// the anchor rectangle before it is sent, so it moves with the anchor and survives a flip
/// or a reposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Offset {
    pub dx: Px,
    pub dy: Px,
}

impl Offset {
    /// No shift: the popup sits exactly where its anchor edge and gravity put it.
    pub const ZERO: Offset = Offset { dx: 0, dy: 0 };
}

/// How a popup should be positioned and behave, independent of any particular anchor rect or
/// content size (those are resolved separately, after layout, and passed to
/// `positioner_params`).
///
/// Not `Copy`: `anchor` may own an `ElementId` (FINDINGS.md F3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupConfig {
    pub anchor: PopupAnchor,
    pub size: PopupSize,
    pub edge: Edge,
    pub gravity: Gravity,
    pub adjust: Adjust,
    pub grab: Grab,
    /// Where the popup sits relative to its anchor; `Offset::ZERO` for none.
    pub offset: Offset,
}

/// The resolved values an `xdg_positioner` should be configured with: `set_size`,
/// `set_anchor_rect`, `set_anchor`, `set_gravity` and `set_constraint_adjustment`, all in one
/// value so a caller can log or test them without touching the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionerParams {
    pub size: Size,
    pub anchor_rect: Rect,
    pub anchor: Edge,
    pub gravity: Gravity,
    pub adjust: Adjust,
}

/// Combine a `PopupConfig` with a resolved anchor rect and a resolved content size into the
/// values to hand the `xdg_positioner`. `size` is the popup's natural size (already measured,
/// for `PopupSize::FitContent`, or already fixed, for `PopupSize::Fixed`); `FitContent` only
/// clamps it to its `max`. The config's `offset` translates the anchor rectangle.
pub fn positioner_params(config: &PopupConfig, anchor: Rect, size: Size) -> PositionerParams {
    let size = match config.size {
        PopupSize::Fixed => size,
        PopupSize::FitContent { max } => Size {
            width: size.width.min(max),
            height: size.height.min(max),
        },
    };
    PositionerParams {
        size,
        anchor_rect: offset_rect(anchor, config.offset),
        anchor: config.edge,
        gravity: config.gravity,
        adjust: config.adjust,
    }
}

/// `rect` moved by `offset`; its size is unchanged.
pub fn offset_rect(rect: Rect, offset: Offset) -> Rect {
    Rect {
        x: rect.x + offset.dx,
        y: rect.y + offset.dy,
        ..rect
    }
}

/// The `xdg_positioner` wire value for an anchor edge or gravity direction.
pub fn to_wire_anchor(edge: Edge) -> Anchor {
    match edge {
        Edge::None => Anchor::None,
        Edge::Top => Anchor::Top,
        Edge::Bottom => Anchor::Bottom,
        Edge::Left => Anchor::Left,
        Edge::Right => Anchor::Right,
        Edge::TopLeft => Anchor::TopLeft,
        Edge::BottomLeft => Anchor::BottomLeft,
        Edge::TopRight => Anchor::TopRight,
        Edge::BottomRight => Anchor::BottomRight,
    }
}

/// The `xdg_positioner` wire value for a gravity direction.
pub fn to_wire_gravity(gravity: Gravity) -> WireGravity {
    match gravity {
        Edge::None => WireGravity::None,
        Edge::Top => WireGravity::Top,
        Edge::Bottom => WireGravity::Bottom,
        Edge::Left => WireGravity::Left,
        Edge::Right => WireGravity::Right,
        Edge::TopLeft => WireGravity::TopLeft,
        Edge::BottomLeft => WireGravity::BottomLeft,
        Edge::TopRight => WireGravity::TopRight,
        Edge::BottomRight => WireGravity::BottomRight,
    }
}

/// The `xdg_positioner` constraint-adjustment bitfield for `adjust`.
pub fn to_wire_constraint_adjustment(adjust: Adjust) -> ConstraintAdjustment {
    let mut wire = ConstraintAdjustment::empty();
    if adjust.slide_x == Toggle::On {
        wire |= ConstraintAdjustment::SlideX;
    }
    if adjust.slide_y == Toggle::On {
        wire |= ConstraintAdjustment::SlideY;
    }
    if adjust.flip_x == Toggle::On {
        wire |= ConstraintAdjustment::FlipX;
    }
    if adjust.flip_y == Toggle::On {
        wire |= ConstraintAdjustment::FlipY;
    }
    if adjust.resize_x == Toggle::On {
        wire |= ConstraintAdjustment::ResizeX;
    }
    if adjust.resize_y == Toggle::On {
        wire |= ConstraintAdjustment::ResizeY;
    }
    wire
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(edge: Edge, gravity: Gravity, adjust: Adjust, size: PopupSize) -> PopupConfig {
        PopupConfig {
            anchor: PopupAnchor::Rect,
            size,
            edge,
            gravity,
            adjust,
            grab: Grab::Grabbed,
            offset: Offset::ZERO,
        }
    }

    #[test]
    fn positioner_params_ignores_which_kind_of_anchor_the_config_names() {
        // `config.anchor` says *what* the anchor rect was resolved from (an explicit rect or a
        // named element, FINDINGS.md F3); `positioner_params` only ever sees the already
        // resolved `Rect`, so a `PopupAnchor::Element` config must behave exactly like a
        // `PopupAnchor::Rect` one here.
        let anchor = Rect {
            x: 10,
            y: 4,
            width: 80,
            height: 24,
        };
        let size = Size {
            width: 160,
            height: 200,
        };
        let mut by_rect = config(Edge::Bottom, Edge::Bottom, Adjust::NONE, PopupSize::Fixed);
        let mut by_element = by_rect.clone();
        by_element.anchor = PopupAnchor::Element(ElementId::new("dock-pill"));
        by_rect.anchor = PopupAnchor::Rect;
        assert_eq!(
            positioner_params(&by_rect, anchor, size),
            positioner_params(&by_element, anchor, size)
        );
    }

    #[test]
    fn fixed_size_passes_through_unchanged() {
        let cfg = config(Edge::Bottom, Edge::Bottom, Adjust::NONE, PopupSize::Fixed);
        let anchor = Rect {
            x: 10,
            y: 4,
            width: 80,
            height: 24,
        };
        let size = Size {
            width: 160,
            height: 200,
        };
        let params = positioner_params(&cfg, anchor, size);
        assert_eq!(params.size, size);
        assert_eq!(params.anchor_rect, anchor);
    }

    #[test]
    fn fit_content_clamps_to_max_on_each_axis() {
        let cfg = config(
            Edge::Bottom,
            Edge::Bottom,
            Adjust::NONE,
            PopupSize::FitContent { max: 100 },
        );
        let anchor = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let params = positioner_params(
            &cfg,
            anchor,
            Size {
                width: 250,
                height: 40,
            },
        );
        assert_eq!(
            params.size,
            Size {
                width: 100,
                height: 40
            }
        );
    }

    #[test]
    fn fit_content_never_grows_a_size_already_under_max() {
        let cfg = config(
            Edge::Bottom,
            Edge::Bottom,
            Adjust::NONE,
            PopupSize::FitContent { max: 500 },
        );
        let anchor = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let params = positioner_params(
            &cfg,
            anchor,
            Size {
                width: 30,
                height: 20,
            },
        );
        assert_eq!(
            params.size,
            Size {
                width: 30,
                height: 20
            }
        );
    }

    // Table-tested flip/slide/resize combinations, at the wire level: this is the part of
    // `positioner_params` most likely to have a transcription bug (an `Adjust` flag mapped to
    // the wrong `ConstraintAdjustment` bit).
    const EDGE_CASES: &[(Edge, Anchor)] = &[
        (Edge::None, Anchor::None),
        (Edge::Top, Anchor::Top),
        (Edge::Bottom, Anchor::Bottom),
        (Edge::Left, Anchor::Left),
        (Edge::Right, Anchor::Right),
        (Edge::TopLeft, Anchor::TopLeft),
        (Edge::BottomLeft, Anchor::BottomLeft),
        (Edge::TopRight, Anchor::TopRight),
        (Edge::BottomRight, Anchor::BottomRight),
    ];

    #[test]
    fn every_edge_maps_to_its_wire_anchor() {
        for &(edge, expected) in EDGE_CASES {
            assert_eq!(to_wire_anchor(edge), expected, "{edge:?}");
        }
    }

    #[test]
    fn every_gravity_maps_to_its_wire_gravity() {
        let table: &[(Gravity, WireGravity)] = &[
            (Edge::None, WireGravity::None),
            (Edge::Top, WireGravity::Top),
            (Edge::Bottom, WireGravity::Bottom),
            (Edge::Left, WireGravity::Left),
            (Edge::Right, WireGravity::Right),
            (Edge::TopLeft, WireGravity::TopLeft),
            (Edge::BottomLeft, WireGravity::BottomLeft),
            (Edge::TopRight, WireGravity::TopRight),
            (Edge::BottomRight, WireGravity::BottomRight),
        ];
        for &(gravity, expected) in table {
            assert_eq!(to_wire_gravity(gravity), expected, "{gravity:?}");
        }
    }

    const ADJUST_CASES: &[(&str, Adjust, ConstraintAdjustment)] = &[
        ("none", Adjust::NONE, ConstraintAdjustment::empty()),
        (
            "slide both axes",
            Adjust {
                slide_x: Toggle::On,
                slide_y: Toggle::On,
                ..Adjust::NONE
            },
            ConstraintAdjustment::SlideX.union(ConstraintAdjustment::SlideY),
        ),
        (
            "flip both axes",
            Adjust {
                flip_x: Toggle::On,
                flip_y: Toggle::On,
                ..Adjust::NONE
            },
            ConstraintAdjustment::FlipX.union(ConstraintAdjustment::FlipY),
        ),
        (
            "resize both axes",
            Adjust {
                resize_x: Toggle::On,
                resize_y: Toggle::On,
                ..Adjust::NONE
            },
            ConstraintAdjustment::ResizeX.union(ConstraintAdjustment::ResizeY),
        ),
        ("slide and flip", Adjust::SLIDE_AND_FLIP, {
            ConstraintAdjustment::SlideX
                .union(ConstraintAdjustment::SlideY)
                .union(ConstraintAdjustment::FlipX)
                .union(ConstraintAdjustment::FlipY)
        }),
        (
            "every flag",
            Adjust {
                slide_x: Toggle::On,
                slide_y: Toggle::On,
                flip_x: Toggle::On,
                flip_y: Toggle::On,
                resize_x: Toggle::On,
                resize_y: Toggle::On,
            },
            ConstraintAdjustment::all(),
        ),
    ];

    #[test]
    fn adjust_flag_combinations_map_to_the_matching_wire_bits() {
        for &(name, adjust, expected) in ADJUST_CASES {
            assert_eq!(to_wire_constraint_adjustment(adjust), expected, "{name}");
        }
    }
}
