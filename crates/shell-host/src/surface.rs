//! One surface the host manages: its role, presenter, document, frame state and stats.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use calloop::RegistrationToken;

use crate::blur::BlurSpec;
use crate::debug::{FrameStats, StatsWindow, SurfaceName};
use crate::dom::document::SurfaceDocument;
use crate::frame::{FrameAction, FrameInput, FrameState};
use crate::gpu::presenter::Presenter;
use crate::id::SurfaceId;
use crate::msg::{SurfaceOp, Visibility};
use crate::region::RegionSpec;
use crate::role::Role;
use crate::scale::{Scale, Size};

/// Whether the compositor has configured the surface yet. Nothing renders before it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Configured {
    Pending,
    Done,
}

/// A surface. Field order is drop order: the presenter (holding the wgpu surface) goes before
/// the role (holding the `wl_surface`) (SPIKE.md S1 finding 6).
#[derive(Debug)]
pub struct HostSurface {
    pub(crate) presenter: Option<Presenter>,
    pub(crate) role: Role,
    pub(crate) document: SurfaceDocument,
    pub(crate) id: SurfaceId,
    pub(crate) name: SurfaceName,
    pub(crate) frame: FrameState,
    pub(crate) watchdog: Option<RegistrationToken>,
    pub(crate) configured: Configured,
    pub(crate) visibility: Visibility,
    pub(crate) logical: Size,
    pub(crate) scale: Scale,
    pub(crate) blur: BlurSpec,
    pub(crate) input: RegionSpec,
    pub(crate) stats: StatsWindow,
}

impl HostSurface {
    /// The buffer size for the current logical size and scale.
    pub fn buffer(&self) -> Size {
        todo!()
    }

    /// Step the frame machine; the caller performs the returned action.
    pub fn step(&mut self, input: FrameInput) -> FrameAction {
        todo!()
    }

    /// Record a configure: the new logical size (0 on an axis keeps the requested size).
    pub fn configure(&mut self, logical: Size) {
        todo!()
    }

    /// Record a new preferred scale.
    pub fn rescale(&mut self, scale: Scale) {
        todo!()
    }

    /// Record `op`'s effect on this surface's own fields; the Wayland requests are the
    /// caller's.
    pub fn record(&mut self, op: &SurfaceOp) {
        todo!()
    }

    /// The stats report over the current sample window (`StatsWindow::peek`); the debug
    /// `stats` command drains the window after printing it.
    pub fn report(&self, adapter: &str) -> FrameStats {
        todo!()
    }
}
