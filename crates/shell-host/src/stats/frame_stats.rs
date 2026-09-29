//! What `stats` reports about one surface, and the vocabulary its fields use.

use serde::{Deserialize, Serialize};

use super::percentile::Percentiles;
use crate::id::SurfaceName;

/// A width and height in pixels, logical or physical by field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// How frames reach the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentMode {
    Mailbox,
    Fifo,
    Immediate,
    /// The CPU backend's `wl_shm` buffers.
    Shm,
}

/// How the compositor blends the surface's alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    PreMultiplied,
    PostMultiplied,
    /// No usable alpha mode: the surface is opaque and its blur is turned off.
    Opaque,
}

/// Cumulative counters for one surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FrameCounters {
    /// Frames rendered and presented.
    pub frames: u64,
    /// Frame callbacks received.
    pub callbacks: u64,
    /// `wl_surface.commit`s (every present is exactly one commit).
    pub commits: u64,
    /// Frames that were due but could not be presented.
    pub skipped: u64,
    /// Watchdog expiries (a compositor that stopped calling back).
    pub watchdog_fired: u64,
    /// Frame ticks delivered to the document (`tick`): frames due while it was animating or
    /// a `FrameDrive` was held.
    #[serde(default)]
    pub ticks: u64,
    /// Renders that ran `HostConfig::prepaint`'s hook after resolving: equal to the frames
    /// resolved with a hook installed (presented or not), and zero without one. A frame that
    /// ran it twice, or an idle period that moves it, is a bug.
    #[serde(default)]
    pub prepaints: u64,
}

/// Everything `stats` reports about one surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameStats {
    pub surface: SurfaceName,
    pub counters: FrameCounters,
    /// Trigger to present, over the window since the previous report.
    pub work: Percentiles,
    /// Present to present, over the same window.
    pub interval: Percentiles,
    /// How many frames the percentiles cover.
    pub samples: u32,
    /// The surface's scale in 120ths.
    pub scale120: u32,
    pub logical: Dimensions,
    pub buffer: Dimensions,
    pub alpha_mode: AlphaMode,
    pub present_mode: PresentMode,
    /// `name (backend, driver)` of the one adapter.
    pub adapter: String,
    /// GPU devices created by this process; stays 1 however many surfaces there are.
    pub devices_created: u32,
    /// Resident set size of the process, KiB.
    pub vm_rss_kib: u64,
}
