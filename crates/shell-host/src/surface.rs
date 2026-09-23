//! One surface the host manages: its role, presenter, document, frame state and stats.

use std::sync::Arc;

use calloop::RegistrationToken;

use crate::blur::BlurSpec;
use crate::debug::{AlphaMode, Dimensions, FrameStats, PresentMode, StatsWindow, SurfaceName};
use crate::dom::document::SurfaceDocument;
use crate::dom::shell_provider::HostShellProvider;
use crate::frame::{FrameAction, FrameInput, FrameState};
use crate::gpu::presenter::Presenter;
use crate::id::SurfaceId;
use crate::msg::{SurfaceOp, Visibility};
use crate::region::{Px, RegionSpec};
use crate::resolve::Tracked;
use crate::role::Role;
use crate::scale::{Scale, Size, buffer_size};

/// Whether the compositor has configured the surface yet. Nothing renders before it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Configured {
    Pending,
    Done,
}

/// The size and scale the viewport, the document and the presenter were last set up for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub logical: Size,
    pub scale: Scale,
}

/// The regions last sent to the compositor, so an unchanged region is not re-sent every frame
/// (`resolve::Tracked`). The one path that sends regions is the per-frame
/// `ctx::paint::send_regions`, after layout; configure handlers only invalidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedRegions {
    /// Starts unknown: the compositor's default input region is infinite, so the first
    /// resolution is always sent.
    pub input: Tracked,
    /// Starts empty: a new `wl_surface` has no blur, so "no blur" sends nothing.
    pub blur: Tracked,
}

impl Default for AppliedRegions {
    fn default() -> AppliedRegions {
        AppliedRegions {
            input: Tracked::new(),
            blur: Tracked::holding(Vec::new()),
        }
    }
}

/// A surface. Field order is drop order: the presenter (holding the wgpu surface) goes before
/// the role (holding the `wl_surface`) (SPIKE.md S1 finding 6).
#[derive(Debug)]
pub struct HostSurface {
    pub(crate) presenter: Option<Presenter>,
    pub(crate) role: Role,
    pub(crate) document: SurfaceDocument,
    /// The document's shell provider: its redraw flag, cursor and IME requests.
    pub(crate) shell: Arc<HostShellProvider>,
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
    /// What the viewport, document and presenter were last set up for; `None` before the
    /// first configure and after the role is rebuilt.
    pub(crate) geometry: Option<Geometry>,
    pub(crate) regions: AppliedRegions,
}

impl HostSurface {
    /// The buffer size for the current logical size and scale.
    pub fn buffer(&self) -> Size {
        buffer_size(self.logical, self.scale)
    }

    /// Step the frame machine; the caller performs the returned action.
    pub fn step(&mut self, input: FrameInput) -> FrameAction {
        let (next, action) = crate::frame::step(self.frame, input);
        self.frame = next;
        action
    }

    /// Record a configure: the new logical size (0 on an axis keeps the requested size).
    pub fn configure(&mut self, logical: Size) {
        self.logical = configured_size(self.logical, logical);
        self.configured = Configured::Done;
    }

    /// Record a new preferred scale.
    pub fn rescale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    /// Record `op`'s effect on this surface's own fields; the Wayland requests are the
    /// caller's.
    pub fn record(&mut self, op: &SurfaceOp) {
        match (op, &mut self.role) {
            (SurfaceOp::SetBlur(blur), _) => {
                self.blur = blur.clone();
            }
            (SurfaceOp::SetInputRegion(input), _) => {
                self.input = input.clone();
            }
            (SurfaceOp::SetVisibility(visibility), _) => self.visibility = *visibility,
            (SurfaceOp::Reposition(next), Role::Popup { placement, .. }) => *placement = *next,
            (
                op,
                Role::Layer {
                    config, keyboard, ..
                },
            ) => record_layer(op, config, keyboard),
            // A layer op on a popup or a toplevel, or a reposition of a non-popup: ignored.
            (_, Role::Popup { .. } | Role::Toplevel { .. }) => {}
        }
    }

    /// The stats report over the current sample window (`StatsWindow::peek`); the debug
    /// `stats` command drains the window after printing it.
    pub fn report(&self, adapter: &str) -> FrameStats {
        let (work, interval, samples) = self.stats.peek();
        let (alpha_mode, present_mode) = match &self.presenter {
            Some(presenter) => (presenter.alpha_mode(), presenter.present_mode()),
            None => (AlphaMode::Opaque, PresentMode::Shm),
        };
        let vm_rss_kib = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| crate::debug::vm_rss_kib(&status))
            .unwrap_or(0);
        FrameStats {
            surface: self.name.clone(),
            counters: self.stats.counters(),
            work,
            interval,
            samples,
            scale120: self.scale.0,
            logical: dimensions(self.logical),
            buffer: dimensions(self.buffer()),
            alpha_mode,
            present_mode,
            adapter: adapter.to_owned(),
            devices_created: crate::gpu::context::devices_created(),
            vm_rss_kib,
        }
    }
}

/// A layer op's effect on the recorded config. `keyboard` follows the config unless a grabbing
/// popup has lifted a `None` layer to `OnDemand`, which the popup's close restores.
fn record_layer(
    op: &SurfaceOp,
    config: &mut crate::layer::LayerConfig,
    keyboard: &mut crate::layer::KeyboardMode,
) {
    match op {
        SurfaceOp::SetKeyboard(mode) => {
            config.keyboard = *mode;
            *keyboard = *mode;
        }
        SurfaceOp::SetExclusiveZone(zone) => config.exclusive = *zone,
        SurfaceOp::SetSize(size) => config.size = *size,
        SurfaceOp::SetMargin(margin) => config.margin = *margin,
        SurfaceOp::SetAnchor(anchor) => config.anchor = *anchor,
        SurfaceOp::SetLayer(layer) => config.layer = *layer,
        SurfaceOp::SetBlur(_)
        | SurfaceOp::SetInputRegion(_)
        | SurfaceOp::SetVisibility(_)
        | SurfaceOp::Reposition(_) => {}
    }
}

/// The logical size after a configure of `got`: an axis the compositor left at 0 keeps
/// `requested`. Pure.
pub fn configured_size(requested: Size, got: Size) -> Size {
    let pick = |got: Px, requested: Px| if got > 0 { got } else { requested };
    Size {
        width: pick(got.width, requested.width),
        height: pick(got.height, requested.height),
    }
}

fn dimensions(size: Size) -> Dimensions {
    let px = |v: Px| u32::try_from(v).unwrap_or(0);
    Dimensions {
        width: px(size.width),
        height: px(size.height),
    }
}

/// What changing a surface's visibility takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityChange {
    /// Already in that state: nothing to do.
    Unchanged,
    /// Cancel the watchdog, drop the presenter (the wgpu surface before its `wl_surface`),
    /// replace the role with a fresh unmapped one (destroying the old Wayland objects), and
    /// reset the frame machine. The document and its `VirtualDom` stay alive.
    Unmap,
    /// Send the fresh role's initial commit; the configure that answers it brings the
    /// presenter back.
    Map,
}

/// The change that takes a surface from `from` to `to`. Pure.
pub fn visibility_change(from: Visibility, to: Visibility) -> VisibilityChange {
    match (from, to) {
        (Visibility::Shown, Visibility::Hidden) => VisibilityChange::Unmap,
        (Visibility::Hidden, Visibility::Shown) => VisibilityChange::Map,
        (Visibility::Shown, Visibility::Shown) | (Visibility::Hidden, Visibility::Hidden) => {
            VisibilityChange::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_axis_keeps_the_requested_size() {
        let size = |width, height| Size { width, height };
        let cases = [
            (
                "bar: width from compositor",
                size(0, 32),
                size(1280, 0),
                size(1280, 32),
            ),
            ("both given", size(740, 96), size(700, 90), size(700, 90)),
            ("both zero", size(740, 96), size(0, 0), size(740, 96)),
        ];
        for (name, requested, got, want) in cases {
            assert_eq!(configured_size(requested, got), want, "{name}");
        }
    }
}
