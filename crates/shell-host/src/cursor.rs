//! The pointer cursor, through `wp_cursor_shape_v1` (KWin and cosmic-comp both have v2). A
//! compositor without it keeps whatever cursor it shows; there is no themed-surface fallback.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use cursor_icon::CursorIcon;
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1;

/// The cursor-shape device for the seat's pointer, and the shape last set.
#[derive(Debug)]
pub struct Cursor {
    pub(crate) manager: Option<CursorShapeManager>,
    pub(crate) device: Option<WpCursorShapeDeviceV1>,
    pub(crate) current: Option<CursorIcon>,
}

impl Cursor {
    /// Show `icon` (or hide the cursor for `None`) on `pointer`, with the serial of its last
    /// `enter`. Does nothing when the shape is already `icon`.
    pub fn set(&mut self, pointer: &WlPointer, serial: u32, icon: Option<CursorIcon>) {
        todo!()
    }
}
