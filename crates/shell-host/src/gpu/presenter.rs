//! How a surface's pixels reach the compositor: a wgpu swapchain on the shared device, or
//! `wl_shm` buffers painted on the CPU. A closed pair, so an enum rather than a trait.

use blitz_traits::shell::ColorScheme;

use crate::dom::document::SurfaceDocument;
use crate::error::{HostError, Result};
use crate::gpu::context::GpuContext;
#[cfg(feature = "debug-inject")]
use crate::gpu::render::capture_document;
use crate::gpu::render::render_into;
use crate::gpu::shm::ShmBuffers;
#[cfg(feature = "debug-inject")]
use crate::gpu::shm::paint_cpu;
#[cfg(feature = "debug-inject")]
use crate::gpu::swapchain::pixel_extent;
use crate::gpu::swapchain::{Swapchain, alpha_mode_of, present_mode_of};
use crate::region::Rect;
use crate::scale::{Scale120, Size};
use crate::scroll::indicator::ThumbPaint;
use crate::stats::{AlphaMode, PresentMode};

/// What the host paints over the document: the scroll thumbs (design/11 §11.3.12).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Overlay {
    pub thumbs: Vec<ThumbPaint>,
    pub scheme: ColorScheme,
}

/// A surface's presenter.
#[derive(Debug)]
pub(crate) enum Presenter {
    /// Boxed: a swapchain (wgpu surface, configuration, scene) is far larger than a shm pool.
    Wgpu(Box<Swapchain>),
    Shm(ShmBuffers),
}

/// What one render needs to know about the surface: its buffer size, its scale, and the
/// document time to resolve at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameTarget {
    /// Physical pixels, `scale::buffer_size(logical, scale)`.
    pub buffer: Size,
    /// Where in the buffer the document paints, physical pixels: the whole buffer, or less a
    /// client-decorated toplevel's resize band (`csd::content_px`).
    pub content: Rect,
    pub scale: Scale120,
    /// Seconds on the host clock.
    pub time: f64,
}

impl FrameTarget {
    /// A target whose document covers the whole buffer.
    #[cfg(test)]
    pub(crate) fn whole(buffer: Size, scale: Scale120, time: f64) -> FrameTarget {
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
    pub(crate) fn paint_area(&self) -> (u32, u32, u32, u32) {
        let px = |v: i32| u32::try_from(v).unwrap_or(0);
        let c = self.content;
        (px(c.width), px(c.height), px(c.x), px(c.y))
    }
}

impl Presenter {
    /// (Re)configure for a new buffer size.
    pub(crate) fn resize(&mut self, gpu: Option<&GpuContext>, buffer: Size) {
        match self {
            // Without the device the size is picked up by the next present's acquire.
            Presenter::Wgpu(swapchain) => {
                if let Some(gpu) = gpu
                    && let Err(error) = swapchain.configure(gpu, buffer)
                {
                    eprintln!("shell-host: {error}");
                }
            }
            Presenter::Shm(buffers) => buffers.buffer = buffer,
        }
    }

    /// Paint, render and present `doc`, which the caller has just resolved (with its pre-paint
    /// hook, `SurfaceDocument::resolve`): resolving again here would re-round the layout the
    /// hook snapped. The caller has also requested the frame callback (`wl_surface.frame`), so
    /// the present's commit carries it. Never waits on the device. Says whether an image was
    /// left out for want of atlas room, so the caller paints again.
    pub(crate) fn present(
        &mut self,
        gpu: Option<&mut GpuContext>,
        doc: &mut SurfaceDocument,
        target: FrameTarget,
        overlay: &Overlay,
    ) -> Result<bool> {
        match self {
            Presenter::Wgpu(swapchain) => {
                present_wgpu(swapchain, need_gpu(gpu)?, doc, target, overlay)
            }
            Presenter::Shm(buffers) => {
                let mut base = doc.dioxus().inner.borrow_mut();
                buffers.present(&mut base, target, overlay)?;
                Ok(false)
            }
        }
    }

    #[cfg(feature = "debug-inject")]
    /// Resolve `doc` (running its pre-paint hook), render it offscreen at `target.buffer` and
    /// return RGBA8 pixels (debug `capture`).
    pub(crate) fn capture(
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
                let pixels = paint_cpu(&mut base, target, &Overlay::default());
                image::RgbaImage::from_raw(width, height, pixels)
                    .ok_or_else(|| HostError::Render("CPU capture size mismatch".into()))
            }
        }
    }

    /// The present mode in use, for stats.
    pub(crate) fn present_mode(&self) -> PresentMode {
        match self {
            Presenter::Wgpu(swapchain) => swapchain
                .config
                .as_ref()
                .map_or(PresentMode::Fifo, |c| present_mode_of(c.present_mode)),
            Presenter::Shm(_) => PresentMode::Shm,
        }
    }

    /// The alpha mode in use, for stats; `AlphaMode::Opaque` means blur is turned off.
    pub(crate) fn alpha_mode(&self) -> AlphaMode {
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
/// the commit.
fn present_wgpu(
    swapchain: &mut Swapchain,
    gpu: &mut GpuContext,
    doc: &mut SurfaceDocument,
    target: FrameTarget,
    overlay: &Overlay,
) -> Result<bool> {
    let frame = swapchain.acquire(gpu, target.buffer)?;
    let view = frame.texture.create_view(&Default::default());
    let mut encoder = gpu
        .device
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shell-host frame"),
        });
    let painted = render_into(
        gpu,
        doc,
        &mut swapchain.scene,
        target,
        overlay,
        &mut encoder,
        &view,
    )?;
    gpu.device.queue.submit([encoder.finish()]);
    frame.present();
    Ok(painted)
}
