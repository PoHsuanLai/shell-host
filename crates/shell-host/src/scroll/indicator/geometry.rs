//! The thumb's geometry from a scroller's rect and scroll metrics, and the zone and drag maths.

use super::fade::Hover;
use crate::scroll::config::{ScrollbarSettings, ScrollbarVisibility};
use blitz_kit::scroll::geom::{Area, ScrollAxis};

/// The thumb sits this far inside its track, px.
const INSET: f64 = 2.0;

/// One scroller's metrics on the thumb's axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    /// The scroller's border box.
    pub rect: Area,
    pub axis: ScrollAxis,
    /// Its viewport extent on the axis.
    pub viewport: f64,
    /// The largest in-range offset.
    pub max: f64,
    /// The raw offset (out of range while stretched).
    pub offset: f64,
}

/// What to paint for one thumb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThumbPaint {
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
pub(crate) fn thumb_span(m: &Metrics, length: f64, min: f64) -> (f64, f64) {
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
pub(crate) fn geometry(
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
    let r = m.rect;
    let min = s.thumb_min_length_px.get();
    match m.axis {
        ScrollAxis::Y => {
            let length = (r.height - 2.0 * INSET).max(0.0);
            let (len, pos) = thumb_span(m, length, min);
            let track = Area {
                x: r.x + r.width - track_w,
                y: r.y,
                width: track_w,
                height: r.height,
            };
            let thumb = Area {
                x: r.x + r.width - INSET - thumb_w,
                y: r.y + INSET + pos,
                width: thumb_w,
                height: len,
            };
            (track, thumb)
        }
        ScrollAxis::X => {
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

/// Whether surface-local point `(x, y)` is within the expand zone of a thumb on `metrics`.
pub(crate) fn in_zone(metrics: &Metrics, x: f64, y: f64, s: &ScrollbarSettings) -> Hover {
    let r = metrics.rect;
    let zone = s.expand_zone_px.get();
    let inside = match metrics.axis {
        ScrollAxis::Y => {
            x >= r.x + r.width - zone && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
        }
        ScrollAxis::X => {
            y >= r.y + r.height - zone && y <= r.y + r.height && x >= r.x && x <= r.x + r.width
        }
    };
    if inside { Hover::Hovered } else { Hover::Away }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::scroll::indicator) fn metrics(offset: f64) -> Metrics {
        Metrics {
            rect: Area {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 1000.0,
            },
            axis: ScrollAxis::Y,
            viewport: 1000.0,
            max: 3000.0,
            offset,
        }
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
    fn the_zone_is_the_16_px_along_the_edge() {
        let s = ScrollbarSettings::default();
        let m = metrics(0.0);
        assert_eq!(in_zone(&m, 390.0, 500.0, &s), Hover::Hovered);
        assert_eq!(in_zone(&m, 380.0, 500.0, &s), Hover::Away);
        assert_eq!(in_zone(&m, 390.0, 1200.0, &s), Hover::Away);
    }
}
