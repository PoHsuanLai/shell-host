//! Table test for `positioner_params`, covering the flip/slide/resize `Adjust` combinations and
//! the `PopupSize` clamping behaviour, against the public API only.

use shell_host::popup::{
    Adjust, Edge, Grab, PopupAnchor, PopupConfig, PopupSize, Toggle, positioner_params,
    to_wire_constraint_adjustment,
};
use shell_host::region::Rect;
use shell_host::scale::Size;
use wayland_protocols::xdg::shell::client::xdg_positioner::ConstraintAdjustment;

struct Case {
    name: &'static str,
    edge: Edge,
    gravity: Edge,
    adjust: Adjust,
    popup_size: PopupSize,
    content_size: Size,
    expected_size: Size,
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
        edge: Edge::Bottom,
        gravity: Edge::Bottom,
        adjust: Adjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 160,
            height: 200,
        },
        expected_size: Size {
            width: 160,
            height: 200,
        },
        expected_wire_adjust: ConstraintAdjustment::SlideX
            .union(ConstraintAdjustment::SlideY)
            .union(ConstraintAdjustment::FlipX)
            .union(ConstraintAdjustment::FlipY),
    },
    Case {
        name: "submenu to the right: resize only, content clamped",
        edge: Edge::TopRight,
        gravity: Edge::BottomRight,
        adjust: Adjust {
            resize_x: Toggle::On,
            resize_y: Toggle::On,
            ..Adjust::NONE
        },
        popup_size: PopupSize::FitContent { max: 120 },
        content_size: Size {
            width: 300,
            height: 40,
        },
        expected_size: Size {
            width: 120,
            height: 40,
        },
        expected_wire_adjust: ConstraintAdjustment::ResizeX.union(ConstraintAdjustment::ResizeY),
    },
    Case {
        name: "tooltip: no adjustment, content already under the cap",
        edge: Edge::Top,
        gravity: Edge::Top,
        adjust: Adjust::NONE,
        popup_size: PopupSize::FitContent { max: 400 },
        content_size: Size {
            width: 90,
            height: 24,
        },
        expected_size: Size {
            width: 90,
            height: 24,
        },
        expected_wire_adjust: ConstraintAdjustment::empty(),
    },
    Case {
        name: "corner-anchored, slide-x only",
        edge: Edge::TopLeft,
        gravity: Edge::BottomLeft,
        adjust: Adjust {
            slide_x: Toggle::On,
            ..Adjust::NONE
        },
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 50,
            height: 50,
        },
        expected_size: Size {
            width: 50,
            height: 50,
        },
        expected_wire_adjust: ConstraintAdjustment::SlideX,
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
            grab: Grab::Grabbed,
        };
        let params = positioner_params(&config, BUTTON, case.content_size);
        assert_eq!(params.size, case.expected_size, "{}: size", case.name);
        assert_eq!(params.anchor_rect, BUTTON, "{}: anchor_rect", case.name);
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
fn fixed_size_ignores_a_configured_max() {
    // `PopupSize::Fixed` has no `max`, so even a huge content size is never clamped.
    let config = PopupConfig {
        anchor: PopupAnchor::Rect,
        size: PopupSize::Fixed,
        edge: Edge::Bottom,
        gravity: Edge::Bottom,
        adjust: Adjust::NONE,
        grab: Grab::Ungrabbed,
    };
    let params = positioner_params(
        &config,
        BUTTON,
        Size {
            width: 10_000,
            height: 10_000,
        },
    );
    assert_eq!(
        params.size,
        Size {
            width: 10_000,
            height: 10_000
        }
    );
}
