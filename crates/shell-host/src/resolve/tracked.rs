//! Remembering what was last sent to the compositor.

use super::spec::Resolved;
use crate::region::Rect;

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

    const PILL_RECT: Rect = Rect {
        x: 20,
        y: 8,
        width: 100,
        height: 24,
    };

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
