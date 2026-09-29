//! Resolving a `RegionSpec` against a laid-out document.

use crate::geometry::inflate;
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
        RegionSpec::Element {
            id,
            radius,
            inflate: by,
        } => {
            let element = ElementRegion {
                id: id.clone(),
                radius: *radius,
                inflate: *by,
            };
            match rounded_rect(&element, &lookup) {
                Some(rr) => Resolved::Strips(rounded_strips(rr)),
                None => Resolved::Pending,
            }
        }
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
                RegionSpec::Element {
                    id: pill(),
                    radius: 0,
                    inflate: 2,
                },
                Resolved::Strips(vec![Rect {
                    x: 18,
                    y: 6,
                    width: 104,
                    height: 28,
                }]),
            ),
            (
                "missing element keeps the old region",
                RegionSpec::Element {
                    id: ElementId::new("no-such-element"),
                    radius: 8,
                    inflate: 0,
                },
                Resolved::Pending,
            ),
        ];
        for (name, spec, want) in cases {
            assert_eq!(resolve(&spec, SIZE, lookup), want, "{name}");
        }
    }

    #[test]
    fn a_rounded_element_is_its_strips() {
        let spec = RegionSpec::Element {
            id: pill(),
            radius: 6,
            inflate: 0,
        };
        let want = rounded_strips(RoundedRect {
            rect: PILL_RECT,
            radius: 6,
        });
        assert_eq!(resolve(&spec, SIZE, lookup), Resolved::Strips(want));
    }

    /// A second element for the `Elements` cases: overlaps `PILL_RECT` on purpose (concatenated
    /// strips are fine for a `wl_region`).
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
}
