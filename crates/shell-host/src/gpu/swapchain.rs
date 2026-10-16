//! A wgpu surface configured on the shared device: Mailbox, else Fifo; PreMultiplied alpha,
//! else PostMultiplied, else opaque (and then blur off).

use crate::error::{HostError, Result};
use crate::gpu::context::{GpuContext, SURFACE_FORMAT};
use crate::scale::{MAX_SURFACE_EXTENT, Size};
use crate::stats::{AlphaMode, PresentMode};

/// One surface's swapchain and scene. The wgpu surface is declared first so it drops first.
pub(crate) struct Swapchain {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: Option<wgpu::SurfaceConfiguration>,
    pub(crate) scene: vello_hybrid::Scene,
}

impl Swapchain {
    /// A swapchain for `surface`, unconfigured until the first `configure`.
    pub(crate) fn new(surface: wgpu::Surface<'static>) -> Swapchain {
        Swapchain {
            surface,
            config: None,
            scene: vello_hybrid::Scene::new(1, 1),
        }
    }

    /// Configure (or reconfigure after `Outdated`/`Lost`) for `buffer` on `gpu`'s device.
    pub(crate) fn configure(&mut self, gpu: &GpuContext, buffer: Size) {
        let (width, height) = pixel_extent(buffer);
        let config = match self.config.take() {
            Some(previous) => wgpu::SurfaceConfiguration {
                width,
                height,
                ..previous
            },
            None => first_config(&self.surface, gpu, width, height),
        };
        self.surface.configure(&gpu.device.device, &config);
        self.config = Some(config);
    }

    /// The buffer size the swapchain is configured for, if it is.
    pub(crate) fn configured_extent(&self) -> Option<(u32, u32)> {
        self.config.as_ref().map(|c| (c.width, c.height))
    }

    /// The next texture to render into, reconfiguring once on `Outdated`/`Lost`.
    pub(crate) fn acquire(
        &mut self,
        gpu: &GpuContext,
        buffer: Size,
    ) -> Result<wgpu::SurfaceTexture> {
        if self.configured_extent() != Some(pixel_extent(buffer)) {
            self.configure(gpu, buffer);
        }
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Ok(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.configure(gpu, buffer);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Ok(t),
                    other => Err(HostError::Render(format!(
                        "acquire after reconfigure: {other:?}"
                    ))),
                }
            }
            other => Err(HostError::Render(format!("acquire: {other:?}"))),
        }
    }
}

/// A buffer size as the unsigned pixel extent wgpu and the renderers take, at least 1x1 and at
/// most [`MAX_SURFACE_EXTENT`]: a zero-sized axis cannot be configured or rendered (and the
/// surface is not visible then), and one beyond the device's texture limit is a validation error.
pub(crate) fn pixel_extent(buffer: Size) -> (u32, u32) {
    let most = u32::try_from(MAX_SURFACE_EXTENT).unwrap_or(1);
    let axis = |v: i32| u32::try_from(v).unwrap_or(0).clamp(1, most);
    (axis(buffer.width), axis(buffer.height))
}

fn first_config(
    surface: &wgpu::Surface<'static>,
    gpu: &GpuContext,
    width: u32,
    height: u32,
) -> wgpu::SurfaceConfiguration {
    let caps = surface.get_capabilities(&gpu.device.adapter);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: SURFACE_FORMAT,
        width,
        height,
        present_mode: choose_present(&caps.present_modes),
        desired_maximum_frame_latency: 2,
        alpha_mode: choose_alpha(&caps.alpha_modes),
        view_formats: vec![],
    };
    eprintln!(
        "shell-host: surface offers present {:?}, alpha {:?}; using {:?}, {:?}",
        caps.present_modes, caps.alpha_modes, config.present_mode, config.alpha_mode
    );
    config
}

/// Mailbox if offered, else Fifo (always offered).
pub(crate) fn choose_present(offered: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if offered.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// PreMultiplied (what vello_hybrid emits), else PostMultiplied, else `Opaque`.
///
/// `Opaque` is taken even when the surface does not list it, only when nothing else is
/// listed at all; a surface that lists only an unusual mode (`Inherit`) gets that mode, which
/// the stats report as opaque.
pub(crate) fn choose_alpha(offered: &[wgpu::CompositeAlphaMode]) -> wgpu::CompositeAlphaMode {
    [
        wgpu::CompositeAlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::PostMultiplied,
        wgpu::CompositeAlphaMode::Opaque,
    ]
    .into_iter()
    .find(|mode| offered.contains(mode))
    .or_else(|| offered.first().copied())
    .unwrap_or(wgpu::CompositeAlphaMode::Opaque)
}

/// The stats name of a wgpu present mode.
pub(crate) fn present_mode_of(mode: wgpu::PresentMode) -> PresentMode {
    match mode {
        wgpu::PresentMode::Mailbox => PresentMode::Mailbox,
        wgpu::PresentMode::Immediate | wgpu::PresentMode::AutoNoVsync => PresentMode::Immediate,
        wgpu::PresentMode::Fifo | wgpu::PresentMode::FifoRelaxed | wgpu::PresentMode::AutoVsync => {
            PresentMode::Fifo
        }
    }
}

/// The stats name of a wgpu alpha mode.
pub(crate) fn alpha_mode_of(mode: wgpu::CompositeAlphaMode) -> AlphaMode {
    match mode {
        wgpu::CompositeAlphaMode::PreMultiplied => AlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::PostMultiplied => AlphaMode::PostMultiplied,
        wgpu::CompositeAlphaMode::Opaque
        | wgpu::CompositeAlphaMode::Inherit
        | wgpu::CompositeAlphaMode::Auto => AlphaMode::Opaque,
    }
}

impl std::fmt::Debug for Swapchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Swapchain")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
