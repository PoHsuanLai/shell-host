//! The `xdg_positioner` wire values for the popup vocabulary.

use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity as WireGravity,
};

use super::config::{PopupAdjust, PopupEdge, PopupToggle};

/// The `xdg_positioner` wire value for an anchor edge or gravity direction.
pub(crate) fn to_wire_anchor(edge: PopupEdge) -> Anchor {
    match edge {
        PopupEdge::None => Anchor::None,
        PopupEdge::Top => Anchor::Top,
        PopupEdge::Bottom => Anchor::Bottom,
        PopupEdge::Left => Anchor::Left,
        PopupEdge::Right => Anchor::Right,
        PopupEdge::TopLeft => Anchor::TopLeft,
        PopupEdge::BottomLeft => Anchor::BottomLeft,
        PopupEdge::TopRight => Anchor::TopRight,
        PopupEdge::BottomRight => Anchor::BottomRight,
    }
}

/// The `xdg_positioner` wire value for a gravity direction.
pub(crate) fn to_wire_gravity(gravity: PopupEdge) -> WireGravity {
    match gravity {
        PopupEdge::None => WireGravity::None,
        PopupEdge::Top => WireGravity::Top,
        PopupEdge::Bottom => WireGravity::Bottom,
        PopupEdge::Left => WireGravity::Left,
        PopupEdge::Right => WireGravity::Right,
        PopupEdge::TopLeft => WireGravity::TopLeft,
        PopupEdge::BottomLeft => WireGravity::BottomLeft,
        PopupEdge::TopRight => WireGravity::TopRight,
        PopupEdge::BottomRight => WireGravity::BottomRight,
    }
}

/// The `xdg_positioner` constraint-adjustment bitfield for `adjust`.
pub(crate) fn to_wire_constraint_adjustment(adjust: PopupAdjust) -> ConstraintAdjustment {
    let mut wire = ConstraintAdjustment::empty();
    if adjust.slide_x == PopupToggle::On {
        wire |= ConstraintAdjustment::SlideX;
    }
    if adjust.slide_y == PopupToggle::On {
        wire |= ConstraintAdjustment::SlideY;
    }
    if adjust.flip_x == PopupToggle::On {
        wire |= ConstraintAdjustment::FlipX;
    }
    if adjust.flip_y == PopupToggle::On {
        wire |= ConstraintAdjustment::FlipY;
    }
    if adjust.resize_x == PopupToggle::On {
        wire |= ConstraintAdjustment::ResizeX;
    }
    if adjust.resize_y == PopupToggle::On {
        wire |= ConstraintAdjustment::ResizeY;
    }
    wire
}

#[cfg(test)]
mod tests {
    use super::*;

    // Table-tested flip/slide/resize combinations, at the wire level: this is the part of
    // `positioner_params` most likely to have a transcription bug (an `PopupAdjust` flag mapped to
    // the wrong `ConstraintAdjustment` bit).
    const EDGE_CASES: &[(PopupEdge, Anchor, WireGravity)] = &[
        (PopupEdge::None, Anchor::None, WireGravity::None),
        (PopupEdge::Top, Anchor::Top, WireGravity::Top),
        (PopupEdge::Bottom, Anchor::Bottom, WireGravity::Bottom),
        (PopupEdge::Left, Anchor::Left, WireGravity::Left),
        (PopupEdge::Right, Anchor::Right, WireGravity::Right),
        (PopupEdge::TopLeft, Anchor::TopLeft, WireGravity::TopLeft),
        (
            PopupEdge::BottomLeft,
            Anchor::BottomLeft,
            WireGravity::BottomLeft,
        ),
        (PopupEdge::TopRight, Anchor::TopRight, WireGravity::TopRight),
        (
            PopupEdge::BottomRight,
            Anchor::BottomRight,
            WireGravity::BottomRight,
        ),
    ];

    #[test]
    fn every_edge_maps_to_its_wire_anchor_and_gravity() {
        for &(edge, anchor, gravity) in EDGE_CASES {
            assert_eq!(to_wire_anchor(edge), anchor, "{edge:?}: anchor");
            assert_eq!(to_wire_gravity(edge), gravity, "{edge:?}: gravity");
        }
    }

    const ADJUST_CASES: &[(&str, PopupAdjust, ConstraintAdjustment)] = &[
        ("none", PopupAdjust::NONE, ConstraintAdjustment::empty()),
        (
            "slide both axes",
            PopupAdjust {
                slide_x: PopupToggle::On,
                slide_y: PopupToggle::On,
                ..PopupAdjust::NONE
            },
            ConstraintAdjustment::SlideX.union(ConstraintAdjustment::SlideY),
        ),
        (
            "flip both axes",
            PopupAdjust {
                flip_x: PopupToggle::On,
                flip_y: PopupToggle::On,
                ..PopupAdjust::NONE
            },
            ConstraintAdjustment::FlipX.union(ConstraintAdjustment::FlipY),
        ),
        (
            "resize both axes",
            PopupAdjust {
                resize_x: PopupToggle::On,
                resize_y: PopupToggle::On,
                ..PopupAdjust::NONE
            },
            ConstraintAdjustment::ResizeX.union(ConstraintAdjustment::ResizeY),
        ),
        ("slide and flip", PopupAdjust::SLIDE_AND_FLIP, {
            ConstraintAdjustment::SlideX
                .union(ConstraintAdjustment::SlideY)
                .union(ConstraintAdjustment::FlipX)
                .union(ConstraintAdjustment::FlipY)
        }),
        (
            "every flag",
            PopupAdjust {
                slide_x: PopupToggle::On,
                slide_y: PopupToggle::On,
                flip_x: PopupToggle::On,
                flip_y: PopupToggle::On,
                resize_x: PopupToggle::On,
                resize_y: PopupToggle::On,
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
