//! How a surface's pixels reach the compositor: a wgpu swapchain on the shared device, or
//! `wl_shm` buffers painted on the CPU. A closed pair, so an enum rather than a trait.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use crate::debug::{AlphaMode, PresentMode};
use crate::dom::document::SurfaceDocument;
use crate::error::Result;
use crate::gpu::context::GpuContext;
use crate::gpu::shm::ShmBuffers;
use crate::gpu::swapchain::Swapchain;
use crate::scale::{Scale, Size};

/// A surface's presenter.
#[derive(Debug)]
pub enum Presenter {
    /// Boxed: a swapchain (wgpu surface, configuration, scene) is far larger than a shm pool.
    Wgpu(Box<Swapchain>),
    Shm(ShmBuffers),
}

/// What one render needs to know about the surface: its buffer size, its scale, and the
/// document time to resolve at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameTarget {
    /// Physical pixels, `scale::buffer_size(logical, scale)`.
    pub buffer: Size,
    pub scale: Scale,
    /// Seconds on the host clock.
    pub time: f64,
}

impl Presenter {
    /// (Re)configure for a new buffer size.
    pub fn resize(&mut self, gpu: Option<&GpuContext>, buffer: Size) {
        todo!()
    }

    /// Resolve `doc` at `target.time`, paint, render, and present. The caller has already
    /// requested the frame callback (`wl_surface.frame`), so the present's commit carries it
    /// (SPIKE.md S1 finding 3). Never waits on the device.
    pub fn present(
        &mut self,
        gpu: Option<&mut GpuContext>,
        doc: &mut SurfaceDocument,
        target: FrameTarget,
    ) -> Result<()> {
        todo!()
    }

    /// Render `doc` offscreen at `target.buffer` and return RGBA8 pixels (debug `capture`).
    pub fn capture(
        &mut self,
        gpu: Option<&mut GpuContext>,
        doc: &mut SurfaceDocument,
        target: FrameTarget,
    ) -> Result<image::RgbaImage> {
        todo!()
    }

    /// The present mode in use, for stats.
    pub fn present_mode(&self) -> PresentMode {
        todo!()
    }

    /// The alpha mode in use, for stats; `AlphaMode::Opaque` means blur is turned off.
    pub fn alpha_mode(&self) -> AlphaMode {
        todo!()
    }
}
