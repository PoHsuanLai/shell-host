//! Geometry the host adds to S2's `region` and `scale` types: points, margins, and turning a
//! laid-out element's fractional bounding box into whole logical pixels.
//!
//! Everything is integer logical px ([`Px`]); floats appear only at the Blitz boundary, as
//! arguments, and never in a stored value.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::region::{ElementId, Px, Rect};

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

/// The [`ElementId`] for the DOM element whose `id` attribute is `name`.
///
/// S2 froze `ElementId` as a `u64`, but Blitz finds an element only by its `id` *string*
/// (`get_element_by_id(&str)`), and its id map is private. Until that signature is revisited
/// (FINDINGS.md F3), a region or popup names its element through this function, which hashes the
/// name (FNV-1a, stable across runs) and remembers it so the host can get the string back with
/// [`element_name`].
pub fn element_id(name: &'static str) -> ElementId {
    let id = ElementId(fnv1a(name.as_bytes()));
    names()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id.0, name);
    id
}

/// The `id` attribute an [`ElementId`] from [`element_id`] was made from; `None` for an id that
/// did not come from there.
pub fn element_name(id: ElementId) -> Option<&'static str> {
    names()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id.0)
        .copied()
}

fn names() -> &'static Mutex<HashMap<u64, &'static str>> {
    static NAMES: OnceLock<Mutex<HashMap<u64, &'static str>>> = OnceLock::new();
    NAMES.get_or_init(Mutex::default)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(PRIME))
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

    #[test]
    fn element_ids_are_stable_and_reversible() {
        // FNV-1a of "dock-hit", computed independently: the id must not depend on the run.
        assert_eq!(element_id("dock-hit").0, fnv1a(b"dock-hit"));
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(element_id("dock-pill"), element_id("dock-hit"));
        assert_eq!(element_name(element_id("dock-pill")), Some("dock-pill"));
        assert_eq!(element_name(ElementId(7)), None);
    }
}
