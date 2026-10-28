//! Remembering what was last sent to the compositor.

use crate::region::Rect;

/// Remembers the last resolved region for one region-following surface (an input region, or a
/// blur region), so the caller only rebuilds and recommits a `wl_region` when the geometry
/// actually moved.
///
/// A pending region (`None`) never overwrites what was last recorded — its contract is "keep
/// whatever region the surface has now" — so it always reports no change: there is nothing new
/// to send, whether or not anything was ever resolved before.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Tracked {
    last: Option<Vec<Rect>>,
}

impl Tracked {
    /// A tracker that has not recorded anything yet; its first real resolution always reports
    /// a change.
    pub(crate) fn new() -> Tracked {
        Tracked::default()
    }

    /// A tracker for a region the compositor is already known to hold, so resolving to the
    /// same strips sends nothing: a fresh surface has no blur region, which is
    /// `Tracked::holding(Vec::new())`, and resolving "no blur" on it creates no effect object.
    pub(crate) fn holding(strips: Vec<Rect>) -> Tracked {
        Tracked { last: Some(strips) }
    }

    /// Record `resolved` and report whether the compositor needs a new region (it differs
    /// from the one last recorded).
    pub(crate) fn update(&mut self, resolved: &Option<Vec<Rect>>) -> bool {
        let Some(strips) = resolved else {
            return false;
        };
        if self.last.as_deref() == Some(strips.as_slice()) {
            return false;
        }
        self.last = Some(strips.clone());
        true
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
        let a = Some(vec![PILL_RECT]);
        let b = Some(vec![Rect { x: 21, ..PILL_RECT }]);
        let mut t = Tracked::new();
        assert!(t.update(&a), "first real resolution");
        assert!(!t.update(&a), "same rect again");
        assert!(t.update(&b), "rect moved");
        assert!(!t.update(&b), "settled at the new rect");
        assert!(!t.update(&None), "pending never forces a recommit");
        // Pending did not overwrite what was last recorded: going back to `b` is still "same".
        assert!(!t.update(&b), "pending kept the last real rect");
        assert!(t.update(&a), "b -> a is a real change");

        let mut never_recorded = Tracked::new();
        assert!(
            !never_recorded.update(&None),
            "a tracker that has never recorded anything treats pending as no change"
        );

        let mut holding_nothing = Tracked::holding(Vec::new());
        assert!(
            !holding_nothing.update(&Some(vec![])),
            "a tracker holding nothing does not resend nothing"
        );
        assert!(holding_nothing.update(&Some(vec![PILL_RECT])));
        assert!(holding_nothing.update(&Some(vec![])), "cleared");
        let mut fresh = Tracked::new();
        assert!(fresh.update(&Some(vec![])), "fresh sends");
    }
}
