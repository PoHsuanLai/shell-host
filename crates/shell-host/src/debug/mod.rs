//! The frame statistics and debug command vocabulary under their public `debug` paths.

mod command;

pub use crate::id::SurfaceName;
pub use crate::stats::{
    AlphaMode, Dimensions, FrameCounters, FrameStats, Percentiles, PresentMode, StatsWindow,
    percentiles, vm_rss_kib,
};
pub use command::{CommandError, DebugAction, DebugCommand, PointerButton, parse_command};
