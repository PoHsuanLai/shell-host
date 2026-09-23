//! The one module allowed `unsafe`: a wgpu surface from raw Wayland handles, and the
//! `wl_display` as a `HasDisplayHandle` for `InstanceDescriptor` (GL needs it at instance
//! creation; SPIKE.md S1 "API changes").
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::ptr::NonNull;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
    WaylandDisplayHandle, WaylandWindowHandle,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

use crate::error::{HostError, Result};

/// A wgpu surface presenting into `surface`.
///
/// Caller contract: the returned surface must drop before `surface` is destroyed and before
/// `conn` closes. `HostSurface` guarantees the first by declaring its presenter before its role
/// (fields drop in declaration order); `HostState`'s `Drop` clears every surface before the
/// connection goes (SPIKE.md S1 finding 6).
pub fn create_surface(
    instance: &wgpu::Instance,
    conn: &Connection,
    surface: &WlSurface,
) -> Result<wgpu::Surface<'static>> {
    let display = display_ptr(conn).ok_or_else(|| {
        HostError::Surface(
            "wl_display pointer is null (wayland-backend without client_system)".into(),
        )
    })?;
    let window = NonNull::new(surface.id().as_ptr().cast::<c_void>()).ok_or_else(|| {
        HostError::Surface("wl_surface pointer is null (surface destroyed)".into())
    })?;
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            display,
        ))),
        raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(window)),
    };
    // SAFETY: both pointers come from live libwayland objects of this process: the display
    // from `conn`, the surface from a `WlSurface` whose proxy is alive (checked non-null
    // above). The caller contract keeps both alive for as long as the returned surface; wgpu
    // uses them only to create the VkSurfaceKHR / EGL window and to attach and commit on
    // present, which is what they are for.
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| HostError::Surface(e.to_string()))
}

fn display_ptr(conn: &Connection) -> Option<NonNull<c_void>> {
    NonNull::new(conn.backend().display_ptr().cast::<c_void>())
}

/// The `wl_display` pointer as a display handle.
#[derive(Debug)]
pub struct WaylandDisplay(NonNull<c_void>);

// SAFETY: the pointer is only an address here; libwayland's `wl_display` may be used from any
// thread, and wgpu only hands it to EGL/Vulkan at instance and surface creation.
unsafe impl Send for WaylandDisplay {}
// SAFETY: as above; nothing is read or written through the pointer by this type.
unsafe impl Sync for WaylandDisplay {}

impl WaylandDisplay {
    /// The display of `conn`; `None` when wayland-backend is not using the system libwayland.
    pub fn new(conn: &Connection) -> Option<WaylandDisplay> {
        display_ptr(conn).map(WaylandDisplay)
    }
}

impl HasDisplayHandle for WaylandDisplay {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(self.0));
        // SAFETY: the pointer came from a live `Connection`, and the instance holding this
        // handle is created from (and dropped with) the host, which keeps that connection
        // open for its whole life; the handle borrows `self`, so it cannot outlive it.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}
