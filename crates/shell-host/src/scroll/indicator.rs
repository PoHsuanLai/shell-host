//! The overlay thumb (design/11 §11.3.12, §11.5.6): its geometry from a scroller's rect and
//! scroll metrics, and its show / hold / fade / expand timeline. Pure; `scroll::paint` draws
//! what this computes.

use crate::scroll::config::{ScrollbarSettings, ScrollbarVisibility};
use crate::scroll::geom::{Axis, Latch};
use crate::scroll::smooth;
use crate::tick::HostInstant;

/// The thumb sits this far inside its track, px.
const INSET: f64 = 2.0;

/// A rectangle in document logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Whether the other axis's track is showing (the vertical track then stops short).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherAxis {
    Shown,
    Hidden,
}

/// One scroller's metrics on the thumb's axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// The scroller's border box.
    pub rect: Area,
    pub axis: Axis,
    /// Its viewport extent on the axis.
    pub viewport: f64,
    /// The largest in-range offset.
    pub max: f64,
    /// The raw offset (out of range while stretched).
    pub offset: f64,
    pub other: OtherAxis,
}

/// What to paint for one thumb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThumbPaint {
    pub thumb: Area,
    /// The track, shown only while the thumb is expanded (hovered); zero opacity otherwise.
    pub track: Area,
    /// 0..=1, multiplied into the thumb colour.
    pub opacity: f64,
    /// 0..=1, how far the track has faded in with the expansion.
    pub track_opacity: f64,
}

/// The thumb's length and its position along a track of `length` px (§11.5.6), pinned and
/// shortened while stretched.
pub fn thumb_span(m: &Metrics, length: f64, min: f64) -> (f64, f64) {
    let content = m.max + m.viewport;
    let base = if content > 0.0 {
        (length * m.viewport / content).max(min).min(length)
    } else {
        length
    };
    if m.offset < 0.0 {
        let len = shortened(base, -m.offset, m.viewport, min);
        return (len, 0.0);
    }
    if m.offset > m.max {
        let len = shortened(base, m.offset - m.max, m.viewport, min);
        return (len, length - len);
    }
    let progress = if m.max > 0.0 { m.offset / m.max } else { 0.0 };
    (base, (length - base) * progress)
}

fn shortened(len: f64, stretch: f64, viewport: f64, min: f64) -> f64 {
    if viewport <= 0.0 {
        return len;
    }
    (len * (1.0 - stretch / viewport)).max(min)
}

/// The track and thumb rectangles for `m` at expansion `expand` (0 at rest, 1 hovered).
pub fn geometry(
    m: &Metrics,
    expand: f64,
    s: &ScrollbarSettings,
    vis: ScrollbarVisibility,
) -> (Area, Area) {
    let lerp = |a: f64, b: f64| a + (b - a) * expand;
    let (track_w, thumb_w) = match vis {
        ScrollbarVisibility::Always => {
            let w = s.legacy_track_width_px.get();
            (w, w - 2.0 * INSET)
        }
        ScrollbarVisibility::WhenScrolling => (
            lerp(s.track_thickness_px.get(), s.track_thickness_hover_px.get()),
            lerp(s.thumb_thickness_px.get(), s.thumb_thickness_hover_px.get()),
        ),
    };
    let short = match (m.other, m.axis) {
        (OtherAxis::Shown, Axis::Y) => s.other_axis_offset_px.get(),
        _ => 0.0,
    };
    let r = m.rect;
    let min = s.thumb_min_length_px.get();
    match m.axis {
        Axis::Y => {
            let length = (r.height - 2.0 * INSET - short).max(0.0);
            let (len, pos) = thumb_span(m, length, min);
            let track = Area {
                x: r.x + r.width - track_w,
                y: r.y,
                width: track_w,
                height: r.height - short,
            };
            let thumb = Area {
                x: r.x + r.width - INSET - thumb_w,
                y: r.y + INSET + pos,
                width: thumb_w,
                height: len,
            };
            (track, thumb)
        }
        Axis::X => {
            let length = (r.width - 2.0 * INSET).max(0.0);
            let (len, pos) = thumb_span(m, length, min);
            let track = Area {
                x: r.x,
                y: r.y + r.height - track_w,
                width: r.width,
                height: track_w,
            };
            let thumb = Area {
                x: r.x + INSET + pos,
                y: r.y + r.height - INSET - thumb_w,
                width: len,
                height: thumb_w,
            };
            (track, thumb)
        }
    }
}

/// Whether the pointer is over the thumb's track zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    Hovered,
    Away,
}

/// What the indicator needs next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// It fades or expands: a frame per callback.
    Frames,
    /// Nothing moves until then (the end of the hold before the fade).
    At(HostInstant),
    /// Hidden, or held for good.
    Never,
}

/// One surface's thumb: which scroller it shows, when that last moved, and the hover.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Indicator {
    pub latch: Option<Latch>,
    last_change: Option<HostInstant>,
    hover: Option<Hover>,
    /// The expansion when the hover last changed, and when.
    expand_from: f64,
    expand_since: Option<HostInstant>,
}

fn secs(from: HostInstant, to: HostInstant) -> f64 {
    to.0.as_secs_f64() - from.0.as_secs_f64()
}

impl Indicator {
    /// `latch`'s offset changed at `now`: show its thumb at full opacity.
    pub fn changed(self, latch: Latch, now: HostInstant) -> Indicator {
        let same = self.latch == Some(latch);
        Indicator {
            latch: Some(latch),
            last_change: Some(now),
            hover: if same { self.hover } else { None },
            expand_from: if same { self.expand_from } else { 0.0 },
            expand_since: if same { self.expand_since } else { None },
        }
    }

    /// The pointer moved onto or off the track zone at `now`.
    pub fn hover(self, hover: Hover, s: &ScrollbarSettings, now: HostInstant) -> Indicator {
        let current = self.hover.unwrap_or(Hover::Away);
        if current == hover || self.latch.is_none() {
            return self;
        }
        let visible = self.opacity(s, ScrollbarVisibility::WhenScrolling, now) > 0.0;
        if hover == Hover::Hovered && !visible {
            return self;
        }
        Indicator {
            expand_from: self.expansion(s, now),
            expand_since: Some(now),
            hover: Some(hover),
            // Leaving the track restarts the hold, as a scroll would.
            last_change: match hover {
                Hover::Away => Some(now),
                Hover::Hovered => self.last_change,
            },
            ..self
        }
    }

    /// The thumb's opacity at `now` (§11.5.6): 1 for the hold, then a linear fade.
    pub fn opacity(
        &self,
        s: &ScrollbarSettings,
        vis: ScrollbarVisibility,
        now: HostInstant,
    ) -> f64 {
        let Some(changed) = self.last_change else {
            return 0.0;
        };
        if vis == ScrollbarVisibility::Always || self.hover == Some(Hover::Hovered) {
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
    pub fn expansion(&self, s: &ScrollbarSettings, now: HostInstant) -> f64 {
        let target = match self.hover {
            Some(Hover::Hovered) => 1.0,
            Some(Hover::Away) | None => 0.0,
        };
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
    pub fn wake(&self, s: &ScrollbarSettings, vis: ScrollbarVisibility, now: HostInstant) -> Wake {
        let Some(changed) = self.last_change else {
            return Wake::Never;
        };
        let expanding = self
            .expand_since
            .is_some_and(|since| secs(since, now) < s.expand_duration_ms.secs());
        if expanding {
            return Wake::Frames;
        }
        if vis == ScrollbarVisibility::Always || self.hover == Some(Hover::Hovered) {
            return Wake::Never;
        }
        let t = secs(changed, now);
        let hold = s.hide_delay_ms.secs();
        if t < hold {
            Wake::At(HostInstant(changed.0 + s.hide_delay_ms.duration()))
        } else if t < hold + s.fade_duration_ms.secs() {
            Wake::Frames
        } else {
            Wake::Never
        }
    }

    /// What to paint for `metrics` at `now`; `None` once faded out.
    pub fn paint(
        &self,
        metrics: &Metrics,
        s: &ScrollbarSettings,
        vis: ScrollbarVisibility,
        now: HostInstant,
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

/// Whether surface-local point `(x, y)` is within the expand zone of a thumb on `metrics`.
pub fn in_zone(metrics: &Metrics, x: f64, y: f64, s: &ScrollbarSettings) -> Hover {
    let r = metrics.rect;
    let zone = s.expand_zone_px.get();
    let inside = match metrics.axis {
        Axis::Y => {
            x >= r.x + r.width - zone && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
        }
        Axis::X => {
            y >= r.y + r.height - zone && y <= r.y + r.height && x >= r.x && x <= r.x + r.width
        }
    };
    if inside { Hover::Hovered } else { Hover::Away }
}

/// The offset a thumb drag of `delta` px along a track of `length` gives, from `offset0`
/// (§11.3.12): content moves `max / (length - len)` px per thumb px; clamped, never stretched.
pub fn drag_offset(m: &Metrics, offset0: f64, delta: f64, length: f64, len: f64) -> f64 {
    let free = length - len;
    if free <= 0.0 {
        return offset0.clamp(0.0, m.max.max(0.0));
    }
    (offset0 + delta * m.max / free).clamp(0.0, m.max.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scroll::geom::Scroller;
    use std::time::Duration;

    fn at(ms: u64) -> HostInstant {
        HostInstant(Duration::from_millis(ms))
    }

    fn metrics(offset: f64) -> Metrics {
        Metrics {
            rect: Area {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 1000.0,
            },
            axis: Axis::Y,
            viewport: 1000.0,
            max: 3000.0,
            offset,
            other: OtherAxis::Hidden,
        }
    }

    const LATCH: Latch = Latch {
        scroller: Scroller::Viewport,
        axis: Axis::Y,
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
    fn the_thumb_shortens_to_75_percent_at_a_quarter_stretch() {
        let s = ScrollbarSettings::default();
        let length = 1000.0 - 4.0;
        let (in_range, _) = thumb_span(&metrics(3000.0), length, 18.0);
        let (stretched, pos) = thumb_span(&metrics(3250.0), length, 18.0);
        assert!(
            (stretched / in_range - 0.75).abs() < 1e-9,
            "{stretched} / {in_range}"
        );
        assert!(
            (pos + stretched - length).abs() < 1e-9,
            "pinned to the bottom"
        );
        let (top, pos) = thumb_span(&metrics(-250.0), length, 18.0);
        assert_eq!((top, pos), (stretched, 0.0), "pinned to the top");
        let (_, thumb) = geometry(
            &metrics(1500.0),
            0.0,
            &s,
            ScrollbarVisibility::WhenScrolling,
        );
        assert_eq!((thumb.x, thumb.width), (400.0 - 2.0 - 8.0, 8.0));
        let (tiny, _) = thumb_span(
            &Metrics {
                max: 1_000_000.0,
                ..metrics(0.0)
            },
            length,
            18.0,
        );
        assert_eq!(tiny, 18.0, "never under the minimum");
    }

    #[test]
    fn hovering_expands_and_holds_the_thumb() {
        let s = ScrollbarSettings::default();
        let vis = ScrollbarVisibility::WhenScrolling;
        let ind = Indicator::default()
            .changed(LATCH, at(0))
            .hover(Hover::Hovered, &s, at(100));
        assert_eq!(ind.wake(&s, vis, at(200)), Wake::Frames);
        assert!((ind.expansion(&s, at(340)) - 1.0).abs() < 1e-6);
        assert_eq!(ind.opacity(&s, vis, at(5000)), 1.0, "held while hovered");
        let (_, thumb) = geometry(&metrics(0.0), 1.0, &s, vis);
        assert_eq!(thumb.width, 12.0);
        let away = ind.hover(Hover::Away, &s, at(6000));
        assert_eq!(away.opacity(&s, vis, at(6499)), 1.0);
        assert_eq!(away.opacity(&s, vis, at(6740)), 0.0);
    }

    #[test]
    fn a_thumb_drag_maps_track_px_to_content_px() {
        let m = metrics(0.0);
        let length = 996.0;
        let (len, _) = thumb_span(&m, length, 18.0);
        let moved = drag_offset(&m, 0.0, 10.0, length, len);
        assert!((moved - 10.0 * 3000.0 / (length - len)).abs() < 1e-9);
        assert_eq!(drag_offset(&m, 0.0, -50.0, length, len), 0.0);
        assert_eq!(drag_offset(&m, 2900.0, 500.0, length, len), 3000.0);
    }

    #[test]
    fn the_zone_is_the_16_px_along_the_edge() {
        let s = ScrollbarSettings::default();
        let m = metrics(0.0);
        assert_eq!(in_zone(&m, 390.0, 500.0, &s), Hover::Hovered);
        assert_eq!(in_zone(&m, 380.0, 500.0, &s), Hover::Away);
        assert_eq!(in_zone(&m, 390.0, 1200.0, &s), Hover::Away);
    }
}
