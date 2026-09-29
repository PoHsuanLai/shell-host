//! `ScrollConfig`: every tunable number of the scroll engine and the overlay thumb, one field per
//! settings key of design/22 §3.7 `scroll` and §3.8 `scrollbar`, with the documented defaults.
//! A shell reads its own settings file and hands the host a `ScrollConfig`
//! (`HostCtx::set_scroll_config`); the host never reads a file itself.
//!
//! Field names are the keys without their section, and the serde form is the keys' own (a
//! `[scroll]` / `[scrollbar]` table pair deserializes straight into it). Settled values that
//! design/22 has no key for (the keyboard line, the spring, the rebound end rule) are constants
//! next to the code that uses them (`scroll::keys`, `scroll::rubber`).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A length in logical pixels, as a settings key spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Px(pub u16);

/// A duration in whole milliseconds, as a settings key spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Ms(pub u16);

/// A ratio in thousandths (`550` is 0.55), as a settings key spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Fraction(pub u16);

/// A dimensionless physics constant (a drag exponent, a speed in px/s).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Scalar(pub f32);

impl Px {
    /// As an `f64` for the formulas.
    pub fn get(self) -> f64 {
        f64::from(self.0)
    }
}

impl Ms {
    /// As a `Duration`.
    pub fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.0))
    }

    /// In seconds, for the formulas.
    pub fn secs(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

impl Fraction {
    /// As a ratio.
    pub fn get(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

impl Scalar {
    /// As an `f64` for the formulas.
    pub fn get(self) -> f64 {
        f64::from(self.0)
    }
}

/// `scroll.natural`: whether content follows the fingers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalScroll {
    #[default]
    Natural,
    Traditional,
}

/// `scroll.momentum`: whether a flick glides on after the fingers lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollMomentum {
    #[default]
    On,
    Off,
}

/// `scroll.momentum_model`: the deceleration curve of a glide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollMomentumModel {
    /// `v' = -a v^b` (design/11 §11.5.1).
    #[default]
    MacMouseFix,
    /// `v(t) = v0 r^t_ms`, `r = 0.998`: the longer glide (design/11 R5).
    Ios,
}

/// `scroll.rubber_band`: how far content stretches past an edge under the fingers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollRubberBand {
    /// `s = (1 - 1/(O c/d + 1)) d`.
    #[default]
    Bounded,
    /// `s = O / divisor`.
    Linear,
    /// No stretch: every edge clamps.
    Off,
}

/// `scroll.scrollbars`: when the overlay thumb shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollbarVisibility {
    #[default]
    WhenScrolling,
    /// Never fades, on the legacy 15 px track.
    Always,
}

/// `scrollbar.track_click_mode`: what a press on the track (not the thumb) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollTrackClick {
    #[default]
    PageTowardClick,
    JumpToClick,
}

/// The `scroll` section (design/22 §3.7), plus `scroll.scrollbars` (listed under §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollSettings {
    pub natural: NaturalScroll,
    pub momentum: ScrollMomentum,
    pub momentum_model: ScrollMomentumModel,
    pub momentum_a: Scalar,
    pub momentum_b: Scalar,
    pub momentum_stop_px_s: Scalar,
    pub momentum_start_px_s: Scalar,
    pub momentum_cap_px_s: Scalar,
    pub velocity_window_ms: Ms,
    pub velocity_held_still_ms: Ms,
    pub rubber_band: ScrollRubberBand,
    pub rubber_band_c: Fraction,
    pub rubber_band_linear_divisor: Scalar,
    pub rubber_band_edge_min_px: Px,
    pub rubber_band_snap_rate: Scalar,
    pub rubber_band_snap_gain: Scalar,
    pub wheel_detent_px: Px,
    pub wheel_burst_window_ms: Ms,
    pub momentum_boost_threshold_px_s: Scalar,
    pub double_scroll_grace_ms: Ms,
    pub scrollbars: ScrollbarVisibility,
}

impl Default for ScrollSettings {
    fn default() -> ScrollSettings {
        ScrollSettings {
            natural: NaturalScroll::Natural,
            momentum: ScrollMomentum::On,
            momentum_model: ScrollMomentumModel::MacMouseFix,
            momentum_a: Scalar(30.0),
            momentum_b: Scalar(0.7),
            momentum_stop_px_s: Scalar(1.0),
            momentum_start_px_s: Scalar(100.0),
            momentum_cap_px_s: Scalar(12_000.0),
            velocity_window_ms: Ms(80),
            velocity_held_still_ms: Ms(50),
            rubber_band: ScrollRubberBand::Bounded,
            rubber_band_c: Fraction(550),
            rubber_band_linear_divisor: Scalar(20.0),
            rubber_band_edge_min_px: Px(10),
            rubber_band_snap_rate: Scalar(12.5),
            rubber_band_snap_gain: Scalar(0.31),
            wheel_detent_px: Px(60),
            wheel_burst_window_ms: Ms(300),
            momentum_boost_threshold_px_s: Scalar(350.0),
            double_scroll_grace_ms: Ms(150),
            scrollbars: ScrollbarVisibility::WhenScrolling,
        }
    }
}

/// The `scrollbar` section (design/22 §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollbarSettings {
    pub track_thickness_px: Px,
    pub track_thickness_hover_px: Px,
    pub thumb_thickness_px: Px,
    pub thumb_thickness_hover_px: Px,
    pub thumb_min_length_px: Px,
    pub hide_delay_ms: Ms,
    pub fade_duration_ms: Ms,
    pub expand_zone_px: Px,
    pub expand_duration_ms: Ms,
    pub legacy_track_width_px: Px,
    pub other_axis_offset_px: Px,
    pub track_click_mode: ScrollTrackClick,
}

impl Default for ScrollbarSettings {
    fn default() -> ScrollbarSettings {
        ScrollbarSettings {
            track_thickness_px: Px(12),
            track_thickness_hover_px: Px(16),
            thumb_thickness_px: Px(8),
            thumb_thickness_hover_px: Px(12),
            thumb_min_length_px: Px(18),
            hide_delay_ms: Ms(500),
            fade_duration_ms: Ms(240),
            expand_zone_px: Px(16),
            expand_duration_ms: Ms(240),
            legacy_track_width_px: Px(15),
            other_axis_offset_px: Px(12),
            track_click_mode: ScrollTrackClick::PageTowardClick,
        }
    }
}

/// Everything the scroll engine and the thumb are tuned by.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollConfig {
    pub scroll: ScrollSettings,
    pub scrollbar: ScrollbarSettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_design_22s() {
        let config = ScrollConfig::default();
        let s = config.scroll;
        let cases: &[(&str, f64, f64)] = &[
            ("momentum_a", s.momentum_a.get(), 30.0),
            ("momentum_b", s.momentum_b.get(), 0.7),
            ("stop", s.momentum_stop_px_s.get(), 1.0),
            ("start", s.momentum_start_px_s.get(), 100.0),
            ("cap", s.momentum_cap_px_s.get(), 12_000.0),
            ("window", s.velocity_window_ms.secs(), 0.08),
            ("held still", s.velocity_held_still_ms.secs(), 0.05),
            ("c", s.rubber_band_c.get(), 0.55),
            ("divisor", s.rubber_band_linear_divisor.get(), 20.0),
            ("edge min", s.rubber_band_edge_min_px.get(), 10.0),
            ("snap rate", s.rubber_band_snap_rate.get(), 12.5),
            ("snap gain", s.rubber_band_snap_gain.get(), 0.31),
            ("detent", s.wheel_detent_px.get(), 60.0),
            ("burst", s.wheel_burst_window_ms.secs(), 0.3),
            ("boost", s.momentum_boost_threshold_px_s.get(), 350.0),
            ("grace", s.double_scroll_grace_ms.secs(), 0.15),
            ("track", config.scrollbar.track_thickness_px.get(), 12.0),
            (
                "track hover",
                config.scrollbar.track_thickness_hover_px.get(),
                16.0,
            ),
            ("thumb", config.scrollbar.thumb_thickness_px.get(), 8.0),
            (
                "thumb hover",
                config.scrollbar.thumb_thickness_hover_px.get(),
                12.0,
            ),
            ("min", config.scrollbar.thumb_min_length_px.get(), 18.0),
            ("hide", config.scrollbar.hide_delay_ms.secs(), 0.5),
            ("fade", config.scrollbar.fade_duration_ms.secs(), 0.24),
            ("zone", config.scrollbar.expand_zone_px.get(), 16.0),
            ("expand", config.scrollbar.expand_duration_ms.secs(), 0.24),
            ("legacy", config.scrollbar.legacy_track_width_px.get(), 15.0),
            (
                "other axis",
                config.scrollbar.other_axis_offset_px.get(),
                12.0,
            ),
        ];
        for (name, got, want) in cases {
            assert!((got - want).abs() < 1e-6, "{name}: {got} != {want}");
        }
        assert_eq!(s.natural, NaturalScroll::Natural);
        assert_eq!(s.rubber_band, ScrollRubberBand::Bounded);
        assert_eq!(s.scrollbars, ScrollbarVisibility::WhenScrolling);
    }

    #[test]
    fn a_partial_table_keeps_the_other_defaults() {
        let json = r#"{"scroll":{"natural":"traditional","wheel_detent_px":53}}"#;
        let config: ScrollConfig = serde_json::from_str(json).expect("parses");
        assert_eq!(config.scroll.natural, NaturalScroll::Traditional);
        assert_eq!(config.scroll.wheel_detent_px, Px(53));
        assert_eq!(config.scroll.momentum, ScrollMomentum::On);
        assert_eq!(config.scrollbar, ScrollbarSettings::default());
    }
}
