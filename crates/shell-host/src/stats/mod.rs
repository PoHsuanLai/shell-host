//! Frame statistics: what `HostCtx::stats` returns and what the `debug-inject` feature prints as
//! one JSON line per surface: frames, callbacks, commits, sizes, scale and adapter, with the
//! work time (trigger to present) kept apart from the present-to-present interval, because the
//! interval's p95 is only the refresh period and says nothing about cost.

mod frame_stats;
mod percentile;
mod rss;
mod window;

#[cfg(test)]
pub(crate) use frame_stats::FrameCounters;
pub use frame_stats::FrameStats;
pub(crate) use frame_stats::{AlphaMode, Dimensions, PresentMode};
pub use percentile::Percentiles;
pub(crate) use rss::vm_rss_kib;
pub(crate) use window::StatsWindow;
