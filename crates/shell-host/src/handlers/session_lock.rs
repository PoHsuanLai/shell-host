//! `ext-session-lock-v1` events: `locked` and `finished` become inputs of the lock in force
//! (applied by `ctx::lock::advance` in the flush that follows); a lock surface's configure is
//! recorded like a layer surface's. sctk has already sent `ack_configure` when `configure` runs.

use smithay_client_toolkit::session_lock::{
    SessionLock, SessionLockHandler, SessionLockSurface, SessionLockSurfaceConfigure,
};
use wayland_client::{Connection, QueueHandle};

use crate::handlers::layer::follow_configure;
use crate::handlers::surface_id_by_wl;
use crate::lock::machine::LockInput;
use crate::region::Px;
use crate::scale::Size;
use crate::state::HostState;

impl SessionLockHandler for HostState {
    fn locked(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        self.lock_input(LockInput::Locked);
    }

    fn finished(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        self.lock_input(LockInput::Finished);
    }

    /// The size is exact (the protocol's `dimensions_mismatch`): recorded as is.
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(id) = surface_id_by_wl(self, surface.wl_surface()) else {
            return;
        };
        let (width, height) = configure.new_size;
        let px = |v: u32| Px::try_from(v).unwrap_or(Px::MAX);
        let logical = Size {
            width: px(width),
            height: px(height),
        };
        let Some(host_surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        host_surface.configure(logical);
        let scale = host_surface.scale;
        follow_configure(self, id, surface.wl_surface(), logical, scale);
    }
}

impl HostState {
    /// Queue `input` for the lock in force, if there is one. There is at most one lock, and a
    /// lock object's events stop when the host drops it at the lock's end, so every
    /// `locked`/`finished` that arrives is the current lock's.
    pub(crate) fn lock_input(&mut self, input: LockInput) {
        if let Some(active) = self.lock.as_mut() {
            active.inbox.push(input);
        }
    }
}
