//! The `xdg_positioner` wire values for the popup vocabulary.

use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity as WireGravity,
};

use super::config::{Adjust, Edge, Gravity, Toggle};

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
