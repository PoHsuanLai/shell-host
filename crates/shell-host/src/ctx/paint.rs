//! Phase 3 of the flush: presenters, geometry, regions and the render itself.
//!
//! Per surface, in order: create the presenter on first use (and the one GPU device on the
//! first surface ever, validated by its first present, falling back to shm when no adapter
//! survives); follow a new size or scale (viewport destination, document viewport, presenter
//! size); take the document's redraw request; and if the frame machine says render: resolve,
//! send any changed input and blur region, request the frame callback, present (whose commit
//! carries all of it), clear the redraw flag, and feed `Presented` back (SPIKE.md S1 findings
//! 3 and 4).

use std::time::Instant;

use blitz_traits::shell::{ColorScheme, Viewport};
use smithay_client_toolkit::compositor::{CompositorState, Region};
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::blur::BlurSpec;
use crate::config::Backend;
use crate::ctx::HostCtx;
use crate::debug::AlphaMode;
use crate::dom::shell_provider::{ImeRequest, RedrawRequest};
use crate::error::{HostError, Recovery, Result};
use crate::event::CloseReason;
use crate::frame::{FrameInput, RenderCmd};
use crate::gpu::context::GpuContext;
use crate::gpu::presenter::{FrameTarget, Presenter};
use crate::gpu::raw_handle::create_surface;
use crate::gpu::shm::ShmBuffers;
use crate::gpu::swapchain::Swapchain;
use crate::handlers::background_effect::{BackgroundEffect, Support};
use crate::handlers::text_input::apply_ime_request;
use crate::id::SurfaceId;
use crate::ime::ImeFocus;
use crate::msg::Visibility;
use crate::region::Rect;
use crate::resolve::{Change, Resolved, resolve};
use crate::scale::{Scale, Size, buffer_size};
use crate::state::HostState;
use crate::surface::{Configured, Geometry, HostSurface};

/// Step every surface's frame machine; a surface that fails is closed with `Failed`.
pub(super) fn step_all<M: 'static>(ctx: &mut HostCtx<M>) {
    let ids: Vec<SurfaceId> = ctx.state.surfaces.keys().copied().collect();
    for id in ids {
        if let Err(error) = step_one(ctx, id) {
            eprintln!("shell-host: surface {id:?} failed: {error}");
            super::ops::close(ctx, id, CloseReason::Failed);
        }
    }
}

fn step_one<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) -> Result<()> {
    let Some(surface) = ctx.state.surfaces.get(&id) else {
        return Ok(());
    };
    let drawable = surface.visibility == Visibility::Shown
        && surface.configured == Configured::Done
        && has_area(surface.logical);
    if !drawable {
        return Ok(());
    }
    ensure_presenter(ctx, id)?;
    let state = &mut ctx.state;
    sync_ime(state, id);
    sync_geometry(state, id);
    if let Some(surface) = state.surfaces.get(&id)
        && surface.shell.take_redraw() == RedrawRequest::Requested
    {
        state.frame_input(id, FrameInput::Invalidate);
    }
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return Ok(());
    };
    match surface.step(FrameInput::Flush).render {
        RenderCmd::Render => render(state, id),
        RenderCmd::Skip => Ok(()),
    }
}

/// Create surface `id`'s presenter if it has none: a swapchain on the one device (created and
/// validated here on the first surface), or shm buffers on the CPU backend. Called eagerly at
/// spawn and when a hidden surface is shown (the configure handlers only resize an existing
/// presenter), and again at every flush as a safety net.
pub(super) fn ensure_presenter<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) -> Result<()> {
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
        state.gpu = Some(GpuContext::new(instance, &wgpu_surface, &ctx.adapter)?);
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

/// shm buffers that attach to and commit `wl` themselves (`ShmBuffers::for_surface`, H2 gpu).
fn shm_presenter(state: &HostState, wl: WlSurface, buffer: Size) -> Result<Presenter> {
    // Before its first configure a surface may have no size yet; the configure resizes.
    let buffer = Size {
        width: buffer.width.max(1),
        height: buffer.height.max(1),
    };
    ShmBuffers::for_surface(&state.shm, wl, buffer).map(Presenter::Shm)
}

/// Send the IME state the focused surface's document last asked for, when it differs from what
/// the text input was last told (`handlers::text_input::apply_ime_request` commits every call).
fn sync_ime(state: &mut HostState, id: SurfaceId) {
    if state.input.keyboard_focus != Some(id) {
        return;
    }
    let Some(wanted) = state.surfaces.get(&id).map(|surface| surface.shell.ime()) else {
        return;
    };
    let current = match state.ime.focus {
        ImeFocus::Disabled => ImeRequest::Disabled,
        ImeFocus::Enabled { cursor_area } => ImeRequest::Enabled { cursor_area },
    };
    if wanted != current {
        apply_ime_request(state, wanted);
    }
}

/// Follow a new preferred scale or configured size: the viewport destination (or the integer
/// buffer scale without fractional-scale), the document's viewport, the presenter's size; then
/// repaint with every region re-sent.
fn sync_geometry(state: &mut HostState, id: SurfaceId) {
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    let wl = surface.role.wl_surface().clone();
    let preferred = state
        .fractional
        .as_ref()
        .and_then(|fractional| fractional.preferred_scale(&wl));
    if let Some(scale) = preferred
        && scale != surface.scale
    {
        surface.rescale(scale);
    }
    let now = Geometry {
        logical: surface.logical,
        scale: surface.scale,
    };
    if surface.geometry == Some(now) {
        return;
    }
    match &state.fractional {
        Some(fractional) => fractional.set_destination(&wl, now.logical.width, now.logical.height),
        None => wl.set_buffer_scale(integer_scale(now.scale)),
    }
    surface
        .document
        .set_viewport(viewport(now.logical, now.scale, state.color_scheme));
    let buffer = surface.buffer();
    if let Some(presenter) = surface.presenter.as_mut() {
        presenter.resize(state.gpu.as_ref(), buffer);
    }
    surface.geometry = Some(now);
    state.frame_input(id, FrameInput::Invalidate);
}

/// Render and present surface `id`, whose frame machine just said `Render`.
fn render(state: &mut HostState, id: SurfaceId) -> Result<()> {
    let started = Instant::now();
    let time = state.clock.seconds_at(started);
    let blur = blur_allowed(state, id);
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return Ok(());
    };
    // Layout first, so element-following regions land in this frame's commit.
    surface.document.resolve(time);
    send_regions(
        &state.compositor,
        state.background_effect.as_mut(),
        &state.qh,
        surface,
        blur,
    );
    let wl = surface.role.wl_surface().clone();
    wl.frame(&state.qh, wl.clone());
    let target = FrameTarget {
        buffer: surface.buffer(),
        scale: surface.scale,
        time,
    };
    let Some(presenter) = surface.presenter.as_mut() else {
        return Ok(());
    };
    let presented = presenter.present(state.gpu.as_mut(), &mut surface.document, target);
    let input = match &presented {
        Ok(()) => {
            let now = Instant::now();
            surface.stats.presented(now - started, now);
            // Resolve may have asked for a redraw itself; continuation is decided on the
            // callback by is_animating, or the machine never idles (S1 finding 4).
            surface.shell.take_redraw();
            FrameInput::Presented(surface.document.animation())
        }
        Err(_) => {
            surface.stats.skipped();
            FrameInput::PresentFailed
        }
    };
    state.frame_input(id, input);
    match presented {
        Err(error) if error.recovery() == Recovery::Fatal => Err(error),
        Err(error) => {
            eprintln!("shell-host: present {id:?}: {error}");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

/// Whether blur may be sent for surface `id`: the compositor blurs, and the surface has real
/// alpha (an opaque swapchain turns blur off, plan "GPU").
fn blur_allowed(state: &HostState, id: SurfaceId) -> Support {
    let opaque = state
        .surfaces
        .get(&id)
        .and_then(|surface| surface.presenter.as_ref())
        .is_some_and(|presenter| presenter.alpha_mode() == AlphaMode::Opaque);
    match state.blur_supported() {
        Support::Supported if !opaque => Support::Supported,
        Support::Supported | Support::Unsupported => Support::Unsupported,
    }
}

/// Resolve the input and blur regions against the laid-out document and send whichever
/// changed since they were last sent.
fn send_regions(
    compositor: &CompositorState,
    effect: Option<&mut BackgroundEffect>,
    qh: &QueueHandle<HostState>,
    surface: &mut HostSurface,
    blur: Support,
) {
    let size = surface.logical;
    let document = &surface.document;
    let input = resolve(&surface.input, size, |id| document.element_rect(id));
    let blur = match (&surface.blur, blur) {
        (BlurSpec::Region(spec), Support::Supported) => {
            resolve(spec, size, |id| document.element_rect(id))
        }
        (BlurSpec::None, _) | (BlurSpec::Region(_), Support::Unsupported) => {
            Resolved::Strips(Vec::new())
        }
    };
    let wl = surface.role.wl_surface();
    if surface.regions.input.update(&input) == Change::Changed
        && let Resolved::Strips(rects) = &input
    {
        let region = region_of(compositor, rects);
        wl.set_input_region(region.as_ref().map(Region::wl_region));
    }
    if surface.regions.blur.update(&blur) == Change::Changed
        && let Resolved::Strips(rects) = &blur
        && let Some(effect) = effect
    {
        let region = (!rects.is_empty())
            .then(|| region_of(compositor, rects))
            .flatten();
        effect.set_blur_region(qh, wl, region.as_ref().map(Region::wl_region));
    }
}

fn region_of(compositor: &CompositorState, rects: &[Rect]) -> Option<Region> {
    let region = Region::new(compositor).ok()?;
    for rect in rects {
        region.add(rect.x, rect.y, rect.width, rect.height);
    }
    Some(region)
}

/// Render surface `id` offscreen at its buffer size (debug `capture`).
pub(crate) fn capture(state: &mut HostState, id: SurfaceId) -> Result<image::RgbaImage> {
    let time = state.clock.seconds_at(Instant::now());
    let surface = state
        .surfaces
        .get_mut(&id)
        .ok_or(HostError::NoSurface(id))?;
    let target = FrameTarget {
        buffer: surface.buffer(),
        scale: surface.scale,
        time,
    };
    let presenter = surface.presenter.as_mut().ok_or(HostError::Render(
        "the surface has not been presented yet".into(),
    ))?;
    presenter.capture(state.gpu.as_mut(), &mut surface.document, target)
}

/// The document viewport for a surface of `logical` size at `scale`: physical pixels, the
/// scale factor, the colour scheme. At least 1x1, so an unconfigured surface still lays out.
pub(super) fn viewport(logical: Size, scale: Scale, scheme: ColorScheme) -> Viewport {
    let buffer = buffer_size(logical, scale);
    let px = |v: i32| u32::try_from(v).unwrap_or(0).max(1);
    // Blitz takes the scale as f32; 120ths are exact in it.
    let factor = scale.as_f64() as f32;
    Viewport::new(px(buffer.width), px(buffer.height), factor, scheme)
}

/// `wl_surface.set_buffer_scale` for a compositor without fractional-scale: whole numbers only.
fn integer_scale(scale: Scale) -> i32 {
    i32::try_from((scale.0 / 120).max(1)).unwrap_or(1)
}

fn has_area(size: Size) -> bool {
    size.width > 0 && size.height > 0
}
