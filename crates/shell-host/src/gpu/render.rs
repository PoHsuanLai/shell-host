//! Painting a resolved document through the shared renderer: `paint_scene` into the
//! surface's `vello_hybrid::Scene` via a `VelloHybridScenePainter` borrowing the device's image
//! cache and widget textures, then `Renderer::render` into the swapchain texture. The frame
//! callback is requested before this, and `SurfaceTexture::present` after.
//!
//! See FINDINGS F75.

use anyrender_vello_hybrid::{ImageManager, VelloHybridScenePainter};
use blitz_dom::BaseDocument;
use blitz_paint::paint_scene;
use vello_hybrid::RenderSize;

use crate::dom::document::SurfaceDocument;
use crate::error::{HostError, Result};
use crate::gpu::context::GpuContext;
#[cfg(any(feature = "debug-inject", test))]
use crate::gpu::context::SURFACE_FORMAT;
use crate::gpu::image_cache;
use crate::gpu::presenter::{FrameTarget, Overlay};
#[cfg(any(feature = "debug-inject", test))]
use crate::gpu::shm::swap_red_blue;
use crate::gpu::swapchain::pixel_extent;

/// Paint the already resolved `doc` into `scene`, the host's overlay (the scroll thumbs) over
/// it, then render the scene into `view` with `encoder`. The caller resolved it (`SurfaceDocument::resolve`, which runs the pre-paint hook).
pub(crate) fn render_into(
    gpu: &mut GpuContext,
    doc: &mut SurfaceDocument,
    scene: &mut vello_hybrid::Scene,
    target: FrameTarget,
    overlay: &Overlay,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> Result<()> {
    let mut base = doc.dioxus().inner.borrow_mut();
    paint_document(gpu, &mut base, scene, (target, overlay), encoder, view)
}

/// Paint an already resolved document into `scene` and render it into `view`: [`render_into`]
/// on a bare `BaseDocument`. Widgets in the document see the shared device and register their
/// textures into it, where they stay for later frames.
pub(crate) fn paint_document(
    gpu: &mut GpuContext,
    doc: &mut BaseDocument,
    scene: &mut vello_hybrid::Scene,
    (target, overlay): (FrameTarget, &Overlay),
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> Result<()> {
    let (width, height) = pixel_extent(target.buffer);
    let (paint_width, paint_height, x, y) = target.paint_area();
    fit_scene(scene, width, height)?;
    let GpuContext {
        device,
        renderer,
        resources,
        images,
        widgets,
        ..
    } = gpu;
    image_cache::trim(images, renderer, resources, encoder);
    {
        let image_manager = ImageManager::new(
            &mut *renderer,
            &mut *resources,
            &device.device,
            &device.queue,
            &mut *encoder,
            images,
        );
        let mut painter =
            VelloHybridScenePainter::new(&mut *scene, image_manager, &mut widgets.textures, device);
        paint_scene(
            &mut painter,
            doc,
            target.scale.as_f64(),
            paint_width,
            paint_height,
            x,
            y,
        );
        crate::scroll::paint::paint(
            &mut painter,
            &overlay.thumbs,
            overlay.scheme,
            target.scale.as_f64(),
            (f64::from(x), f64::from(y)),
        );
    }
    renderer
        .render(
            scene,
            resources,
            &device.device,
            &device.queue,
            encoder,
            &RenderSize { width, height },
            view,
            &widgets.bindings(),
        )
        .map_err(|e| HostError::Render(format!("{e:?}")))
}

/// An empty scene of exactly the render size (vello_hybrid wants the two equal).
fn fit_scene(scene: &mut vello_hybrid::Scene, width: u32, height: u32) -> Result<()> {
    let w = u16::try_from(width).map_err(|_| too_large(width, height))?;
    let h = u16::try_from(height).map_err(|_| too_large(width, height))?;
    if scene.width() == w && scene.height() == h {
        scene.reset();
    } else {
        scene.reset_and_resize(w, h);
    }
    Ok(())
}

fn too_large(width: u32, height: u32) -> HostError {
    HostError::Render(format!(
        "{width}x{height} exceeds the renderer's 65535 px limit"
    ))
}

#[cfg(any(feature = "debug-inject", test))]
/// Render an already resolved document offscreen at `target.buffer` through the shared
/// renderer and read it back as RGBA8, premultiplied as rendered. Blocks on the device: for
/// debug captures and tests, never the frame path.
pub(crate) fn capture_document(
    gpu: &mut GpuContext,
    doc: &mut BaseDocument,
    target: FrameTarget,
) -> Result<image::RgbaImage> {
    let (width, height) = pixel_extent(target.buffer);
    let device = gpu.device.device.clone();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shell-host capture"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SURFACE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shell-host capture readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut scene = vello_hybrid::Scene::new(1, 1);
    paint_document(
        gpu,
        doc,
        &mut scene,
        (target, &Overlay::default()),
        &mut encoder,
        &view,
    )?;
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        extent(width, height),
    );
    gpu.device.queue.submit([encoder.finish()]);
    read_rows(&device, &readback, row, width, height)
}

#[cfg(any(feature = "debug-inject", test))]
fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

#[cfg(any(feature = "debug-inject", test))]
/// Map the readback buffer and turn its padded BGRA rows into a tight RGBA image.
fn read_rows(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    row: u32,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage> {
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| HostError::Render(e.to_string()))?;
    let tight = (width * 4) as usize;
    let mut rgba = vec![0u8; tight * height as usize];
    {
        let data = slice.get_mapped_range();
        for (src, dst) in data.chunks(row as usize).zip(rgba.chunks_mut(tight)) {
            swap_red_blue(&src[..tight], dst);
        }
    }
    readback.unmap();
    image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| HostError::Render("capture buffer size mismatch".into()))
}
