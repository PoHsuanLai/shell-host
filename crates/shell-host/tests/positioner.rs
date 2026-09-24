//! Table test for `positioner_params`, covering the flip/slide/resize `Adjust` combinations and
//! the `PopupSize` clamping behaviour, against the public API only.

use shell_host::popup::{
    Adjust, Edge, Grab, Offset, PopupAnchor, PopupConfig, PopupSize, Toggle, offset_rect,
    positioner_params, to_wire_constraint_adjustment,
};
use shell_host::region::{ElementId, Rect};
use shell_host::scale::Size;
use wayland_protocols::xdg::shell::client::xdg_positioner::ConstraintAdjustment;

struct Case {
    name: &'static str,
    edge: Edge,
    gravity: Edge,
    adjust: Adjust,
    popup_size: PopupSize,
    content_size: Size,
    offset: Offset,
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
        edge: Edge::Bottom,
        gravity: Edge::Bottom,
        adjust: Adjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 160,
            height: 200,
        },
        offset: Offset::ZERO,
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
        offset: Offset::ZERO,
        expected_size: Size {
            width: 120,
            height: 40,
        },
        expected_anchor_rect: BUTTON,
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
        offset: Offset::ZERO,
        expected_size: Size {
            width: 90,
            height: 24,
        },
        expected_anchor_rect: BUTTON,
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
        offset: Offset::ZERO,
        expected_size: Size {
            width: 50,
            height: 50,
        },
        expected_anchor_rect: BUTTON,
        expected_wire_adjust: ConstraintAdjustment::SlideX,
    },
    Case {
        name: "bar menu at (-8, +6) from its title (design/13 §13.3.2)",
        edge: Edge::BottomLeft,
        gravity: Edge::BottomRight,
        adjust: Adjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::FitContent { max: 480 },
        content_size: Size {
            width: 220,
            height: 300,
        },
        offset: Offset { dx: -8, dy: 6 },
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
        edge: Edge::BottomRight,
        gravity: Edge::BottomLeft,
        adjust: Adjust::SLIDE_AND_FLIP,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 200,
            height: 120,
        },
        offset: Offset { dx: 8, dy: 6 },
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
        edge: Edge::BottomLeft,
        gravity: Edge::BottomRight,
        adjust: Adjust::NONE,
        popup_size: PopupSize::Fixed,
        content_size: Size {
            width: 100,
            height: 100,
        },
        offset: Offset { dx: -30, dy: -10 },
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
fn fixed_size_ignores_a_configured_max() {
    // `PopupSize::Fixed` has no `max`, so even a huge content size is never clamped.
    let config = PopupConfig {
        anchor: PopupAnchor::Rect,
        size: PopupSize::Fixed,
        edge: Edge::Bottom,
        gravity: Edge::Bottom,
        adjust: Adjust::NONE,
        grab: Grab::Ungrabbed,
        offset: Offset::ZERO,
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

#[test]
fn a_config_anchored_to_an_element_positions_the_same_way_as_one_anchored_to_a_rect() {
    // `PopupConfig` is no longer `Copy` once its anchor can own an `ElementId`
    // (FINDINGS.md F3); this proves the type still builds and positions correctly either way.
    let config = PopupConfig {
        anchor: PopupAnchor::Element(ElementId::new("dock-pill")),
        size: PopupSize::Fixed,
        edge: Edge::Top,
        gravity: Edge::Top,
        adjust: Adjust::SLIDE_AND_FLIP,
        grab: Grab::Grabbed,
        offset: Offset::ZERO,
    };
    let size = Size {
        width: 160,
        height: 90,
    };
    let params = positioner_params(&config, BUTTON, size);
    assert_eq!(params.anchor_rect, BUTTON);
    assert_eq!(params.size, size);
    assert_eq!(params.anchor, Edge::Top);
    assert_eq!(params.gravity, Edge::Top);
}

#[test]
fn an_offset_moves_the_anchor_rect_and_keeps_its_size() {
    const OFFSETS: &[(Offset, Rect)] = &[
        (Offset::ZERO, BUTTON),
        (
            Offset { dx: -8, dy: 6 },
            Rect {
                x: 12,
                y: 10,
                width: 80,
                height: 24,
            },
        ),
        (
            Offset { dx: 0, dy: -4 },
            Rect {
                x: 20,
                y: 0,
                width: 80,
                height: 24,
            },
        ),
    ];
    for &(offset, want) in OFFSETS {
        assert_eq!(offset_rect(BUTTON, offset), want, "{offset:?}");
    }
    assert_eq!(Offset::default(), Offset::ZERO);
}
