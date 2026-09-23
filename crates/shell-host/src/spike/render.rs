//! Painting a document through the shared renderer, and presenting it on a frame callback.

use std::time::Instant;

use anyrender_vello_hybrid::{ImageManager, VelloHybridScenePainter};
use blitz_dom::Document;
use blitz_paint::paint_scene;
use vello_hybrid::{RenderSize, TextureBindings};
use wayland_client::QueueHandle;

use super::gpu::Gpu;
use super::surface::{Pace, SpikeSurface};

/// Resolves the document at the surface clock and renders it into `view`.
pub fn paint(
    gpu: &mut Gpu,
    s: &mut SpikeSurface,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> Result<(), String> {
    let (w, h) = s.buffer;
    let scale = s.scale120 as f64 / 120.0;
    let t = s.clock.elapsed().as_secs_f64();
    let mut inner = s.doc.inner_mut();
    inner.resolve(t);
    s.scene.reset();
    {
        let mut images = Default::default();
        let mut bindings = Default::default();
        let image_manager = ImageManager::new(
            &mut gpu.renderer,
            &mut gpu.resources,
            &gpu.handle.device,
            &gpu.handle.queue,
            encoder,
            &mut images,
        );
        let mut painter =
            VelloHybridScenePainter::new(&mut s.scene, image_manager, &mut bindings, &gpu.handle);
        paint_scene(&mut painter, &mut inner, scale, w, h, 0, 0);
    }
    gpu.renderer
        .render(
            &s.scene,
            &mut gpu.resources,
            &gpu.handle.device,
            &gpu.handle.queue,
            encoder,
            &RenderSize {
                width: w,
                height: h,
            },
            view,
            &TextureBindings::new(),
        )
        .map_err(|e| format!("{e:?}"))
}

fn acquire(gpu: &Gpu, s: &mut SpikeSurface) -> Option<wgpu::SurfaceTexture> {
    use wgpu::CurrentSurfaceTexture as C;
    let surface = s.wgpu.as_ref()?;
    match surface.get_current_texture() {
        C::Success(t) | C::Suboptimal(t) => Some(t),
        C::Outdated | C::Lost => {
            s.configure_swapchain(gpu);
            match s.wgpu.as_ref()?.get_current_texture() {
                C::Success(t) | C::Suboptimal(t) => Some(t),
                other => {
                    eprintln!("[{}] acquire after reconfigure: {other:?}", s.name);
                    None
                }
            }
        }
        other => {
            eprintln!("[{}] acquire: {other:?}", s.name);
            None
        }
    }
}

/// Renders and presents one frame, requesting a frame callback first; the present commits.
pub fn present<D>(gpu: &mut Gpu, s: &mut SpikeSurface, qh: &QueueHandle<D>)
where
    D: wayland_client::Dispatch<
            wayland_client::protocol::wl_callback::WlCallback,
            wayland_client::protocol::wl_surface::WlSurface,
        > + 'static,
{
    let start = Instant::now();
    let Some(texture) = acquire(gpu, s) else {
        s.stats.skipped += 1;
        return;
    };
    let view = texture.texture.create_view(&Default::default());
    let mut encoder = gpu
        .handle
        .device
        .create_command_encoder(&Default::default());
    if let Err(e) = paint(gpu, s, &mut encoder, &view) {
        eprintln!("[{}] render failed: {e}", s.name);
        s.stats.skipped += 1;
        return;
    }
    gpu.handle.queue.submit([encoder.finish()]);
    let wl = s.wl_surface().clone();
    wl.frame(qh, wl.clone());
    texture.present();
    let now = Instant::now();
    s.stats.presented((now - start).as_secs_f64() * 1000.0, now);
    s.pace = Pace::InFlight;
}

/// Whether the document still wants frames (a transition or animation is running).
pub fn animating(s: &SpikeSurface) -> bool {
    s.doc.inner().is_animating()
}
