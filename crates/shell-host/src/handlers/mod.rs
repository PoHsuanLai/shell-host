//! Wayland event handlers: sctk handler traits implemented on `HostState`, and hand-written
//! `Dispatch` implementations for the protocols sctk 0.20 does not wrap (background effect,
//! fractional scale + viewporter, text input). The delegate macros that route to them are all
//! in `state.rs`.

pub(crate) mod activation;
pub(crate) mod background_effect;
pub(crate) mod band;
pub(crate) mod compositor;
pub(crate) mod data_device;
pub(crate) mod devices;
pub(crate) mod drag_source;
pub(crate) mod fractional;
pub(crate) mod keyboard;
pub(crate) mod layer;
pub(crate) mod output;
pub(crate) mod pipe_writes;
pub(crate) mod pointer;
pub(crate) mod quire_window;
pub(crate) mod render_device;
pub(crate) mod seat;
pub(crate) mod session_lock;
pub(crate) mod surface_objects;
pub(crate) mod surrounding;
pub(crate) mod text_input;
pub(crate) mod xdg;

use wayland_client::protocol::wl_surface::WlSurface;

use crate::event::{CloseReason, HostEvent};
use crate::frame::TimerCmd;
use crate::id::SurfaceId;
use crate::scale::{BufferMapping, Scale120, Size, buffer_mapping};
use crate::state::HostState;
use crate::timers::TimerRequest;

/// The id of the surface whose role owns `wl_surface`, if any is still live. Every handler that
/// receives a bare `wl_surface::WlSurface` (there is no other way to name a surface at the
/// protocol level) starts by looking it up this way, then indexes `state.surfaces` directly —
/// returning a borrow here instead would tie up the whole of `state` for its lifetime, through
/// the function-call boundary, which is exactly the field access the rest of a handler still
/// needs (`state.timers`, `state.gpu`, `state.fractional`, ...).
pub(crate) fn surface_id_by_wl(state: &HostState, wl_surface: &WlSurface) -> Option<SurfaceId> {
    state
        .surfaces
        .iter()
        .find(|(_, s)| s.role.wl_surface() == wl_surface)
        .map(|(&id, _)| id)
}

/// Closes `id` and every popup (transitively) whose parent chain leads to it: cancels each
/// surface's watchdog, removes it from `state.surfaces`, then raises `HostEvent::SurfaceClosed`
/// — `reason` for `id` itself, [`CloseReason::ParentClosed`] for its descendants. Descendants
/// are removed (and their close events raised) before their ancestor, so no popup ever outlives
/// its parent and a shell never sees a popup close after the surface that owned it.
///
/// Shared by every handler that can make a surface go away: a layer surface's `closed`, a
/// toplevel's `request_close`, and a popup's own `done`.
pub(crate) fn close_surface_and_popups(state: &mut HostState, id: SurfaceId, reason: CloseReason) {
    let children: Vec<SurfaceId> = state
        .surfaces
        .iter()
        .filter_map(|(&child, s)| (s.role.parent() == Some(id)).then_some(child))
        .collect();
    for child in children {
        // A child may already be gone if an earlier sibling's cascade reached it first (not
        // possible today, since a popup has exactly one parent, but the lookup stays cheap and
        // this keeps the function correct if that ever changes).
        if state.surfaces.contains_key(&child) {
            close_surface_and_popups(state, child, CloseReason::ParentClosed);
        }
    }
    state.timers.push(TimerRequest::Watchdog {
        id,
        cmd: TimerCmd::Cancel,
    });
    // As for a close the host starts: the pointer's stay and the keyboard
    // focus end with the surface.
    state.withdraw_pointer(id);
    state.withdraw_keyboard(id);
    state.forget_quire_window(id);
    if let Some(mut surface) = state.remove_surface(id) {
        state.release_textures(&mut surface);
        state
            .events
            .push_back(HostEvent::SurfaceClosed { id, reason });
    }
}

/// Tie `wl_surface`'s buffer to its `logical` size at `scale`, as `buffer_mapping` decides:
/// through the viewport once the compositor prefers a fractional scale, else the buffer scale.
/// Nothing is sent for a size without an area: a viewport destination of 0 is a protocol error
/// that ends the connection, and such a surface has no buffer yet.
pub(crate) fn follow_buffer_scale(
    state: &HostState,
    wl_surface: &WlSurface,
    logical: Size,
    scale: Scale120,
) {
    if !logical.has_area() {
        return;
    }
    let fractional = state.fractional.as_ref();
    let preferred = fractional.and_then(|fractional| fractional.preferred_scale(wl_surface));
    match (buffer_mapping(preferred, logical, scale), fractional) {
        (BufferMapping::Viewport(size), Some(fractional)) => {
            fractional.set_destination(wl_surface, size);
        }
        (BufferMapping::Viewport(_), None) => {}
        (BufferMapping::Integer(factor), _) => wl_surface.set_buffer_scale(factor),
    }
}
