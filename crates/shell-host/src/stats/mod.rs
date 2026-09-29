//! Frame statistics: what `HostCtx::stats` returns and what the `debug-inject` feature prints as
//! one JSON line per surface: frames, callbacks, commits, sizes, scale and adapter, with the
//! work time (trigger to present) kept apart from the present-to-present interval, because the
//! interval's p95 is only the refresh period and says nothing about cost.

mod frame_stats;
mod percentile;
mod rss;
mod window;

pub use frame_stats::{AlphaMode, Dimensions, FrameCounters, FrameStats, PresentMode};
pub use percentile::{Percentiles, percentiles};
pub use rss::vm_rss_kib;
pub use window::StatsWindow;
