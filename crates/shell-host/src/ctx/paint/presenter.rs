//! Creating a surface's presenter: a swapchain on the one GPU device, or shm buffers.

use wayland_client::protocol::wl_surface::WlSurface;

use crate::config::Backend;
use crate::ctx::HostCtx;
use crate::error::{HostError, Recovery, Result};
use crate::gpu::context::GpuContext;
use crate::gpu::presenter::Presenter;
use crate::gpu::raw_handle::create_surface;
use crate::gpu::shm::ShmBuffers;
use crate::gpu::swapchain::Swapchain;
use crate::handlers::render_device::RenderDevice;
use crate::id::SurfaceId;
use crate::render_gpu::follow_compositor;
use crate::scale::Size;
use crate::state::HostState;

/// Create surface `id`'s presenter if it has none: a swapchain on the one device (created and
/// validated here on the first surface), or shm buffers on the CPU backend. Called eagerly at
/// spawn and when a hidden surface is shown (the configure handlers only resize an existing
/// presenter), and again at every flush as a safety net.
pub(in crate::ctx) fn ensure_presenter<M: 'static>(
    ctx: &mut HostCtx<M>,
    id: SurfaceId,
) -> Result<()> {
    let Some(surface) = ctx.state.surfaces.get(&id) else {
        return Ok(());
    };
    if surface.presenter.is_some() {
        return Ok(());
    }
    let buffer = surface.buffer();
    let presenter = match ctx.backend {
        Backend::Gpu => match gpu_presenter(ctx, id) {
            Ok(presenter) => presenter,
            Err(error) if error.recovery() == Recovery::Fallback => {
                eprintln!("shell-host: no usable GPU ({error}); using the CPU backend");
                ctx.backend = Backend::Cpu;
                shm_presenter(&ctx.state, surface_wl(&ctx.state, id)?, buffer)?
            }
            Err(error) => return Err(error),
        },
        Backend::Cpu => shm_presenter(&ctx.state, surface_wl(&ctx.state, id)?, buffer)?,
    };
    if let Some(surface) = ctx.state.surfaces.get_mut(&id) {
        surface.presenter = Some(presenter);
        // Forces `sync_geometry` to size the new presenter and repaint.
        surface.geometry = None;
    }
    Ok(())
}

fn gpu_presenter<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) -> Result<Presenter> {
    let state = &mut ctx.state;
    let conn = &state.conn;
    let instance = ctx
        .instance
        .get_or_insert_with(|| GpuContext::instance(conn));
    let Some(surface) = state.surfaces.get(&id) else {
        return Err(HostError::NoSurface(id));
    };
    let wgpu_surface = create_surface(instance, conn, surface.role.wl_surface())?;
    if state.gpu.is_none() {
        let compositor = state.render_device.as_ref().and_then(RenderDevice::ids);
        let pref = follow_compositor(&ctx.adapter, compositor);
        state.gpu = Some(GpuContext::new(instance, &wgpu_surface, &pref)?);
    }
    Ok(Presenter::Wgpu(Box::new(Swapchain::new(wgpu_surface))))
}

fn surface_wl(state: &HostState, id: SurfaceId) -> Result<WlSurface> {
    state
        .surfaces
        .get(&id)
        .map(|surface| surface.role.wl_surface().clone())
        .ok_or(HostError::NoSurface(id))
}

/// shm buffers that attach to and commit `wl` themselves (`ShmBuffers::for_surface`).
fn shm_presenter(state: &HostState, wl: WlSurface, buffer: Size) -> Result<Presenter> {
    // Before its first configure a surface may have no size yet; the configure resizes.
    let buffer = Size {
        width: buffer.width.max(1),
        height: buffer.height.max(1),
    };
    ShmBuffers::for_surface(&state.shm, wl, buffer).map(Presenter::Shm)
}
