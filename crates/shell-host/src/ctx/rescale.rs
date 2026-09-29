//! Following each surface's preferred fractional scale (`wp_fractional_scale_v1`).
//!
//! The handler only records the latest `preferred_scale` per `wl_surface`; this moves it onto
//! the surface, which publishes it to the document (`use_surface_scale`). It runs at the start
//! of every flush, before the woken documents are polled, so a scale that arrived with (or
//! before) a configure is read by the Dioxus tree in the same flush that renders the frame: the
//! first frame at a new scale is drawn by a tree that already knows it. The integer fallback
//! (`CompositorHandler::scale_factor_changed`) rescales in its handler, which is earlier still.
//!
//! See FINDINGS F43.

use crate::id::SurfaceId;
use crate::state::HostState;

/// Follow every surface's preferred scale. Costs a map lookup per surface and wakes nothing
/// unless a scale changed, so an idle host stays idle.
pub(super) fn follow_all(state: &mut HostState) {
    let ids: Vec<SurfaceId> = state.surfaces.keys().copied().collect();
    for id in ids {
        follow_preferred(state, id);
    }
}

/// Take surface `id`'s last reported preferred scale, if it differs from the one it has.
pub(super) fn follow_preferred(state: &mut HostState, id: SurfaceId) {
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    let preferred = state
        .fractional
        .as_ref()
        .and_then(|fractional| fractional.preferred_scale(surface.role.wl_surface()));
    if let Some(scale) = preferred
        && scale != surface.scale
    {
        surface.rescale(scale);
    }
}
