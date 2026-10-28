//! The thumb's show / hold / fade / expand timeline: one surface's `Indicator`.

use super::geometry::{Metrics, ThumbPaint, geometry};
use crate::scroll::config::{ScrollbarSettings, ScrollbarVisibility};
use blitz_kit::scroll::geom::Latch;
use blitz_kit::scroll::smooth;
use blitz_kit::scroll::time::Elapsed;

/// What the indicator needs next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wake {
    /// It fades or expands: a frame per callback.
    Frames,
    /// Nothing moves until then (the end of the hold before the fade).
    At(Elapsed),
    /// Hidden, or held for good.
    Never,
}

/// One surface's thumb: which scroller it shows, when that last moved, and the hover.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Indicator {
    pub latch: Option<Latch>,
    last_change: Option<Elapsed>,
    /// The pointer is over the thumb's track zone.
    hovered: bool,
    /// The expansion when the hover last changed, and when.
    expand_from: f64,
    expand_since: Option<Elapsed>,
}

fn secs(from: Elapsed, to: Elapsed) -> f64 {
    to.0.as_secs_f64() - from.0.as_secs_f64()
}

impl Indicator {
    /// `latch`'s offset changed at `now`: show its thumb at full opacity.
    pub(crate) fn changed(self, latch: Latch, now: Elapsed) -> Indicator {
        let same = self.latch == Some(latch);
        Indicator {
            latch: Some(latch),
            last_change: Some(now),
            hovered: if same { self.hovered } else { false },
            expand_from: if same { self.expand_from } else { 0.0 },
            expand_since: if same { self.expand_since } else { None },
        }
    }

    /// The pointer moved onto or off the track zone at `now`.
    pub(crate) fn hover(self, hovered: bool, s: &ScrollbarSettings, now: Elapsed) -> Indicator {
        if self.hovered == hovered || self.latch.is_none() {
            return self;
        }
        let visible = self.opacity(s, ScrollbarVisibility::WhenScrolling, now) > 0.0;
        if hovered && !visible {
            return self;
        }
        Indicator {
            expand_from: self.expansion(s, now),
            expand_since: Some(now),
            hovered,
            // Leaving the track restarts the hold, as a scroll would.
            last_change: if hovered { self.last_change } else { Some(now) },
            ..self
        }
    }

    /// The thumb's opacity at `now` (§11.5.6): 1 for the hold, then a linear fade.
    pub(crate) fn opacity(
        &self,
        s: &ScrollbarSettings,
        vis: ScrollbarVisibility,
        now: Elapsed,
    ) -> f64 {
        let Some(changed) = self.last_change else {
            return 0.0;
        };
        if vis == ScrollbarVisibility::Always || self.hovered {
            return 1.0;
        }
        let t = secs(changed, now);
        let hold = s.hide_delay_ms.secs();
        let fade = s.fade_duration_ms.secs();
        if t < hold {
            1.0
        } else if fade > 0.0 {
            (1.0 - (t - hold) / fade).max(0.0)
        } else {
            0.0
        }
    }

    /// The expansion at `now`, 0 at rest and 1 hovered, eased over the expand duration.
    pub(crate) fn expansion(&self, s: &ScrollbarSettings, now: Elapsed) -> f64 {
        let target = if self.hovered { 1.0 } else { 0.0 };
        let Some(since) = self.expand_since else {
            return target;
        };
        let dur = s.expand_duration_ms.secs();
        let p = if dur > 0.0 {
            secs(since, now) / dur
        } else {
            1.0
        };
        self.expand_from + (target - self.expand_from) * smooth::e_out(p)
    }

    /// What the indicator needs after a frame at `now`.
    pub(crate) fn wake(
        &self,
        s: &ScrollbarSettings,
        vis: ScrollbarVisibility,
        now: Elapsed,
    ) -> Wake {
        let Some(changed) = self.last_change else {
            return Wake::Never;
        };
        let expanding = self
            .expand_since
            .is_some_and(|since| secs(since, now) < s.expand_duration_ms.secs());
        if expanding {
            return Wake::Frames;
        }
        if vis == ScrollbarVisibility::Always || self.hovered {
            return Wake::Never;
        }
        let t = secs(changed, now);
        let hold = s.hide_delay_ms.secs();
        if t < hold {
            Wake::At(Elapsed(changed.0 + s.hide_delay_ms.duration()))
        } else if t < hold + s.fade_duration_ms.secs() {
            Wake::Frames
        } else {
            Wake::Never
        }
    }

    /// What to paint for `metrics` at `now`; `None` once faded out.
    pub(crate) fn paint(
        &self,
        metrics: &Metrics,
        s: &ScrollbarSettings,
        vis: ScrollbarVisibility,
        now: Elapsed,
    ) -> Option<ThumbPaint> {
        let opacity = self.opacity(s, vis, now);
        if opacity <= 0.0 {
            return None;
        }
        let expand = self.expansion(s, now);
        let (track, thumb) = geometry(metrics, expand, s, vis);
        Some(ThumbPaint {
            thumb,
            track,
            opacity,
            track_opacity: expand * opacity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::tests::metrics;
    use super::*;
    use blitz_kit::scroll::geom::{ScrollAxis, Scroller};
    use std::time::Duration;

    fn at(ms: u64) -> Elapsed {
        Elapsed(Duration::from_millis(ms))
    }

    const LATCH: Latch = Latch {
        scroller: Scroller::Viewport,
        axis: ScrollAxis::Y,
    };

    #[test]
    fn the_fade_of_11_8_item_10() {
        let s = ScrollbarSettings::default();
        let vis = ScrollbarVisibility::WhenScrolling;
        let ind = Indicator::default();
        assert_eq!(ind.opacity(&s, vis, at(0)), 0.0, "hidden before any scroll");
        let ind = ind.changed(LATCH, at(1000));
        assert_eq!(
            ind.opacity(&s, vis, at(1000)),
            1.0,
            "visible on the changed frame"
        );
        assert_eq!(ind.opacity(&s, vis, at(1499)), 1.0);
        assert_eq!(ind.wake(&s, vis, at(1100)), Wake::At(at(1500)));
        let falling = ind.opacity(&s, vis, at(1508));
        assert!(falling < 1.0 && falling > 0.9, "{falling}");
        assert_eq!(ind.wake(&s, vis, at(1508)), Wake::Frames);
        assert_eq!(ind.opacity(&s, vis, at(1740)), 0.0, "gone at 740 ms");
        assert_eq!(ind.wake(&s, vis, at(1741)), Wake::Never);
        assert_eq!(ind.paint(&metrics(0.0), &s, vis, at(1741)), None);
    }

    #[test]
    fn hovering_expands_and_holds_the_thumb() {
        let s = ScrollbarSettings::default();
        let vis = ScrollbarVisibility::WhenScrolling;
        let ind = Indicator::default()
            .changed(LATCH, at(0))
            .hover(true, &s, at(100));
        assert_eq!(ind.wake(&s, vis, at(200)), Wake::Frames);
        assert!((ind.expansion(&s, at(340)) - 1.0).abs() < 1e-6);
        assert_eq!(ind.opacity(&s, vis, at(5000)), 1.0, "held while hovered");
        let (_, thumb) = geometry(&metrics(0.0), 1.0, &s, vis);
        assert_eq!(thumb.width, 12.0);
        let away = ind.hover(false, &s, at(6000));
        assert_eq!(away.opacity(&s, vis, at(6499)), 1.0);
        assert_eq!(away.opacity(&s, vis, at(6740)), 0.0);
    }
}
