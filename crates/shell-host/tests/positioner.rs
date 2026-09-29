//! Table test for `positioner_params`, covering the flip/slide/resize `PopupAdjust` combinations and
//! the `PopupSize` clamping behaviour, against the public API only.

use shell_host::PopupPlacement;
use shell_host::popup::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
    PopupToggle, offset_rect, positioner_params, refit, to_wire_constraint_adjustment,
};
use shell_host::region::{ElementId, Rect};
use shell_host::scale::Size;
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
fn fixed_size_ignores_a_configured_max() {
    // `PopupSize::Fixed` has no `max`, so even a huge content size is never clamped.
    let config = PopupConfig {
        anchor: PopupAnchor::Rect,
        size: PopupSize::Fixed,
        edge: PopupEdge::Bottom,
        gravity: PopupEdge::Bottom,
        adjust: PopupAdjust::NONE,
        grab: PopupGrab::Ungrabbed,
        offset: PopupOffset::ZERO,
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

#[test]
fn an_offset_moves_the_anchor_rect_and_keeps_its_size() {
    const OFFSETS: &[(PopupOffset, Rect)] = &[
        (PopupOffset::ZERO, BUTTON),
        (
            PopupOffset { dx: -8, dy: 6 },
            Rect {
                x: 12,
                y: 10,
                width: 80,
                height: 24,
            },
        ),
        (
            PopupOffset { dx: 0, dy: -4 },
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
    assert_eq!(PopupOffset::default(), PopupOffset::ZERO);
}

/// `popup::refit`: a `FitContent` popup's placement changes, clamped to `max`, exactly
/// when the measured content differs from its current placement.
#[test]
fn refit_reflows_a_fit_content_popup_to_its_measured_content_clamped_to_max() {
    let fixed = PopupConfig {
        anchor: PopupAnchor::Rect,
        size: PopupSize::Fixed,
        edge: PopupEdge::Bottom,
        gravity: PopupEdge::Bottom,
        adjust: PopupAdjust::NONE,
        grab: PopupGrab::Ungrabbed,
        offset: PopupOffset::ZERO,
    };
    let fit_content = PopupConfig {
        size: PopupSize::FitContent { max: 200 },
        ..fixed.clone()
    };
    let current = PopupPlacement {
        anchor_rect: BUTTON,
        size: Size {
            width: 160,
            height: 48,
        },
    };

    assert_eq!(
        refit(
            &fixed,
            current,
            Size {
                width: 999,
                height: 999
            }
        ),
        None,
        "Fixed never refits"
    );
    assert_eq!(
        refit(
            &fit_content,
            current,
            Size {
                width: 160,
                height: 48
            }
        ),
        None,
        "already at its measured size"
    );
    assert_eq!(
        refit(
            &fit_content,
            current,
            Size {
                width: 160,
                height: 144
            }
        ),
        Some(PopupPlacement {
            size: Size {
                width: 160,
                height: 144
            },
            ..current
        }),
        "content grew: refit to it"
    );
    assert_eq!(
        refit(
            &fit_content,
            current,
            Size {
                width: 260,
                height: 999
            }
        ),
        Some(PopupPlacement {
            size: Size {
                width: 200,
                height: 200
            },
            ..current
        }),
        "content past max: refit, clamped on each axis"
    );
}
