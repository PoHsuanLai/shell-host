//! Turning a [`RegionSpec`] or a [`PopupAnchor`] into concrete geometry, after layout.
//!
//! Pure: the caller looks up element rectangles in the laid-out document and passes a lookup
//! function; building the `wl_region` from the result, and committing it to the compositor, are
//! the caller's effects (input regions via `rounded_strips`; blur regions the same way, through
//! `BackgroundEffect::set_blur_region`, double-buffered with the next commit).
//! [`Tracked`] is the other half of "the caller's effect": it remembers what was last sent, so a
//! surface whose element rectangle has not moved does not recommit on every frame (design/10
//! §10.3.12, "recomputed on every frame whose pill rect changed" — the converse also holds: a
//! frame whose rect has *not* changed recomputes nothing).

use crate::geometry::inflate;
use crate::popup::PopupAnchor;
use crate::region::{ElementId, ElementRegion, Rect, RegionSpec, RoundedRect, rounded_strips};
use crate::scale::Size;

/// A region ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The union of these rectangles; empty means "nothing".
    Strips(Vec<Rect>),
    /// The element the region follows is not laid out yet (or not in the document); keep
    /// whatever region the surface has now.
    Pending,
}

/// Resolve `spec` for a surface of logical `size`, finding element rectangles with `lookup`.
pub fn resolve(
    spec: &RegionSpec,
    size: Size,
    lookup: impl Fn(ElementId) -> Option<Rect>,
) -> Resolved {
    match spec {
        RegionSpec::Empty => Resolved::Strips(Vec::new()),
        RegionSpec::Whole => Resolved::Strips(whole(size)),
        RegionSpec::Rects(rects) => {
            Resolved::Strips(rects.iter().flat_map(|&rr| rounded_strips(rr)).collect())
        }
        RegionSpec::Element(element) => match rounded_rect(element, &lookup) {
            Some(rr) => Resolved::Strips(rounded_strips(rr)),
            None => Resolved::Pending,
        },
        RegionSpec::Elements(elements) => Resolved::Strips(
            elements
                .iter()
                .filter_map(|element| rounded_rect(element, &lookup))
                .flat_map(rounded_strips)
                .collect(),
        ),
    }
}

/// `element`'s bounding rect, inflated and given its radius; `None` while it is not (or no
/// longer) in the document. Shared by `RegionSpec::Element` and `RegionSpec::Elements`, which
/// differ only in what an absent element means to the caller.
fn rounded_rect(
    element: &ElementRegion,
    lookup: impl Fn(ElementId) -> Option<Rect>,
) -> Option<RoundedRect> {
    lookup(element.id.clone()).map(|rect| RoundedRect {
        rect: inflate(rect, element.inflate),
        radius: element.radius,
    })
}

/// Resolve a popup's anchor rectangle: `PopupAnchor::Rect` uses `placement_rect` (the value
/// carried alongside the config, FINDINGS.md F4) unchanged; `PopupAnchor::Element` looks the
/// element up the same way [`resolve`] does, falling back to `placement_rect` while the element
/// is not yet laid out (a popup has to anchor somewhere even on its very first configure).
pub fn resolve_popup_anchor(
    anchor: &PopupAnchor,
    placement_rect: Rect,
    lookup: impl Fn(ElementId) -> Option<Rect>,
) -> Rect {
    match anchor {
        PopupAnchor::Rect => placement_rect,
        PopupAnchor::Element(id) => lookup(id.clone()).unwrap_or(placement_rect),
    }
}

fn whole(size: Size) -> Vec<Rect> {
    let rect = Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
    };
    if rect.has_area() {
        vec![rect]
    } else {
        Vec::new()
    }
}

/// Whether a freshly resolved region differs from the one [`Tracked`] last recorded, and so
/// needs to be sent to the compositor again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Same,
    Changed,
}

/// Remembers the last resolved region for one region-following surface (an input region, or a
/// blur region), so the caller only rebuilds and recommits a `wl_region` when the geometry
/// actually moved.
///
/// [`Resolved::Pending`] never overwrites what was last recorded — [`Resolved`]'s own contract is
/// "keep whatever region the surface has now" — so it always reports [`Change::Same`]: there is
/// nothing new to send, whether or not anything was ever resolved before.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tracked {
    last: Option<Vec<Rect>>,
}

impl Tracked {
    /// A tracker that has not recorded anything yet; its first real resolution always reports
    /// [`Change::Changed`].
    pub fn new() -> Tracked {
        Tracked::default()
    }

    /// A tracker for a region the compositor is already known to hold, so resolving to the
    /// same strips sends nothing: a fresh surface has no blur region, which is
    /// `Tracked::holding(Vec::new())`, and resolving "no blur" on it creates no effect object.
    pub fn holding(strips: Vec<Rect>) -> Tracked {
        Tracked { last: Some(strips) }
    }

    /// Record `resolved` and report whether the compositor needs a new region.
    pub fn update(&mut self, resolved: &Resolved) -> Change {
        let Resolved::Strips(strips) = resolved else {
            return Change::Same;
        };
        if self.last.as_deref() == Some(strips.as_slice()) {
            return Change::Same;
        }
        self.last = Some(strips.clone());
        Change::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        width: 200,
        height: 40,
    };
    const PILL_RECT: Rect = Rect {
        x: 20,
        y: 8,
        width: 100,
        height: 24,
    };

    fn pill() -> ElementId {
        ElementId::new("dock-pill")
    }

    fn lookup(id: ElementId) -> Option<Rect> {
        (id == pill()).then_some(PILL_RECT)
    }

    #[test]
    fn every_spec_resolves_as_documented() {
        let square = RoundedRect {
            rect: Rect {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            radius: 0,
        };
        let cases: Vec<(&str, RegionSpec, Resolved)> = vec![
            ("empty", RegionSpec::Empty, Resolved::Strips(vec![])),
            (
                "whole",
                RegionSpec::Whole,
                Resolved::Strips(vec![Rect {
                    x: 0,
                    y: 0,
                    width: 200,
                    height: 40,
                }]),
            ),
            (
                "square rects pass through",
                RegionSpec::Rects(vec![square, square]),
                Resolved::Strips(vec![square.rect, square.rect]),
            ),
            (
                "element, square, inflated",
                RegionSpec::Element(ElementRegion {
                    id: pill(),
                    radius: 0,
                    inflate: 2,
                }),
                Resolved::Strips(vec![Rect {
                    x: 18,
                    y: 6,
                    width: 104,
                    height: 28,
                }]),
            ),
            (
                "missing element keeps the old region",
                RegionSpec::Element(ElementRegion {
                    id: ElementId::new("no-such-element"),
                    radius: 8,
                    inflate: 0,
                }),
                Resolved::Pending,
            ),
        ];
        for (name, spec, want) in cases {
            assert_eq!(resolve(&spec, SIZE, lookup), want, "{name}");
        }
    }

    #[test]
    fn a_rounded_element_is_its_strips() {
        let spec = RegionSpec::Element(ElementRegion {
            id: pill(),
            radius: 6,
            inflate: 0,
        });
        let want = rounded_strips(RoundedRect {
            rect: PILL_RECT,
            radius: 6,
        });
        assert_eq!(resolve(&spec, SIZE, lookup), Resolved::Strips(want));
    }

    /// A second element for the `Elements` cases: overlaps `PILL_RECT` on purpose (concatenated
    /// strips are fine for a `wl_region`, G60).
    const CARD_RECT: Rect = Rect {
        x: 60,
        y: 4,
        width: 100,
        height: 24,
    };

    fn card() -> ElementId {
        ElementId::new("card")
    }

    /// Both `pill()` and `card()` resolve; anything else (the "missing" cases below) does not.
    fn lookup_two(id: ElementId) -> Option<Rect> {
        if id == pill() {
            Some(PILL_RECT)
        } else if id == card() {
            Some(CARD_RECT)
        } else {
            None
        }
    }

    #[test]
    fn elements_resolves_to_the_concatenated_strips_of_every_present_element() {
        let missing = ElementId::new("missing");
        let cases: Vec<(&str, Vec<ElementRegion>, Vec<Rect>)> = vec![
            (
                "two overlapping cards, both present",
                vec![
                    ElementRegion {
                        id: pill(),
                        radius: 6,
                        inflate: 0,
                    },
                    ElementRegion {
                        id: card(),
                        radius: 0,
                        inflate: 0,
                    },
                ],
                [
                    rounded_strips(RoundedRect {
                        rect: PILL_RECT,
                        radius: 6,
                    }),
                    rounded_strips(RoundedRect {
                        rect: CARD_RECT,
                        radius: 0,
                    }),
                ]
                .concat(),
            ),
            (
                "one of two missing contributes nothing, the other still resolves",
                vec![
                    ElementRegion {
                        id: pill(),
                        radius: 0,
                        inflate: 0,
                    },
                    ElementRegion {
                        id: missing.clone(),
                        radius: 4,
                        inflate: 0,
                    },
                ],
                vec![PILL_RECT],
            ),
            (
                "every element missing is an empty union, not pending",
                vec![ElementRegion {
                    id: missing,
                    radius: 4,
                    inflate: 0,
                }],
                vec![],
            ),
            ("no elements at all is an empty union", vec![], vec![]),
        ];
        for (name, elements, want) in cases {
            let spec = RegionSpec::Elements(elements);
            assert_eq!(
                resolve(&spec, SIZE, lookup_two),
                Resolved::Strips(want),
                "{name}"
            );
        }
    }

    #[test]
    fn whole_of_an_unconfigured_surface_is_nothing() {
        let zero = Size {
            width: 0,
            height: 0,
        };
        assert_eq!(
            resolve(&RegionSpec::Whole, zero, lookup),
            Resolved::Strips(vec![])
        );
    }

    #[test]
    fn popup_anchor_resolves_an_element_or_falls_back_to_the_placement_rect() {
        let placement = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let cases: &[(&str, PopupAnchor, Rect)] = &[
            (
                "explicit rect uses the placement rect",
                PopupAnchor::Rect,
                placement,
            ),
            (
                "element uses its own rect",
                PopupAnchor::Element(pill()),
                PILL_RECT,
            ),
            (
                "unresolved element falls back to the placement rect",
                PopupAnchor::Element(ElementId::new("no-such-element")),
                placement,
            ),
        ];
        for (name, anchor, want) in cases {
            assert_eq!(
                resolve_popup_anchor(anchor, placement, lookup),
                *want,
                "{name}"
            );
        }
    }

    #[test]
    fn tracked_change_detection() {
        let a = Resolved::Strips(vec![PILL_RECT]);
        let b = Resolved::Strips(vec![Rect { x: 21, ..PILL_RECT }]);
        let mut t = Tracked::new();
        assert_eq!(t.update(&a), Change::Changed, "first real resolution");
        assert_eq!(t.update(&a), Change::Same, "same rect again");
        assert_eq!(t.update(&b), Change::Changed, "rect moved");
        assert_eq!(t.update(&b), Change::Same, "settled at the new rect");
        assert_eq!(
            t.update(&Resolved::Pending),
            Change::Same,
            "pending never forces a recommit"
        );
        // Pending did not overwrite what was last recorded: going back to `b` is still "same".
        assert_eq!(
            t.update(&b),
            Change::Same,
            "pending kept the last real rect"
        );
        assert_eq!(t.update(&a), Change::Changed, "b -> a is a real change");
    }

    #[test]
    fn a_tracker_that_has_never_recorded_anything_treats_pending_as_no_change() {
        let mut t = Tracked::new();
        assert_eq!(t.update(&Resolved::Pending), Change::Same);
    }

    #[test]
    fn a_tracker_holding_nothing_does_not_resend_nothing() {
        let mut t = Tracked::holding(Vec::new());
        assert_eq!(
            t.update(&Resolved::Strips(vec![])),
            Change::Same,
            "still empty"
        );
        assert_eq!(
            t.update(&Resolved::Strips(vec![PILL_RECT])),
            Change::Changed
        );
        assert_eq!(
            t.update(&Resolved::Strips(vec![])),
            Change::Changed,
            "cleared"
        );
        let mut fresh = Tracked::new();
        assert_eq!(
            fresh.update(&Resolved::Strips(vec![])),
            Change::Changed,
            "fresh sends"
        );
    }
}
