//! One layer surface: its wgpu swapchain, its document, its scene and its frame state.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use dioxus_native_dom::DioxusDocument;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

use super::doc::Content;
use super::gpu::{FORMAT, Gpu};
use super::stats::Stats;

/// Frame pacing: at most one frame in flight; a request while one is in flight is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Idle,
    InFlight,
    InFlightDirty,
}

/// A layer surface and everything that renders into it. Field order is drop order: the wgpu
/// surface must go before the `wl_surface` it presents into (see `raw_handle`).
pub struct SpikeSurface {
    pub wgpu: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    pub blur: Option<ExtBackgroundEffectSurfaceV1>,
    pub fractional: Option<WpFractionalScaleV1>,
    pub viewport: Option<WpViewport>,
    pub layer: LayerSurface,
    pub name: &'static str,
    pub content: Content,
    pub doc: DioxusDocument,
    pub scene: vello_hybrid::Scene,
    pub redraw: Arc<AtomicBool>,
    pub logical: (u32, u32),
    /// Preferred scale in 120ths; 120 until the compositor says otherwise.
    pub scale120: u32,
    pub buffer: (u32, u32),
    pub configured: bool,
    pub pace: Pace,
    pub clock: Instant,
    pub stats: Stats,
}

impl SpikeSurface {
    pub fn scale(&self) -> f32 {
        self.scale120 as f32 / 120.0
    }

    /// (Re)configures the swapchain for the current buffer size.
    pub fn configure_swapchain(&mut self, gpu: &Gpu) {
        let Some(surface) = &self.wgpu else { return };
        let caps = surface.get_capabilities(&gpu.handle.adapter);
        let config = match &self.config {
            Some(c) => wgpu::SurfaceConfiguration {
                width: self.buffer.0,
                height: self.buffer.1,
                ..c.clone()
            },
            None => wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: FORMAT,
                width: self.buffer.0,
                height: self.buffer.1,
                present_mode: choose_present(&caps.present_modes),
                desired_maximum_frame_latency: 2,
                alpha_mode: choose_alpha(&caps.alpha_modes),
                view_formats: vec![],
            },
        };
        if self.config.is_none() {
            eprintln!(
                "[{}] caps formats={:?} present={:?} alpha={:?} -> present={:?} alpha={:?}",
                self.name,
                caps.formats,
                caps.present_modes,
                caps.alpha_modes,
                config.present_mode,
                config.alpha_mode
            );
        }
        surface.configure(&gpu.handle.device, &config);
        self.config = Some(config);
        self.scene = vello_hybrid::Scene::new(self.buffer.0 as u16, self.buffer.1 as u16);
    }

    pub fn wl_surface(&self) -> &wayland_client::protocol::wl_surface::WlSurface {
        self.layer.wl_surface()
    }
}

/// Mailbox, then Fifo (override with `SPIKE_PRESENT=fifo|immediate|mailbox`).
pub fn choose_present(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    let want = match std::env::var("SPIKE_PRESENT").as_deref() {
        Ok("fifo") => wgpu::PresentMode::Fifo,
        Ok("immediate") => wgpu::PresentMode::Immediate,
        _ => wgpu::PresentMode::Mailbox,
    };
    if modes.contains(&want) {
        want
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// PreMultiplied (what vello_hybrid emits), else PostMultiplied, else whatever is offered.
pub fn choose_alpha(modes: &[wgpu::CompositeAlphaMode]) -> wgpu::CompositeAlphaMode {
    [
        wgpu::CompositeAlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::PostMultiplied,
    ]
    .into_iter()
    .find(|m| modes.contains(m))
    .or_else(|| modes.first().copied())
    .unwrap_or(wgpu::CompositeAlphaMode::Auto)
}
