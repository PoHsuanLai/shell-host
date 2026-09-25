//! The CPU presenter: `anyrender_vello_cpu` into `wl_shm` buffers. The fallback when no adapter
//! survives validation (`Backend::Cpu`, or `Backend::Gpu` with nothing usable). No
//! custom-widget textures on this path, as on the headless one.

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::BaseDocument;
use blitz_paint::paint_scene;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use wayland_client::protocol::wl_shm;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::error::{HostError, Result};
use crate::gpu::presenter::FrameTarget;
use crate::gpu::swapchain::pixel_extent;
use crate::scale::Size;

/// A surface's shm buffer pool.
#[derive(Debug)]
pub struct ShmBuffers {
    pub(crate) pool: SlotPool,
    pub(crate) buffer: Size,
    /// Where the frames go; boxed so the `Presenter::Shm` variant stays small.
    target: Option<Box<Attached>>,
}

/// The surface a pool presents on, and the buffer it last handed over.
#[derive(Debug)]
struct Attached {
    surface: WlSurface,
    /// Reused once the compositor releases it.
    current: Option<Buffer>,
}

impl ShmBuffers {
    /// A pool for `buffer`-sized ARGB8888 frames.
    pub fn new(shm: &Shm, buffer: Size) -> crate::error::Result<ShmBuffers> {
        let pool = SlotPool::new(frame_len(buffer).max(4), shm)
            .map_err(|e| HostError::Surface(format!("wl_shm pool: {e}")))?;
        Ok(ShmBuffers {
            pool,
            buffer,
            target: None,
        })
    }

    /// A pool whose frames are presented on `surface`. A pool from `new` alone cannot
    /// present: `Presenter::present` does not carry the `wl_surface`, so the pool holds it.
    pub fn for_surface(shm: &Shm, surface: WlSurface, buffer: Size) -> Result<ShmBuffers> {
        let mut buffers = ShmBuffers::new(shm, buffer)?;
        buffers.target = Some(Box::new(Attached {
            surface,
            current: None,
        }));
        Ok(buffers)
    }

    /// Paint the resolved `doc` on the CPU, copy it into a free buffer, attach, damage and
    /// commit. The caller has requested the frame callback, so this commit carries it.
    pub(crate) fn present(&mut self, doc: &mut BaseDocument, target: FrameTarget) -> Result<()> {
        let attached = self
            .target
            .as_mut()
            .ok_or_else(|| HostError::Surface("shm presenter has no wl_surface".into()))?;
        let rgba = paint_cpu(doc, target);
        let (width, height) = pixel_extent(target.buffer);
        let buffer = free_buffer(&mut self.pool, attached.current.take(), width, height)?;
        let canvas = self.pool.canvas(&buffer).ok_or_else(|| {
            HostError::Render("shm buffer is still held by the compositor".into())
        })?;
        swap_red_blue(&rgba, canvas);
        buffer
            .attach_to(&attached.surface)
            .map_err(|e| HostError::Render(format!("attach: {e}")))?;
        attached
            .surface
            .damage_buffer(0, 0, width as i32, height as i32);
        attached.surface.commit();
        attached.current = Some(buffer);
        self.buffer = target.buffer;
        Ok(())
    }
}

/// `last` if the compositor has released it and it has the right size, else a new buffer
/// (the old one is destroyed when released).
fn free_buffer(
    pool: &mut SlotPool,
    last: Option<Buffer>,
    width: u32,
    height: u32,
) -> Result<Buffer> {
    let stride = width as i32 * 4;
    if let Some(buffer) = last
        && buffer.height() == height as i32
        && buffer.stride() == stride
        && pool.canvas(&buffer).is_some()
    {
        return Ok(buffer);
    }
    pool.create_buffer(
        width as i32,
        height as i32,
        stride,
        wl_shm::Format::Argb8888,
    )
    .map(|(buffer, _)| buffer)
    .map_err(|e| HostError::Render(format!("shm buffer: {e}")))
}

fn frame_len(size: Size) -> usize {
    let (width, height) = pixel_extent(size);
    width as usize * height as usize * 4
}

/// Paint a resolved document on the CPU at `target.buffer`: premultiplied RGBA8.
pub(crate) fn paint_cpu(doc: &mut BaseDocument, target: FrameTarget) -> Vec<u8> {
    let (width, height) = pixel_extent(target.buffer);
    let (paint_width, paint_height, x, y) = target.paint_area();
    let scale = target.scale.as_f64();
    render_to_buffer::<VelloCpuImageRenderer, _>(
        |painter| paint_scene(painter, doc, scale, paint_width, paint_height, x, y),
        width,
        height,
    )
}

/// Swap the first and third byte of every 4-byte pixel from `src` into `dst`: RGBA8 to the
/// byte order of `wl_shm` ARGB8888 on a little-endian machine (B, G, R, A), and the GPU's
/// BGRA8 back to RGBA8. Both sides are premultiplied (vello_cpu renders premultiplied, and
/// Wayland's ARGB8888 is premultiplied), so alpha and the colour values pass through. Stops at
/// the shorter of the two.
pub fn swap_red_blue(src: &[u8], dst: &mut [u8]) {
    for (s, d) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<4>().0)
    {
        *d = [s[2], s[1], s[0], s[3]];
    }
}
