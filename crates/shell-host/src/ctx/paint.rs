//! Phase 3 of the flush: presenters, geometry, regions and the render itself.
//!
//! Per surface, in order: create the presenter on first use (and the one GPU device on the
//! first surface ever, validated by its first present, falling back to shm when no adapter
//! survives); follow a new size or scale (viewport destination, document viewport, presenter
//! size); take the document's redraw request; and if the frame machine says render: resolve
//! (once, with the pre-paint hook), send any changed input and blur region, request the frame callback, present (whose commit
//! carries all of it), clear the redraw flag, and feed `Presented` back.

use std::time::Instant;

use blitz_traits::shell::{ColorScheme, Viewport};
use smithay_client_toolkit::compositor::{CompositorState, Region};
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::blur::BlurSpec;
use crate::config::Backend;
use crate::csd::{Inset, band, content_px, to_surface};
use crate::ctx::HostCtx;
use crate::dom::document::HoverShift;
use crate::dom::shell_provider::{ImeRequest, RedrawRequest};
use crate::error::{HostError, Recovery, Result};
use crate::event::CloseReason;
use crate::frame::{FrameInput, RenderCmd};
use crate::gpu::context::GpuContext;
use crate::gpu::presenter::{FrameTarget, Overlay, Presenter};
use crate::gpu::raw_handle::create_surface;
use crate::gpu::shm::ShmBuffers;
use crate::gpu::swapchain::Swapchain;
use crate::handlers::background_effect::{BackgroundEffect, Support};
use crate::handlers::text_input::apply_ime_request;
use crate::id::SurfaceId;
use crate::ime::ImeFocus;
use crate::msg::Visibility;
use crate::placement::PopupPlacement;
use crate::popup::refit;
use crate::prepaint::Prepainted;
use crate::region::Rect;
use crate::resolve::{Change, Resolved, resolve};
use crate::role::Role;
use crate::scale::{Scale, Size, buffer_size};
use crate::scroll::indicator::Wake;
use crate::state::HostState;
use crate::stats::AlphaMode;
use crate::surface::{Configured, Geometry, HostSurface};
use crate::timers::TimerRequest;

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

/// shm buffers that attach to and commit `wl` themselves (`ShmBuffers::for_surface`).
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
    super::rescale::follow_preferred(state, id);
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    let wl = surface.role.wl_surface().clone();
    let now = Geometry {
        logical: surface.logical,
        inset: surface.inset,
        scale: surface.scale,
    };
    if surface.geometry == Some(now) {
        return;
    }
    match &state.fractional {
        Some(fractional) => fractional.set_destination(&wl, now.logical.width, now.logical.height),
        None => wl.set_buffer_scale(integer_scale(now.scale)),
    }
    surface.document.set_viewport(viewport(
        now.logical,
        now.inset,
        now.scale,
        state.color_scheme,
    ));
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
    let now_host = state.clock.instant_at(started);
    let blur = blur_allowed(state, id);

    let (refit, hover) = {
        let Some(surface) = state.surfaces.get_mut(&id) else {
            return Ok(());
        };
        // The scroll engine first: this frame's raw offsets are written before the resolve,
        // which hoists positioned descendants with the offsets current then.
        {
            let mut doc = surface.document.base_mut();
            surface.scroll.frame(&mut doc, now_host);
        }
        // Layout first, so element-following regions land in this frame's commit, and a
        // `FitContent` popup can be measured before anything else this frame reads its size.
        // The only resolve of the frame: it runs the pre-paint hook, and the present paints
        // what it left, so regions, popup fits and pixels all read the same (snapped) layout.
        let resolved = surface.document.resolve(time);
        surface.scroll.publish(&surface.document.base());
        if resolved.prepainted == Prepainted::Ran {
            surface.stats.prepainted();
        }
        (pending_refit(surface), resolved.hover)
    };
    if let Some(placement) = refit {
        super::ops::refit_popup(state, id, placement);
    }

    let Some(surface) = state.surfaces.get_mut(&id) else {
        return Ok(());
    };
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
        content: surface.content(),
        scale: surface.scale,
        time,
    };
    let overlay = Overlay {
        thumbs: surface.scroll.overlay(&surface.document.base(), now_host),
        scheme: state.color_scheme,
    };
    let Some(presenter) = surface.presenter.as_mut() else {
        return Ok(());
    };
    let presented = presenter.present(state.gpu.as_mut(), &mut surface.document, target, &overlay);
    let input = match &presented {
        Ok(()) => {
            let now = Instant::now();
            surface.stats.presented(now - started, now);
            // Resolve may have asked for a redraw itself; continuation is decided on the
            // callback by is_animating, or the machine never idles. A load that
            // completed on another thread meanwhile is not dropped: the next flush takes it
            // and dirties the frame in flight.
            surface.shell.drop_echo();
            FrameInput::Presented(surface.pacing(now_host))
        }
        Err(_) => {
            surface.stats.skipped();
            FrameInput::PresentFailed
        }
    };
    state.frame_input(id, input);
    if let Some(surface) = state.surfaces.get(&id)
        && let Wake::At(at) = surface.scroll.wake(now_host)
    {
        let after = at.0.saturating_sub(now_host.0);
        state.timers.push(TimerRequest::ScrollWake { id, after });
    }
    if hover == HoverShift::Moved {
        // The resolve moved the hover (Blitz's `refresh_hover`, and the repair after it):
        // the `:hover` restyle it asked for is the next frame's, and its redraw
        // request was just dropped as this render's echo.
        state.frame_input(id, FrameInput::Invalidate);
    }
    match presented {
        Err(error) if error.recovery() == Recovery::Fatal => Err(error),
        Err(error) => {
            eprintln!("shell-host: present {id:?}: {error}");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

/// Whether `surface`'s just-laid-out content calls for a new placement (`popup::refit`);
/// `None` for a non-popup, a `Fixed`-size popup, or one whose document has not laid out yet.
fn pending_refit(surface: &HostSurface) -> Option<PopupPlacement> {
    let Role::Popup {
        config, placement, ..
    } = &surface.role
    else {
        return None;
    };
    let content = surface.document.content_size()?;
    refit(config, *placement, content)
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
/// changed since they were last sent. Both are the document's, moved to surface coordinates;
/// the input region adds the resize band (`csd::band`), the blur region never covers it.
fn send_regions(
    compositor: &CompositorState,
    effect: Option<&mut BackgroundEffect>,
    qh: &QueueHandle<HostState>,
    surface: &mut HostSurface,
    blur: Support,
) {
    let size = surface.document_size();
    let inset = surface.inset;
    let document = &surface.document;
    let input = in_surface(
        resolve(&surface.input, size, |id| document.element_rect(id)),
        inset,
        band(surface.logical, inset),
    );
    let blur = match (&surface.blur, blur) {
        (BlurSpec::Region(spec), Support::Supported) => in_surface(
            resolve(spec, size, |id| document.element_rect(id)),
            inset,
            Vec::new(),
        ),
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

/// A document-coordinate region in surface coordinates, plus `extra` surface strips. A
/// pending region stays pending: the surface keeps the one it has.
fn in_surface(resolved: Resolved, inset: Inset, extra: Vec<Rect>) -> Resolved {
    match resolved {
        Resolved::Strips(rects) => {
            let mut strips = to_surface(rects, inset);
            strips.extend(extra);
            Resolved::Strips(strips)
        }
        Resolved::Pending => Resolved::Pending,
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
#[cfg(feature = "debug-inject")]
pub(crate) fn capture(state: &mut HostState, id: SurfaceId) -> Result<image::RgbaImage> {
    let time = state.clock.seconds_at(Instant::now());
    let surface = state
        .surfaces
        .get_mut(&id)
        .ok_or(HostError::NoSurface(id))?;
    let target = FrameTarget {
        buffer: surface.buffer(),
        content: surface.content(),
        scale: surface.scale,
        time,
    };
    let presenter = surface.presenter.as_mut().ok_or(HostError::Render(
        "the surface has not been presented yet".into(),
    ))?;
    presenter.capture(state.gpu.as_mut(), &mut surface.document, target)
}

/// The document viewport for a surface of `logical` size with resize band `inset` at `scale`:
/// the physical pixels the document paints in (`csd::content_px`, the whole buffer without a
/// band), the scale factor, the colour scheme. At least 1x1, so an unconfigured surface still
/// lays out.
pub(super) fn viewport(logical: Size, inset: Inset, scale: Scale, scheme: ColorScheme) -> Viewport {
    let content = content_px(buffer_size(logical, scale), inset, scale);
    let px = |v: i32| u32::try_from(v).unwrap_or(0).max(1);
    // Blitz takes the scale as f32; 120ths are exact in it.
    let factor = scale.as_f64() as f32;
    Viewport::new(px(content.width), px(content.height), factor, scheme)
}

/// `wl_surface.set_buffer_scale` for a compositor without fractional-scale: whole numbers only.
fn integer_scale(scale: Scale) -> i32 {
    i32::try_from((scale.0 / 120).max(1)).unwrap_or(1)
}

fn has_area(size: Size) -> bool {
    size.width > 0 && size.height > 0
}
