//! Blitz documents on Wayland layer-shell surfaces, xdg popups and xdg toplevels, sharing one
//! GPU device. Portable to any Wayland compositor. `ARCHITECTURE.md` holds the module map, the
//! layers and the curated root below; every module is private.
//!
//! # Using it
//!
//! A shell builds a [`HostConfig`], calls [`Host::connect`] (which binds the Wayland globals and
//! owns a small tokio runtime for Dioxus tasks), keeps a [`HostHandle`] for messages from other
//! threads, and calls [`Host::run`] with one callback. The callback receives [`HostEvent`]s (the
//! first is `Started`, after the initial outputs) and a [`HostCtx`], through which it spawns
//! surfaces (layer-shell, popups, toplevels, the session lock), each rendering a Dioxus
//! [`HostRoot`] as a Blitz document on one shared GPU device.
//!
//! # Features
//!
//! - `headless`: `headless::render_png`, a document to pixels with no window, no Wayland
//!   connection and no GPU surface (render tests).
//! - `debug-inject`: per-surface debug commands on stdin and `HostCtx::inject` (acceptance
//!   scripts; off in a shipped shell).

mod blur;
mod blur_health;
mod bridge;
mod clipboard;
mod clock;
mod config;
mod csd;
mod ctx;
mod cursor;
mod dom;
mod drag;
mod error;
mod event;
mod frame;
mod geometry;
mod gpu;
mod grab;
mod handle;
mod handlers;
#[cfg(feature = "headless")]
pub mod headless;
mod host;
mod host_handle;
mod id;
mod ime;
#[cfg(feature = "debug-inject")]
pub mod inject;
mod input;
mod layer;
mod live;
mod live_scale;
mod lock;
mod modality;
mod modifiers;
mod msg;
mod output;
mod placement;
mod popup;
mod popup_keyboard;
mod prepaint;
mod region;
mod render_gpu;
mod resolve;
mod role;
mod root;
mod scale;
mod scroll;
mod state;
mod stats;
mod surface;
mod tick;
mod timers;
mod toplevel;
mod wake;

pub use blitz_kit::adapter::AdapterPref;
pub use blitz_kit::fonts::FontFaces;
pub use blitz_kit::scroll::cmd::ScrollCmd;
pub use blitz_kit::scroll::engine::ScrollAnimate;
pub use blitz_kit::scroll::geom::ScrollAxis;
pub use blitz_traits::shell::ColorScheme;
pub use blur::BlurSpec;
pub use bridge::use_watch;
pub use config::{Backend, HostConfig, Ms, registered_families};
pub use ctx::HostCtx;
pub use drag::outcome::{DragAction, DragOutcome, DragRefused};
pub use drag::spec::{BadMime, DragActions, DragIcon, DragPayload, DragSpec, Mime};
pub use error::{HostError, Recovery, Result};
pub use event::{CloseReason, HostEvent, KeyboardFocus};
pub use geometry::{Margin, Point};
pub use grab::GrabRefused;
pub use handle::{
    SurfaceHandle, use_frame_tick, use_input_modality, use_modifiers, use_surface,
    use_surface_scale, use_toplevel_state, use_wheel,
};
pub use host::Host;
pub use host_handle::HostHandle;
pub use id::{OutputId, SurfaceId, SurfaceName};
pub use layer::{
    Anchor, ExclusiveZone, KeyboardMode, LayerConfig, LayerExtent, LayerLevel, LayerSize,
    OutputTarget,
};
pub use lock::handle::{LockHandle, use_lock, use_lock_phase};
pub use lock::machine::{LockPhase, LockState};
pub use lock::{LockConfig, LockError, LockId};
pub use modality::InputModality;
pub use modifiers::{LockKey, Modifier, Modifiers};
pub use msg::{SurfaceOp, Visibility};
pub use output::OutputInfo;
pub use placement::PopupPlacement;
pub use popup::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
    PopupToggle, offset_rect,
};
pub use prepaint::{Prepaint, PrepaintFn};
pub use region::{ElementId, ElementRegion, Px, Rect, RegionSpec, RoundedRect};
pub use root::HostRoot;
pub use scale::{Scale, Size, buffer_size};
pub use scroll::config::{
    NaturalScroll, ScrollConfig, ScrollFraction, ScrollMomentum, ScrollMomentumModel, ScrollMs,
    ScrollPx, ScrollRubberBand, ScrollScalar, ScrollSettings, ScrollTrackClick, ScrollbarSettings,
    ScrollbarVisibility,
};
pub use scroll::watch::{ScrollExtent, ScrollPhase, ScrollPoint, ScrollState, use_scroll};
pub use scroll::wheel::{WheelEvent, WheelMotion, WheelPhase, WheelSource};
pub use stats::{FrameStats, Percentiles};
pub use tick::{FrameDrive, FrameTick, HostInstant};
pub use toplevel::{
    Activation, Decorations, Fullscreen, Maximize, ResizeEdge, ResizeMargin, Tile, TileEdges,
    ToplevelConfig, ToplevelState,
};
