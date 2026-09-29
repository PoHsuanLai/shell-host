//! Geometry the host adds to the `region` and `scale` types: points, margins, and turning a
//! laid-out element's fractional bounding box into whole logical pixels.
//!
//! Everything is integer logical px ([`Px`]); floats appear only at the Blitz boundary, as
//! arguments, and never in a stored value.

use crate::region::{Px, Rect};

/// A surface-local position in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: Px,
    pub y: Px,
}

/// Space kept free around a layer surface, in logical pixels (`zwlr_layer_surface_v1.set_margin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Margin {
    pub top: Px,
    pub right: Px,
    pub bottom: Px,
    pub left: Px,
}

impl Margin {
    /// No margin on any side.
    pub const ZERO: Margin = Margin {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };
}

/// The smallest whole-pixel rectangle containing a fractional box (Blitz's
/// `get_client_bounding_rect`): the origin rounds down, the far edge rounds up, so a region
/// built from it never clips the element it follows.
pub fn enclosing_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    let left = x.floor();
    let top = y.floor();
    let right = (x + width.max(0.0)).ceil();
    let bottom = (y + height.max(0.0)).ceil();
    Rect {
        x: left as Px,
        y: top as Px,
        width: (right - left) as Px,
        height: (bottom - top) as Px,
    }
}

/// `rect` grown by `by` on every side (shrunk for a negative `by`, never below zero size).
pub fn inflate(rect: Rect, by: Px) -> Rect {
    Rect {
        x: rect.x - by,
        y: rect.y - by,
        width: (rect.width + 2 * by).max(0),
        height: (rect.height + 2 * by).max(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: Px, y: Px, width: Px, height: Px) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    type Bounds = (f64, f64, f64, f64);

    #[test]
    fn enclosing_rect_rounds_outward() {
        let cases: &[(&str, Bounds, Rect)] = &[
            ("whole", (10.0, 4.0, 80.0, 24.0), rect(10, 4, 80, 24)),
            (
                "fractional origin",
                (10.5, 4.25, 80.0, 24.0),
                rect(10, 4, 81, 25),
            ),
            (
                "fractional size",
                (0.0, 0.0, 47.2, 47.9),
                rect(0, 0, 48, 48),
            ),
            (
                "negative origin",
                (-0.5, -2.2, 1.0, 1.0),
                rect(-1, -3, 2, 2),
            ),
            (
                "negative size clamps",
                (3.0, 3.0, -5.0, 2.0),
                rect(3, 3, 0, 2),
            ),
        ];
        for (name, (x, y, w, h), want) in cases {
            assert_eq!(enclosing_rect(*x, *y, *w, *h), *want, "{name}");
        }
    }

    #[test]
    fn inflate_grows_and_shrinks_symmetrically() {
        let cases: &[(Rect, Px, Rect)] = &[
            (rect(10, 10, 20, 20), 4, rect(6, 6, 28, 28)),
            (rect(10, 10, 20, 20), 0, rect(10, 10, 20, 20)),
            (rect(10, 10, 20, 20), -5, rect(15, 15, 10, 10)),
            (rect(10, 10, 4, 4), -5, rect(15, 15, 0, 0)),
        ];
        for (before, by, want) in cases {
            assert_eq!(inflate(*before, *by), *want, "{before:?} by {by}");
        }
    }
}
