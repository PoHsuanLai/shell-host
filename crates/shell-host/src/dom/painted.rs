//! Where a laid-out box paints: its layout rectangle carried through its own CSS transform and
//! every transformed ancestor's. The pure part.
//!
//! Blitz's `get_client_bounding_rect` is the layout box: `transform` never moves it (layout
//! ignores transforms by design), so a region or popup anchor built from it stays where the box
//! was laid out while the box paints somewhere else (a swiped or leaving notification banner
//! left a grey blur rectangle behind). The paint (`blitz-paint`'s `render_element`) composes
//! `parent * translate(location) * node.transform()` down the tree; the same map, written in
//! page coordinates, is the composition of each transformed box's transform *about its own
//! untransformed origin*, innermost first:
//!
//! ```text
//! painted(p) = F_root(... F_parent(F_node(p)))    F_a(p) = O_a + T_a (p - O_a)
//! ```
//!
//! where `O_a` is box `a`'s untransformed border-box origin and `T_a` its transform (with
//! `transform-origin`, `translate`, `rotate` and `scale` already folded in by Blitz). The
//! rectangle's four corners go through that map, and the result is their bounding box:
//! exact for translation and scale, the axis-aligned bounding box of the turned quad for a
//! rotation or skew (a region is a union of axis-aligned rectangles, so that is the most a
//! region can say). Not handled: 3-D transforms (Blitz drops a non-2-D matrix itself, so the
//! box paints untransformed and the rect agrees), clipping by an ancestor's `overflow`, and
//! opacity (a faded box keeps its region until it is removed).

use crate::dom::hover_sync::Bounds;

/// A 2-D affine map in CSS px, in kurbo's coefficient order: `(x, y)` goes to
/// `(a x + c y + e, b x + d y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2 {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine2 {
    /// The map that moves nothing.
    pub const IDENTITY: Affine2 = Affine2 {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// A move by `(x, y)`.
    pub const fn translate(x: f64, y: f64) -> Affine2 {
        Affine2 {
            e: x,
            f: y,
            ..Affine2::IDENTITY
        }
    }

    /// A scale by `(x, y)` about the origin.
    pub const fn scale(x: f64, y: f64) -> Affine2 {
        Affine2 {
            a: x,
            d: y,
            ..Affine2::IDENTITY
        }
    }

    /// A turn by `radians` (clockwise on screen, y down) about the origin.
    pub fn rotate(radians: f64) -> Affine2 {
        let (sin, cos) = radians.sin_cos();
        Affine2 {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Blitz's stored transform (`Node::transform`, kurbo coefficients) back in CSS px: Blitz
    /// keeps it in device px (`S * T * S^-1`: the translation is multiplied by the viewport
    /// `scale`, the linear part is not), so divide the translation back out. A non-positive
    /// scale is taken as 1.
    pub fn from_device(coeffs: [f64; 6], scale: f64) -> Affine2 {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let [a, b, c, d, e, f] = coeffs;
        Affine2 {
            a,
            b,
            c,
            d,
            e: e / scale,
            f: f / scale,
        }
    }

    fn apply(self, point: Point2) -> Point2 {
        Point2 {
            x: self.a * point.x + self.c * point.y + self.e,
            y: self.b * point.x + self.d * point.y + self.f,
        }
    }
}

/// A point in the same fractional, surface-local CSS px as [`Bounds`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

/// One transformed box on the way from an element up to the root: its untransformed border-box
/// origin, in the coordinates of the rectangle being carried, and its transform about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub origin: Point2,
    pub transform: Affine2,
}

/// Where `bounds` (a layout rectangle) paints, given the transformed boxes from the element
/// itself outward (`chain[0]` is the innermost): the bounding box of its four corners carried
/// through each one in turn. An empty chain returns `bounds` unchanged.
pub fn painted_bounds(bounds: Bounds, chain: &[Placed]) -> Bounds {
    let corners = [
        (bounds.x, bounds.y),
        (bounds.x + bounds.width, bounds.y),
        (bounds.x, bounds.y + bounds.height),
        (bounds.x + bounds.width, bounds.y + bounds.height),
    ]
    .map(|(x, y)| chain.iter().fold(Point2 { x, y }, carry));
    let left = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let top = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// `point` through one box's transform about its origin: `O + T (p - O)`.
fn carry(point: Point2, placed: &Placed) -> Point2 {
    let local = Point2 {
        x: point.x - placed.origin.x,
        y: point.y - placed.origin.y,
    };
    let moved = placed.transform.apply(local);
    Point2 {
        x: moved.x + placed.origin.x,
        y: moved.y + placed.origin.y,
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;

    const fn bounds(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    const fn at(x: f64, y: f64, transform: Affine2) -> Placed {
        Placed {
            origin: Point2 { x, y },
            transform,
        }
    }

    fn close(left: Bounds, right: Bounds) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        near(left.x, right.x)
            && near(left.y, right.y)
            && near(left.width, right.width)
            && near(left.height, right.height)
    }

    /// Every case: a 100 x 40 card at (10, 20) unless the case says otherwise.
    #[test]
    fn a_rect_paints_through_every_transform_on_its_chain() {
        let card = bounds(10.0, 20.0, 100.0, 40.0);
        let cases: Vec<(&str, Bounds, Vec<Placed>, Bounds)> = vec![
            ("no transform", card, vec![], card),
            (
                "identity",
                card,
                vec![at(10.0, 20.0, Affine2::IDENTITY)],
                card,
            ),
            (
                "own translateX(60px)",
                card,
                vec![at(10.0, 20.0, Affine2::translate(60.0, 0.0))],
                bounds(70.0, 20.0, 100.0, 40.0),
            ),
            (
                "a parent's translateX(60px) (sill's swipe)",
                bounds(0.0, 0.0, 100.0, 40.0),
                vec![at(0.0, 0.0, Affine2::translate(60.0, 0.0))],
                bounds(60.0, 0.0, 100.0, 40.0),
            ),
            (
                "own and a parent's translations add",
                card,
                vec![
                    at(10.0, 20.0, Affine2::translate(5.0, -3.0)),
                    at(0.0, 0.0, Affine2::translate(-20.0, 7.0)),
                ],
                bounds(-5.0, 24.0, 100.0, 40.0),
            ),
            (
                "own scale(0.5) about its top-left",
                card,
                vec![at(10.0, 20.0, Affine2::scale(0.5, 0.5))],
                bounds(10.0, 20.0, 50.0, 20.0),
            ),
            (
                "own scale(0.5) about its centre (transform-origin folded in)",
                card,
                vec![at(
                    10.0,
                    20.0,
                    // translate(50, 20) * scale(0.5) * translate(-50, -20)
                    Affine2 {
                        e: 25.0,
                        f: 10.0,
                        ..Affine2::scale(0.5, 0.5)
                    },
                )],
                bounds(35.0, 30.0, 50.0, 20.0),
            ),
            (
                "a parent's scale(2) about the parent's origin moves the child too",
                card,
                vec![at(0.0, 0.0, Affine2::scale(2.0, 2.0))],
                bounds(20.0, 40.0, 200.0, 80.0),
            ),
            (
                "rotate(90deg) about the top-left: the turned box's bounding box",
                card,
                vec![at(10.0, 20.0, Affine2::rotate(FRAC_PI_2))],
                bounds(-30.0, 20.0, 40.0, 100.0),
            ),
            (
                "an exit: translateX(110%) leaves the surface",
                card,
                vec![at(10.0, 20.0, Affine2::translate(110.0, 0.0))],
                bounds(120.0, 20.0, 100.0, 40.0),
            ),
            (
                "an exit: scale(0) collapses to its origin",
                card,
                vec![at(10.0, 20.0, Affine2::scale(0.0, 0.0))],
                bounds(10.0, 20.0, 0.0, 0.0),
            ),
        ];
        for (name, rect, chain, expected) in cases {
            let got = painted_bounds(rect, &chain);
            assert!(close(got, expected), "{name}: {got:?} != {expected:?}");
        }
    }

    #[test]
    fn a_device_transform_comes_back_in_css_px() {
        let cases: &[(&str, [f64; 6], f64, Affine2)] = &[
            (
                "1x",
                [1.0, 0.0, 0.0, 1.0, 60.0, 0.0],
                1.0,
                Affine2::translate(60.0, 0.0),
            ),
            (
                "1.5x: translation divided, linear kept",
                [2.0, 0.0, 0.0, 2.0, 90.0, 30.0],
                1.5,
                Affine2 {
                    e: 60.0,
                    f: 20.0,
                    ..Affine2::scale(2.0, 2.0)
                },
            ),
            (
                "a zero scale is read as 1",
                [1.0, 0.0, 0.0, 1.0, 7.0, 0.0],
                0.0,
                Affine2::translate(7.0, 0.0),
            ),
        ];
        for &(name, coeffs, scale, expected) in cases {
            assert_eq!(Affine2::from_device(coeffs, scale), expected, "{name}");
        }
    }
}
