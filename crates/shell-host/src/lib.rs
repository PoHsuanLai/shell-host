//! Blitz documents on Wayland layer-shell surfaces, xdg popups and xdg toplevels, sharing one
//! GPU device. Portable to any Wayland compositor; see DESIGN.md for the module map.

pub mod blur;
pub mod bridge;
pub mod clipboard;
pub mod clock;
pub mod config;
pub mod ctx;
pub mod cursor;
pub mod debug;
pub mod dom;
pub mod error;
pub mod event;
pub mod frame;
pub mod geometry;
pub mod gpu;
pub mod handle;
pub mod handlers;
#[cfg(feature = "headless")]
pub mod headless;
pub mod host;
pub mod host_handle;
pub mod id;
pub mod ime;
#[cfg(feature = "debug-inject")]
pub mod inject;
pub mod input;
pub mod layer;
pub mod msg;
pub mod output;
pub mod placement;
pub mod popup;
pub mod region;
pub mod resolve;
pub mod role;
pub mod root;
pub mod scale;
#[cfg(feature = "spike")]
pub mod spike;
pub mod state;
pub mod surface;
pub mod timers;
pub mod toplevel;
pub mod wake;

pub use blitz_traits::shell::ColorScheme;
pub use blur::BlurSpec;
pub use bridge::use_watch;
pub use config::{AdapterPref, Backend, HostConfig};
pub use ctx::HostCtx;
pub use debug::{FrameStats, Percentiles, SurfaceName};
pub use error::{HostError, Recovery, Result};
pub use event::{CloseReason, Focus, HostEvent};
pub use geometry::{Margin, Point};
pub use handle::{SurfaceHandle, use_surface};
pub use host::Host;
pub use host_handle::HostHandle;
pub use id::{OutputId, SurfaceId};
pub use layer::{
    Anchor, ExclusiveZone, Extent, KeyboardMode, Layer, LayerConfig, LayerSize, OutputTarget,
};
pub use msg::{SurfaceOp, Visibility};
pub use output::OutputInfo;
pub use placement::PopupPlacement;
pub use popup::{Adjust, Edge, Grab, Gravity, PopupAnchor, PopupConfig, PopupSize, Toggle};
pub use region::{ElementId, Px, Rect, RegionSpec, RoundedRect};
pub use root::Root;
pub use scale::{Scale, Size};
pub use toplevel::{Activation, Decorations, ToplevelConfig};
