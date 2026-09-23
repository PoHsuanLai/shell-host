//! The one unsafe module: a wgpu surface from raw Wayland handles.
#![allow(unsafe_code)]

use std::ptr::NonNull;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

/// Creates a wgpu surface presenting into `surface`.
///
/// The caller must drop the returned surface before `surface` is destroyed and before `conn`
/// is closed. `SpikeSurface` guarantees the first by declaring its wgpu surface field before
/// the layer surface that owns the `wl_surface` (fields drop in declaration order), and the
/// connection lives for the whole process.
pub fn create_surface(
    instance: &wgpu::Instance,
    conn: &Connection,
    surface: &WlSurface,
) -> Result<wgpu::Surface<'static>, String> {
    let display = NonNull::new(conn.backend().display_ptr().cast())
        .ok_or("wl_display pointer is null (wayland-backend without client_system?)")?;
    let window = NonNull::new(surface.id().as_ptr().cast()).ok_or("wl_surface pointer is null")?;
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            display,
        ))),
        raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(window)),
    };
    // SAFETY: both pointers come from live libwayland objects owned by this process: the
    // display from `conn` (alive for the process) and the surface from a `WlSurface` that,
    // per the contract above, outlives the returned `Surface`. wgpu only uses them to create
    // a VkSurfaceKHR / EGL window and to commit on present, which is what they are for.
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
}

/// The wl_display as a `HasDisplayHandle`, which wgpu's GL backend needs at instance creation.
#[derive(Debug)]
pub struct Display(NonNull<std::ffi::c_void>);

// SAFETY: a wl_display pointer is only an address here; libwayland's display is thread-safe
// and wgpu only hands it to EGL.
unsafe impl Send for Display {}
// SAFETY: as above.
unsafe impl Sync for Display {}

impl Display {
    pub fn new(conn: &Connection) -> Option<Self> {
        NonNull::new(conn.backend().display_ptr().cast()).map(Self)
    }
}

impl raw_window_handle::HasDisplayHandle for Display {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(self.0));
        // SAFETY: the connection (and so the display) lives for the whole process.
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) })
    }
}
