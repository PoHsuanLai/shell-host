//! Phase 3 of the flush: presenters, geometry, regions and the render itself.
//!
//! Per surface, in order: create the presenter on first use (and the one GPU device on the
//! first surface ever, validated by its first present, falling back to shm when no adapter
//! survives); follow a new size or scale (viewport destination, document viewport, presenter
//! size); take the document's redraw request; and if the frame machine says render: resolve
//! (once, with the pre-paint hook), send any changed input and blur region, request the frame callback, present (whose commit
//! carries all of it), clear the redraw flag, and feed `Presented` back.

#[cfg(feature = "debug-inject")]
#[cfg(feature = "debug-inject")]
mod capture;
mod geometry;
mod presenter;
mod recover;
mod regions;
mod render;

#[cfg(feature = "debug-inject")]
#[cfg(feature = "debug-inject")]
pub(crate) use capture::capture;
pub(super) use geometry::viewport;
pub(super) use presenter::ensure_presenter;

use crate::ctx::HostCtx;
use crate::error::Result;
use crate::event::CloseReason;
use crate::frame::FrameInput;
use crate::handlers::surrounding;
use crate::handlers::text_input::apply_ime_request;
use crate::id::SurfaceId;
use crate::msg::Visibility;
use crate::state::HostState;

use geometry::sync_geometry;
use render::render;

/// Step every surface's frame machine; a surface that fails is closed with `Failed`. A lost GPU
/// device is rebuilt first (`recover`).
pub(super) fn step_all<M: 'static>(ctx: &mut HostCtx<M>) {
    recover::recover_device(ctx);
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
    let drawable =
        surface.visibility == Visibility::Shown && surface.configured && surface.logical.has_area();
    if !drawable {
        return Ok(());
    }
    ensure_presenter(ctx, id)?;
    let state = &mut ctx.state;
    sync_ime(state, id);
    sync_geometry(state, id);
    if let Some(surface) = state.surfaces.get(&id)
        && surface.shell.take_redraw()
    {
        state.frame_input(id, FrameInput::Invalidate);
    }
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return Ok(());
    };
    if surface.step(FrameInput::Flush).render {
        render(state, id)
    } else {
        Ok(())
    }
}

/// Send the IME state the focused surface's document last asked for, when it differs from what
/// the text input was last told (`handlers::text_input::apply_ime_request` commits every call),
/// then the focused field's text around the caret when that changed (`handlers::surrounding`).
fn sync_ime(state: &mut HostState, id: SurfaceId) {
    if state.input.keyboard_focus != Some(id) {
        return;
    }
    let Some(wanted) = state.surfaces.get(&id).map(|surface| surface.shell.ime()) else {
        return;
    };
    if wanted != state.ime.focus {
        apply_ime_request(state, wanted);
    }
    surrounding::sync(state);
}
