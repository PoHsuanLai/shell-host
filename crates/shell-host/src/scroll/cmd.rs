//! Programmatic scrolls (design/11 §11.3.11): `ScrollCmd`, sent through
//! `SurfaceHandle::scroll`. They run through the engine with the smooth rule (or at once), never
//! through Blitz's own `scroll_to` (whose 300 ms animation would fight the engine).

use crate::region::ElementId;
use crate::scroll::engine::ScrollAnimate;
use crate::scroll::geom::ScrollAxis;

/// The margin `IntoView` keeps between the element and the scroller's edge, px.
pub const INTO_VIEW_MARGIN: f64 = 8.0;

/// A programmatic scroll of the scroll container `element` (an element with that `id`).
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollCmd {
    /// Scroll `element` on `axis` to `offset`.
    To {
        element: ElementId,
        axis: ScrollAxis,
        offset: f64,
        animate: ScrollAnimate,
    },
    /// Scroll `element` on `axis` by `delta`.
    By {
        element: ElementId,
        axis: ScrollAxis,
        delta: f64,
        animate: ScrollAnimate,
    },
    /// Scroll the nearest vertical scroll container of `element` until it is in view, with
    /// [`INTO_VIEW_MARGIN`] to spare.
    IntoView {
        element: ElementId,
        animate: ScrollAnimate,
    },
}

/// The offset that brings a child spanning `child_start..child_end` into a view spanning
/// `view_start..view_end` (both in the same coordinates, the view scrolled to `offset`) with
/// `margin` to spare; `None` if it is already fully in view. Clamped to `0..=max`.
pub fn into_view(
    view: (f64, f64),
    child: (f64, f64),
    offset: f64,
    max: f64,
    margin: f64,
) -> Option<f64> {
    let (view_start, view_end) = view;
    let (child_start, child_end) = child;
    let wanted = if child_start - margin < view_start {
        offset - (view_start - (child_start - margin))
    } else if child_end + margin > view_end {
        let by = (child_end + margin) - view_end;
        // A child taller than the view shows its start.
        let room = (child_start - margin) - view_start;
        offset + by.min(room)
    } else {
        return None;
    };
    Some(wanted.clamp(0.0, max.max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_view_scrolls_the_least_that_shows_the_child() {
        let view = (100.0, 500.0);
        let cases = [
            ("already visible", (200.0, 300.0), 50.0, None),
            ("below", (520.0, 560.0), 50.0, Some(50.0 + 68.0)),
            ("above", (60.0, 90.0), 50.0, Some(50.0 - 48.0)),
            ("above the top: clamped", (60.0, 90.0), 10.0, Some(0.0)),
            (
                "taller than the view: its start",
                (300.0, 900.0),
                0.0,
                Some(192.0),
            ),
        ];
        for (name, child, offset, want) in cases {
            assert_eq!(
                into_view(view, child, offset, 1000.0, INTO_VIEW_MARGIN),
                want,
                "{name}"
            );
        }
    }
}
