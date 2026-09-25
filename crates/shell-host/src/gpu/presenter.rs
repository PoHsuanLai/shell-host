//! How a surface's pixels reach the compositor: a wgpu swapchain on the shared device, or
//! `wl_shm` buffers painted on the CPU. A closed pair, so an enum rather than a trait.

use crate::debug::{AlphaMode, PresentMode};
use crate::dom::document::SurfaceDocument;
use crate::error::{HostError, Result};
use crate::gpu::context::GpuContext;
use crate::gpu::render::{capture_document, render_into};
use crate::gpu::shm::{ShmBuffers, paint_cpu};
use crate::gpu::swapchain::{Swapchain, alpha_mode_of, pixel_extent, present_mode_of};
use crate::region::Rect;
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
    /// Where in the buffer the document paints, physical pixels: the whole buffer, or less a
    /// client-decorated toplevel's resize band (`csd::content_px`).
    pub content: Rect,
    pub scale: Scale,
    /// Seconds on the host clock.
    pub time: f64,
}

impl FrameTarget {
    /// A target whose document covers the whole buffer.
    pub fn whole(buffer: Size, scale: Scale, time: f64) -> FrameTarget {
        FrameTarget {
            buffer,
            content: Rect {
                x: 0,
                y: 0,
                width: buffer.width,
                height: buffer.height,
            },
            scale,
            time,
        }
    }

    /// `paint_scene`'s viewport width and height and its x and y offset, physical pixels.
    pub fn paint_area(&self) -> (u32, u32, u32, u32) {
        let px = |v: i32| u32::try_from(v).unwrap_or(0);
        let c = self.content;
        (px(c.width), px(c.height), px(c.x), px(c.y))
    }
}

impl Presenter {
    /// (Re)configure for a new buffer size.
    pub fn resize(&mut self, gpu: Option<&GpuContext>, buffer: Size) {
        match self {
            // Without the device the size is picked up by the next present's acquire.
            Presenter::Wgpu(swapchain) => {
                if let Some(gpu) = gpu {
                    swapchain.configure(gpu, buffer);
                }
            }
            Presenter::Shm(buffers) => buffers.buffer = buffer,
        }
    }

    /// Paint, render and present `doc`, which the caller has just resolved (with its pre-paint
    /// hook, `SurfaceDocument::resolve`): resolving again here would re-round the layout the
    /// hook snapped. The caller has also requested the frame callback (`wl_surface.frame`), so
    /// the present's commit carries it (SPIKE.md S1 finding 3). Never waits on the device.
    pub fn present(
        &mut self,
        gpu: Option<&mut GpuContext>,
        doc: &mut SurfaceDocument,
        target: FrameTarget,
    ) -> Result<()> {
        match self {
            Presenter::Wgpu(swapchain) => present_wgpu(swapchain, need_gpu(gpu)?, doc, target),
            Presenter::Shm(buffers) => {
                let mut base = doc.dioxus().inner.borrow_mut();
                buffers.present(&mut base, target)
            }
        }
    }

    /// Resolve `doc` (running its pre-paint hook), render it offscreen at `target.buffer` and
    /// return RGBA8 pixels (debug `capture`).
    pub fn capture(
        &mut self,
        gpu: Option<&mut GpuContext>,
        doc: &mut SurfaceDocument,
        target: FrameTarget,
    ) -> Result<image::RgbaImage> {
        // A capture is its own render, not a copy of the last frame: it resolves like one.
        doc.resolve(target.time);
        let mut base = doc.dioxus().inner.borrow_mut();
        match self {
            Presenter::Wgpu(_) => capture_document(need_gpu(gpu)?, &mut base, target),
            Presenter::Shm(_) => {
                let (width, height) = pixel_extent(target.buffer);
                image::RgbaImage::from_raw(width, height, paint_cpu(&mut base, target))
                    .ok_or_else(|| HostError::Render("CPU capture size mismatch".into()))
            }
        }
    }

    /// The present mode in use, for stats.
    pub fn present_mode(&self) -> PresentMode {
        match self {
            Presenter::Wgpu(swapchain) => swapchain
                .config
                .as_ref()
                .map_or(PresentMode::Fifo, |c| present_mode_of(c.present_mode)),
            Presenter::Shm(_) => PresentMode::Shm,
        }
    }

    /// The alpha mode in use, for stats; `AlphaMode::Opaque` means blur is turned off.
    pub fn alpha_mode(&self) -> AlphaMode {
        match self {
            Presenter::Wgpu(swapchain) => swapchain
                .config
                .as_ref()
                .map_or(AlphaMode::Opaque, |c| alpha_mode_of(c.alpha_mode)),
            // wl_shm ARGB8888 is premultiplied.
            Presenter::Shm(_) => AlphaMode::PreMultiplied,
        }
    }
}

fn need_gpu(gpu: Option<&mut GpuContext>) -> Result<&mut GpuContext> {
    gpu.ok_or_else(|| HostError::Render("a wgpu presenter needs the GPU context".into()))
}

/// Acquire, render, submit, present: the frame request is already queued, and the present is
/// the commit (SPIKE.md S1 finding 3).
fn present_wgpu(
    swapchain: &mut Swapchain,
    gpu: &mut GpuContext,
    doc: &mut SurfaceDocument,
    target: FrameTarget,
) -> Result<()> {
    let frame = swapchain.acquire(gpu, target.buffer)?;
    let view = frame.texture.create_view(&Default::default());
    let mut encoder = gpu
        .device
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shell-host frame"),
        });
    render_into(gpu, doc, &mut swapchain.scene, target, &mut encoder, &view)?;
    gpu.device.queue.submit([encoder.finish()]);
    frame.present();
    Ok(())
}
