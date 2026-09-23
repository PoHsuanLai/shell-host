//! The one module allowed `unsafe`: a wgpu surface from raw Wayland handles, and the
//! `wl_display` as a `HasDisplayHandle` for `InstanceDescriptor` (GL needs it at instance
//! creation; SPIKE.md S1 "API changes").
#![allow(unsafe_code)]
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::ffi::c_void;
use std::ptr::NonNull;

use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle};
use wayland_client::Connection;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::error::Result;

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
    todo!()
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
        todo!()
    }
}

impl HasDisplayHandle for WaylandDisplay {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        todo!()
    }
}
