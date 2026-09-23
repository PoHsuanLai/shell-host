//! A wgpu surface configured on the shared device: Mailbox, else Fifo; PreMultiplied alpha,
//! else PostMultiplied, else opaque (and then blur off).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use crate::debug::{AlphaMode, PresentMode};
use crate::gpu::context::GpuContext;
use crate::scale::Size;

/// One surface's swapchain and scene. The wgpu surface is declared first so it drops first.
pub struct Swapchain {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: Option<wgpu::SurfaceConfiguration>,
    pub(crate) scene: vello_hybrid::Scene,
}

impl Swapchain {
    /// A swapchain for `surface`, unconfigured until the first `configure`.
    pub fn new(surface: wgpu::Surface<'static>) -> Swapchain {
        todo!()
    }

    /// Configure (or reconfigure after `Outdated`/`Lost`) for `buffer` on `gpu`'s device.
    pub fn configure(&mut self, gpu: &GpuContext, buffer: Size) {
        todo!()
    }
}

/// Mailbox if offered, else Fifo (always offered).
pub fn choose_present(offered: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    todo!()
}

/// PreMultiplied (what vello_hybrid emits), else PostMultiplied, else `Opaque`.
pub fn choose_alpha(offered: &[wgpu::CompositeAlphaMode]) -> wgpu::CompositeAlphaMode {
    todo!()
}

/// The stats name of a wgpu present mode.
pub fn present_mode_of(mode: wgpu::PresentMode) -> PresentMode {
    todo!()
}

/// The stats name of a wgpu alpha mode.
pub fn alpha_mode_of(mode: wgpu::CompositeAlphaMode) -> AlphaMode {
    todo!()
}

impl std::fmt::Debug for Swapchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Swapchain")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
