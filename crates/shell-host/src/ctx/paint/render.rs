//! Rendering one surface and presenting it, with the popup refit and the scroll wake it raises.

use std::time::Instant;

use crate::error::{Recovery, Result};
use crate::event::HostEvent;
use crate::frame::FrameInput;
use crate::gpu::presenter::{FrameTarget, Overlay};
use crate::id::SurfaceId;
use crate::placement::PopupPlacement;
use crate::popup::refit;
use crate::role::Role;
use crate::scroll::indicator::Wake;
use crate::state::HostState;
use crate::stats::AlphaMode;
use crate::surface::HostSurface;
use crate::timers::TimerRequest;

use super::regions::send_regions;

/// Render and present surface `id`, whose frame machine just said `Render`.
pub(super) fn render(state: &mut HostState, id: SurfaceId) -> Result<()> {
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
        if resolved.prepainted {
            surface.stats.prepainted();
        }
        (pending_refit(surface), resolved.hover_moved)
    };
    if let Some(placement) = refit {
        crate::ctx::ops::refit_popup(state, id, placement);
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
        scheme: surface.document.scheme(),
    };
    let Some(presenter) = surface.presenter.as_mut() else {
        return Ok(());
    };
    let presented = presenter.present(state.gpu.as_mut(), &mut surface.document, target, &overlay);
    let input = match &presented {
        Ok(_) => {
            let now = Instant::now();
            surface.stats.presented(now - started, now);
            // Resolve may have asked for a redraw itself; continuation is decided on the
            // callback by is_animating, or the machine never idles. A load that
            // completed on another thread meanwhile is not dropped: the next flush takes it
            // and dirties the frame in flight.
            surface.shell.drop_echo();
            FrameInput::Presented {
                animating: surface.pacing(now_host),
            }
        }
        Err(_) => {
            surface.stats.skipped();
            FrameInput::PresentFailed
        }
    };
    let first_frame = presented.is_ok();
    if first_frame && let Some(lock) = state.lock.as_mut() {
        lock.presented(id);
    }
    state.frame_input(id, input);
    if first_frame {
        announce_mapped(state, id);
    }
    if let Some(surface) = state.surfaces.get(&id)
        && let Wake::At(at) = surface.scroll.wake(now_host)
    {
        let after = at.0.saturating_sub(now_host.0);
        state.timers.push(TimerRequest::ScrollWake { id, after });
    }
    if hover {
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
        Ok(true) => {
            // The atlas was full: the next frame starts from an empty cache and draws them.
            state.frame_input(id, FrameInput::Invalidate);
            Ok(())
        }
        Ok(false) => Ok(()),
    }
}

/// Tell the shell surface `id` is on screen, once per map.
fn announce_mapped(state: &mut HostState, id: SurfaceId) {
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    if !surface.mapped {
        surface.mapped = true;
        state.events.push_back(HostEvent::SurfaceMapped { id });
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
fn blur_allowed(state: &HostState, id: SurfaceId) -> bool {
    let opaque = state
        .surfaces
        .get(&id)
        .and_then(|surface| surface.presenter.as_ref())
        .is_some_and(|presenter| presenter.alpha_mode() == AlphaMode::Opaque);
    state.blur_supported() && !opaque
}
