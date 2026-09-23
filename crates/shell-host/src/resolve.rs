//! Turning a [`RegionSpec`] into the rectangles a `wl_region` is built from, after layout.
//!
//! Pure: the caller looks up element rectangles in the laid-out document and passes a lookup
//! function; building the `wl_region` from the result is the caller's effect. Used for the
//! input region and, through [`BlurSpec`](crate::blur::BlurSpec), the blur region, recomputed on
//! every frame whose element rectangle changed (design/10-BEHAVIOUR-dock.md §10.3.12).

use crate::geometry::inflate;
use crate::region::{ElementId, Rect, RegionSpec, RoundedRect, rounded_strips};
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
        } => match lookup(*id) {
            Some(rect) => Resolved::Strips(rounded_strips(RoundedRect {
                rect: inflate(rect, *by),
                radius: *radius,
            })),
            None => Resolved::Pending,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        width: 200,
        height: 40,
    };
    const PILL: ElementId = ElementId(1);
    const PILL_RECT: Rect = Rect {
        x: 20,
        y: 8,
        width: 100,
        height: 24,
    };

    fn lookup(id: ElementId) -> Option<Rect> {
        (id == PILL).then_some(PILL_RECT)
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
                    id: PILL,
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
                    id: ElementId(99),
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
            id: PILL,
            radius: 6,
            inflate: 0,
        };
        let want = rounded_strips(RoundedRect {
            rect: PILL_RECT,
            radius: 6,
        });
        assert_eq!(resolve(&spec, SIZE, lookup), Resolved::Strips(want));
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
