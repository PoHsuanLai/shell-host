//! The surface's `wl_surface` as its document sees it. A hidden surface is rebuilt with a new
//! `wl_surface` (`ctx::spawn::rebuild`), so the handle reads it through a cell the host
//! refreshes, and never keeps a stale object.
//!
//! See FINDINGS F34.

use std::cell::RefCell;
use std::rc::Rc;

use wayland_client::protocol::wl_surface::WlSurface;

/// One surface's current `wl_surface`, shared by the host and its `SurfaceHandle`.
#[derive(Debug, Clone)]
pub(crate) struct WlCell(Rc<RefCell<Option<WlSurface>>>);

impl WlCell {
    /// A cell holding `surface`.
    pub(crate) fn new(surface: WlSurface) -> WlCell {
        WlCell(Rc::new(RefCell::new(Some(surface))))
    }

    /// A cell with no surface: handles built without a connection (tests).
    #[cfg(test)]
    pub(crate) fn detached() -> WlCell {
        WlCell(Rc::new(RefCell::new(None)))
    }

    /// Replace the surface (the host rebuilt the role).
    pub(crate) fn set(&self, surface: WlSurface) {
        *self.0.borrow_mut() = Some(surface);
    }

    /// The surface now; `None` only for a detached cell.
    pub(crate) fn get(&self) -> Option<WlSurface> {
        self.0.borrow().clone()
    }
}
