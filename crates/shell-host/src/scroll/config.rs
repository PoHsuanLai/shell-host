//! `ScrollConfig`: the scroll engine's settings (`ScrollSettings`, owned by `blitz_kit`) and the
//! overlay thumb's (`ScrollbarSettings`, design/22 §3.8). A shell reads its own settings file and
//! hands the host a `ScrollConfig` (`HostCtx::set_scroll_config`); the host never reads a file
//! itself.
//!
//! The serde form is the keys' own (a `[scroll]` / `[scrollbar]` table pair deserializes
//! straight into it).

use serde::{Deserialize, Serialize};

pub use blitz_kit::scroll::config::{
    NaturalScroll, ScrollFraction, ScrollMomentum, ScrollMs, ScrollPx, ScrollScalar,
    ScrollSettings, ScrollbarVisibility,
};

/// `scrollbar.track_click_mode`: what a press on the track (not the thumb) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollTrackClick {
    #[default]
    PageTowardClick,
    JumpToClick,
}

/// The `scrollbar` section (design/22 §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollbarSettings {
    pub track_thickness_px: ScrollPx,
    pub track_thickness_hover_px: ScrollPx,
    pub thumb_thickness_px: ScrollPx,
    pub thumb_thickness_hover_px: ScrollPx,
    pub thumb_min_length_px: ScrollPx,
    pub hide_delay_ms: ScrollMs,
    pub fade_duration_ms: ScrollMs,
    pub expand_zone_px: ScrollPx,
    pub expand_duration_ms: ScrollMs,
    pub legacy_track_width_px: ScrollPx,
    pub other_axis_offset_px: ScrollPx,
    pub track_click_mode: ScrollTrackClick,
}

impl Default for ScrollbarSettings {
    fn default() -> ScrollbarSettings {
        ScrollbarSettings {
            track_thickness_px: ScrollPx(12),
            track_thickness_hover_px: ScrollPx(16),
            thumb_thickness_px: ScrollPx(8),
            thumb_thickness_hover_px: ScrollPx(12),
            thumb_min_length_px: ScrollPx(18),
            hide_delay_ms: ScrollMs(500),
            fade_duration_ms: ScrollMs(240),
            expand_zone_px: ScrollPx(16),
            expand_duration_ms: ScrollMs(240),
            legacy_track_width_px: ScrollPx(15),
            other_axis_offset_px: ScrollPx(12),
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
    fn the_scrollbar_defaults_are_design_22s() {
        let config = ScrollConfig::default();
        let cases: &[(&str, f64, f64)] = &[
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
    }

    #[test]
    fn a_partial_table_keeps_the_other_defaults() {
        let json = r#"{"scroll":{"natural":"traditional","wheel_detent_px":53}}"#;
        let config: ScrollConfig = serde_json::from_str(json).expect("parses");
        assert_eq!(config.scroll.natural, NaturalScroll::Traditional);
        assert_eq!(config.scroll.wheel_detent_px, ScrollPx(53));
        assert_eq!(config.scroll.momentum, ScrollMomentum::On);
        assert_eq!(config.scrollbar, ScrollbarSettings::default());
    }
}
