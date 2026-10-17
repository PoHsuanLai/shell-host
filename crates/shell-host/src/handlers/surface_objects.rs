//! Removing a surface from the host: the protocol objects made for its `wl_surface` (the
//! fractional-scale and viewport objects, the blur object) are destroyed with it, and the maps
//! that find them forget it. Every removal of a surface goes through here, so a popup opened
//! every few seconds for weeks leaves nothing behind in the shell or in the compositor. A surface
//! that is closed for good also gives back its widgets' textures.

use crate::id::SurfaceId;
use crate::state::HostState;
use crate::surface::HostSurface;

impl HostState {
    /// Take surface `id` out of the host, after destroying the objects made for its current
    /// `wl_surface`. The surface (and with it the `wl_surface`) is dropped by the caller.
    pub(crate) fn remove_surface(&mut self, id: SurfaceId) -> Option<HostSurface> {
        let wl = self.surfaces.get(&id)?.role.wl_surface().clone();
        if let Some(fractional) = self.fractional.as_mut() {
            fractional.forget(&wl);
        }
        if let Some(effect) = self.background_effect.as_mut() {
            effect.forget(&wl);
        }
        self.surfaces.remove(&id)
    }

    /// Let go of the GPU textures the widgets of `surface`'s document registered, for a surface
    /// that is going away for good (not one that is only unmapped and kept). They live in the
    /// one shared device's map and would otherwise stay there until the host ends.
    pub(crate) fn release_textures(&mut self, surface: &mut HostSurface) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        gpu.widgets.forget_document(&mut surface.document.base_mut());
    }
}
