//! Table test for `positioner_params`, covering the flip/slide/resize `PopupAdjust` combinations and
//! the `PopupSize` clamping behaviour, against the public API only.

use crate::popup::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
    PopupToggle, positioner_params, to_wire_constraint_adjustment,
};
use crate::region::{ElementId, Rect};
use crate::scale::Size;
use wayland_protocols::xdg::shell::client::xdg_positioner::ConstraintAdjustment;

struct Case {
    name: &'static str,
    edge: PopupEdge,
    gravity: PopupEdge,
    adjust: PopupAdjust,
    popup_size: PopupSize,
    content_size: Size,
    offset: PopupOffset,
    expected_size: Size,
    expected_anchor_rect: Rect,
    expected_wire_adjust: ConstraintAdjustment,
}

const BUTTON: Rect = Rect {
    x: 20,
    y: 4,
    width: 80,
    height: 24,
};

const CASES: &[Case] = &[
    Case {
        name: "menu below button: slide+flip, fixed size",
        edge: PopupEdge::Bottom,
        gravity: PopupEdge::Bottom,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 160,
            height: 200,
        },
        offset: PopupOffset::ZERO,
        expected_size: Size {
            width: 160,
            height: 200,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::SlideX
            .union(ConstraintAdjustment::SlideY)
            .union(ConstraintAdjustment::FlipX)
            .union(ConstraintAdjustment::FlipY),
    },
    Case {
        name: "submenu to the right: resize only, content clamped",
        edge: PopupEdge::TopRight,
        gravity: PopupEdge::BottomRight,
        adjust: PopupAdjust {
            resize_x: PopupToggle::On,
            resize_y: PopupToggle::On,
            ..PopupAdjust::NONE
        },
        popup_size: PopupSize::FitContent { max: 120 },
        content_size: Size {
            width: 300,
            height: 40,
        },
        offset: PopupOffset::ZERO,
        expected_size: Size {
            width: 120,
            height: 40,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::ResizeX.union(ConstraintAdjustment::ResizeY),
    },
    Case {
        name: "tooltip: no adjustment, content already under the cap",
        edge: PopupEdge::Top,
        gravity: PopupEdge::Top,
        adjust: PopupAdjust::NONE,
        popup_size: PopupSize::FitContent { max: 400 },
        content_size: Size {
            width: 90,
            height: 24,
        },
        offset: PopupOffset::ZERO,
        expected_size: Size {
            width: 90,
            height: 24,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::empty(),
    },
    Case {
        name: "corner-anchored, slide-x only",
        edge: PopupEdge::TopLeft,
        gravity: PopupEdge::BottomLeft,
        adjust: PopupAdjust {
            slide_x: PopupToggle::On,
            ..PopupAdjust::NONE
        },
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 50,
            height: 50,
        },
        offset: PopupOffset::ZERO,
        expected_size: Size {
            width: 50,
            height: 50,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::SlideX,
    },
    Case {
        name: "bar menu at (-8, +6) from its title (design/13 §13.3.2)",
        edge: PopupEdge::BottomLeft,
        gravity: PopupEdge::BottomRight,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::FitContent { max: 480 },
        content_size: Size {
            width: 220,
            height: 300,
        },
        offset: PopupOffset { dx: -8, dy: 6 },
        expected_size: Size {
            width: 220,
            height: 300,
        },
        expected_anchor_rect: Rect {
            x: 12,
            y: 10,
            width: 80,
            height: 24,
        },
        expected_wire_adjust: ConstraintAdjustment::SlideX
            .union(ConstraintAdjustment::SlideY)
            .union(ConstraintAdjustment::FlipX)
            .union(ConstraintAdjustment::FlipY),
    },
    Case {
        name: "status item menu at (+8, +6)",
        edge: PopupEdge::BottomRight,
        gravity: PopupEdge::BottomLeft,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 200,
            height: 120,
        },
        offset: PopupOffset { dx: 8, dy: 6 },
        expected_size: Size {
            width: 200,
            height: 120,
        },
        expected_anchor_rect: Rect {
            x: 28,
            y: 10,
            width: 80,
            height: 24,
        },
        expected_wire_adjust: ConstraintAdjustment::SlideX
            .union(ConstraintAdjustment::SlideY)
            .union(ConstraintAdjustment::FlipX)
            .union(ConstraintAdjustment::FlipY),
    },
    Case {
        name: "an offset past the parent's origin is sent as is",
        edge: PopupEdge::BottomLeft,
        gravity: PopupEdge::BottomRight,
        adjust: PopupAdjust::NONE,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 100,
            height: 100,
        },
        offset: PopupOffset { dx: -30, dy: -10 },
        expected_size: Size {
            width: 100,
            height: 100,
        },
        expected_anchor_rect: Rect {
            x: -10,
            y: -6,
            width: 80,
            height: 24,
        },
        expected_wire_adjust: ConstraintAdjustment::empty(),
    },
    Case {
        name: "a fixed size ignores a configured max, however large the content",
        edge: PopupEdge::Bottom,
        gravity: PopupEdge::Bottom,
        adjust: PopupAdjust::NONE,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 10_000,
            height: 10_000,
        },
        offset: PopupOffset::ZERO,
        expected_size: Size {
            width: 10_000,
            height: 10_000,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::empty(),
    },
];

#[test]
fn positioner_params_matches_the_table() {
    for case in CASES {
        let config = PopupConfig {
            anchor: PopupAnchor::Rect,
            size: case.popup_size,
            edge: case.edge,
            gravity: case.gravity,
            adjust: case.adjust,
            grab: PopupGrab::Grabbed,
            offset: case.offset,
        };
        let params = positioner_params(&config, BUTTON, case.content_size);
        assert_eq!(params.size, case.expected_size, "{}: size", case.name);
        assert_eq!(
            params.anchor_rect, case.expected_anchor_rect,
            "{}: anchor_rect",
            case.name
        );
        assert_eq!(params.anchor, case.edge, "{}: anchor edge", case.name);
        assert_eq!(params.gravity, case.gravity, "{}: gravity", case.name);
        assert_eq!(
            to_wire_constraint_adjustment(params.adjust),
            case.expected_wire_adjust,
            "{}: wire constraint adjustment",
            case.name
        );
    }
}

#[test]
fn a_config_anchored_to_an_element_positions_the_same_way_as_one_anchored_to_a_rect() {
    // `PopupConfig` is no longer `Copy` once its anchor can own an `ElementId`;
    // this proves the type still builds and positions correctly either way.
    let config = PopupConfig {
        anchor: PopupAnchor::Element(ElementId::new("dock-pill")),
        size: PopupSize::Fixed,
        edge: PopupEdge::Top,
        gravity: PopupEdge::Top,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        grab: PopupGrab::Grabbed,
        offset: PopupOffset::ZERO,
    };
    let size = Size {
        width: 160,
        height: 90,
    };
    let params = positioner_params(&config, BUTTON, size);
    assert_eq!(params.anchor_rect, BUTTON);
    assert_eq!(params.size, size);
    assert_eq!(params.anchor, PopupEdge::Top);
    assert_eq!(params.gravity, PopupEdge::Top);
}
