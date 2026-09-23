//! Painting a document through the shared renderer: `resolve(t)`, `paint_scene` into the
//! surface's `vello_hybrid::Scene` via a `VelloHybridScenePainter` borrowing the device's image
//! cache and widget textures, then `Renderer::render` into the swapchain texture. The frame
//! callback is requested before this, and `SurfaceTexture::present` after (SPIKE.md S1
//! finding 3).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use crate::dom::document::SurfaceDocument;
use crate::error::Result;
use crate::gpu::context::GpuContext;
use crate::gpu::presenter::FrameTarget;

/// Resolve and paint `doc` into `scene`, then render the scene into `view` with `encoder`.
pub fn render_into(
    gpu: &mut GpuContext,
    doc: &mut SurfaceDocument,
    scene: &mut vello_hybrid::Scene,
    target: FrameTarget,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> Result<()> {
    todo!()
}
