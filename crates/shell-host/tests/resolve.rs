//! Pure-table tests for the geometry `resolve.rs` and `layer.rs` add on top of
//! `region`/`scale`/`popup`, against the public API only: rounding a fractional element rect
//! outward, inflating it, `resolve::Tracked`'s change detection, and a layer surface's
//! `configure` -> resolved logical size (`Extent::{Fill, Px}`).

use shell_host::geometry::{enclosing_rect, inflate};
use shell_host::handlers::layer::configured_size;
use shell_host::layer::{Extent, LayerSize};
use shell_host::region::{ElementId, ElementRegion, Rect, RegionSpec, RoundedRect};
use shell_host::resolve::{Change, Resolved, Tracked, resolve, resolve_popup_anchor};
use shell_host::scale::Size;
use shell_host::{Adjust, Edge, Grab, Offset, PopupAnchor, PopupConfig, PopupSize};

const BUTTON: Rect = Rect {
    x: 10,
    y: 4,
    width: 80,
    height: 24,
};

#[test]
fn enclosing_rect_rounds_a_fractional_bounding_box_outward() {
    type Bounds = (f64, f64, f64, f64);
    const CASES: &[(&str, Bounds, Rect)] = &[
        (
            "already whole",
            (10.0, 4.0, 80.0, 24.0),
            Rect {
                x: 10,
                y: 4,
                width: 80,
                height: 24,
            },
        ),
        (
            "fractional on every edge rounds out, never in",
            (10.2, 4.8, 79.4, 23.1),
            Rect {
                x: 10,
                y: 4,
                width: 80,
                height: 24,
            },
        ),
        (
            "a sub-pixel box still covers at least one pixel",
            (0.1, 0.1, 0.2, 0.2),
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        ),
    ];
    for (name, (x, y, w, h), want) in CASES {
        assert_eq!(enclosing_rect(*x, *y, *w, *h), *want, "{name}");
    }
}

#[test]
fn inflate_grows_a_rect_on_every_side() {
    const CASES: &[(&str, Rect, i32, Rect)] = &[
        (
            "positive inflate grows symmetrically",
            BUTTON,
            4,
            Rect {
                x: 6,
                y: 0,
                width: 88,
                height: 32,
            },
        ),
        ("zero inflate is a no-op", BUTTON, 0, BUTTON),
        (
            "negative inflate shrinks, clamped at zero area",
            Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
            -10,
            Rect {
                x: 10,
                y: 10,
                width: 0,
                height: 0,
            },
        ),
    ];
    for (name, rect, by, want) in CASES {
        assert_eq!(inflate(*rect, *by), *want, "{name}");
    }
}

#[test]
fn tracked_only_reports_a_change_when_the_resolved_rect_moves() {
    let here = Resolved::Strips(vec![BUTTON]);
    let moved = Resolved::Strips(vec![Rect { x: 12, ..BUTTON }]);
    let mut tracked = Tracked::new();

    assert_eq!(tracked.update(&here), Change::Changed, "first resolution");
    assert_eq!(tracked.update(&here), Change::Same, "settled, no move");
    assert_eq!(
        tracked.update(&Resolved::Pending),
        Change::Same,
        "pending never recommits"
    );
    assert_eq!(
        tracked.update(&here),
        Change::Same,
        "pending did not disturb the last real rect"
    );
    assert_eq!(tracked.update(&moved), Change::Changed, "the rect moved");
    assert_eq!(
        tracked.update(&moved),
        Change::Same,
        "settled at the new rect"
    );
}

#[test]
fn resolving_a_rounded_element_region_matches_resolving_the_equivalent_rects_spec() {
    let id = ElementId::new("dock-pill");
    let lookup = |candidate: ElementId| (candidate == id).then_some(BUTTON);
    let element_spec = RegionSpec::Element {
        id: id.clone(),
        radius: 6,
        inflate: 0,
    };
    let rects_spec = RegionSpec::Rects(vec![RoundedRect {
        rect: BUTTON,
        radius: 6,
    }]);
    let size = Size {
        width: 200,
        height: 40,
    };
    assert_eq!(
        resolve(&element_spec, size, lookup),
        resolve(&rects_spec, size, |_| None),
    );
}

#[test]
fn resolving_two_elements_matches_resolving_the_equivalent_rects_spec() {
    let a = ElementId::new("card-a");
    let b = ElementId::new("card-b");
    const OTHER: Rect = Rect {
        x: 60,
        y: 4,
        width: 100,
        height: 24,
    };
    let lookup = |candidate: ElementId| {
        if candidate == a {
            Some(BUTTON)
        } else if candidate == b {
            Some(OTHER)
        } else {
            None
        }
    };
    let elements_spec = RegionSpec::Elements(vec![
        ElementRegion {
            id: a.clone(),
            radius: 6,
            inflate: 0,
        },
        ElementRegion {
            id: b.clone(),
            radius: 0,
            inflate: 0,
        },
    ]);
    let rects_spec = RegionSpec::Rects(vec![
        RoundedRect {
            rect: BUTTON,
            radius: 6,
        },
        RoundedRect {
            rect: OTHER,
            radius: 0,
        },
    ]);
    let size = Size {
        width: 200,
        height: 40,
    };
    assert_eq!(
        resolve(&elements_spec, size, lookup),
        resolve(&rects_spec, size, |_| None),
        "present elements concatenate the same as an equivalent Rects spec",
    );

    let one_missing = RegionSpec::Elements(vec![
        ElementRegion {
            id: a.clone(),
            radius: 6,
            inflate: 0,
        },
        ElementRegion {
            id: ElementId::new("no-such-card"),
            radius: 0,
            inflate: 0,
        },
    ]);
    assert_eq!(
        resolve(&one_missing, size, lookup),
        resolve(
            &RegionSpec::Rects(vec![RoundedRect {
                rect: BUTTON,
                radius: 6,
            }]),
            size,
            |_| None,
        ),
        "a missing element contributes nothing, the other still resolves",
    );
}

#[test]
fn a_popup_anchored_to_an_element_falls_back_to_its_placement_rect_until_laid_out() {
    let config = PopupConfig {
        anchor: PopupAnchor::Element(ElementId::new("dock-pill")),
        size: PopupSize::Fixed,
        edge: Edge::Top,
        gravity: Edge::Top,
        adjust: Adjust::NONE,
        grab: Grab::Ungrabbed,
        offset: Offset::ZERO,
    };
    let placement = Rect {
        x: 0,
        y: 0,
        width: 12,
        height: 12,
    };
    // Not laid out yet: falls back to the placement rect passed alongside the config.
    assert_eq!(
        resolve_popup_anchor(&config.anchor, placement, |_| None),
        placement
    );
    // Laid out: uses the element's own rect instead.
    assert_eq!(
        resolve_popup_anchor(&config.anchor, placement, |_| Some(BUTTON)),
        BUTTON
    );
}

#[test]
fn layer_configure_resolves_to_a_size_per_axis() {
    let dock = LayerSize {
        width: Extent::Px(600),
        height: Extent::Px(96),
    };
    let bar = LayerSize {
        width: Extent::Fill,
        height: Extent::Px(32),
    };
    let cases: &[(&str, LayerSize, (u32, u32), Size)] = &[
        (
            "a fixed-size axis keeps its request when the compositor sends 0",
            dock,
            (0, 0),
            Size {
                width: 600,
                height: 96,
            },
        ),
        (
            "the compositor may still override a fixed size (a resize)",
            dock,
            (620, 100),
            Size {
                width: 620,
                height: 100,
            },
        ),
        (
            "a stretching axis takes whatever the compositor resolved it to",
            bar,
            (1920, 0),
            Size {
                width: 1920,
                height: 32,
            },
        ),
    ];
    for (name, requested, wire, want) in cases {
        assert_eq!(configured_size(*requested, *wire), *want, "{name}");
    }
}
