//! The pointer cursor, through `wp_cursor_shape_v1` (KWin and cosmic-comp both have v2). A
//! compositor without it keeps whatever cursor it shows; there is no themed-surface fallback.

use cursor_icon::CursorIcon;
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use wayland_client::Proxy;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};

/// The cursor-shape device for the seat's pointer, and the shape last set.
#[derive(Debug)]
pub(crate) struct Cursor {
    pub(crate) manager: Option<CursorShapeManager>,
    pub(crate) device: Option<WpCursorShapeDeviceV1>,
    pub(crate) current: Option<CursorIcon>,
}

impl Cursor {
    /// Show `icon` (or hide the cursor for `None`) on `pointer`, with the serial of its last
    /// `enter`. Does nothing when the shape is already `icon`.
    pub(crate) fn set(&mut self, pointer: &WlPointer, serial: u32, icon: Option<CursorIcon>) {
        if self.current == icon {
            return;
        }
        match icon {
            None => pointer.set_cursor(serial, None, 0, 0),
            Some(icon) => {
                if let Some(device) = &self.device {
                    device.set_shape(serial, shape_for(icon, device.version()));
                }
            }
        }
        self.current = icon;
    }
}

/// The `wp_cursor_shape_device_v1` shape for `icon`, or `Shape::Default` for a `dnd-ask`/
/// `all-resize` icon on a v1 device (those shapes need `since="2"`) or for a `CursorIcon`
/// variant added after this table was written (`CursorIcon` is `#[non_exhaustive]`).
fn shape_for(icon: CursorIcon, version: u32) -> Shape {
    match icon {
        CursorIcon::Default => Shape::Default,
        CursorIcon::ContextMenu => Shape::ContextMenu,
        CursorIcon::Help => Shape::Help,
        CursorIcon::Pointer => Shape::Pointer,
        CursorIcon::Progress => Shape::Progress,
        CursorIcon::Wait => Shape::Wait,
        CursorIcon::Cell => Shape::Cell,
        CursorIcon::Crosshair => Shape::Crosshair,
        CursorIcon::Text => Shape::Text,
        CursorIcon::VerticalText => Shape::VerticalText,
        CursorIcon::Alias => Shape::Alias,
        CursorIcon::Copy => Shape::Copy,
        CursorIcon::Move => Shape::Move,
        CursorIcon::NoDrop => Shape::NoDrop,
        CursorIcon::NotAllowed => Shape::NotAllowed,
        CursorIcon::Grab => Shape::Grab,
        CursorIcon::Grabbing => Shape::Grabbing,
        CursorIcon::EResize => Shape::EResize,
        CursorIcon::NResize => Shape::NResize,
        CursorIcon::NeResize => Shape::NeResize,
        CursorIcon::NwResize => Shape::NwResize,
        CursorIcon::SResize => Shape::SResize,
        CursorIcon::SeResize => Shape::SeResize,
        CursorIcon::SwResize => Shape::SwResize,
        CursorIcon::WResize => Shape::WResize,
        CursorIcon::EwResize => Shape::EwResize,
        CursorIcon::NsResize => Shape::NsResize,
        CursorIcon::NeswResize => Shape::NeswResize,
        CursorIcon::NwseResize => Shape::NwseResize,
        CursorIcon::ColResize => Shape::ColResize,
        CursorIcon::RowResize => Shape::RowResize,
        CursorIcon::AllScroll => Shape::AllScroll,
        CursorIcon::ZoomIn => Shape::ZoomIn,
        CursorIcon::ZoomOut => Shape::ZoomOut,
        CursorIcon::DndAsk if version >= 2 => Shape::DndAsk,
        CursorIcon::AllResize if version >= 2 => Shape::AllResize,
        _ => Shape::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_shape_round_trips_by_name_at_version_2() {
        let cases = [
            (CursorIcon::Default, Shape::Default),
            (CursorIcon::Pointer, Shape::Pointer),
            (CursorIcon::Text, Shape::Text),
            (CursorIcon::Grabbing, Shape::Grabbing),
            (CursorIcon::NwseResize, Shape::NwseResize),
            (CursorIcon::AllScroll, Shape::AllScroll),
            (CursorIcon::ZoomOut, Shape::ZoomOut),
            (CursorIcon::DndAsk, Shape::DndAsk),
            (CursorIcon::AllResize, Shape::AllResize),
        ];
        for (icon, want) in cases {
            assert_eq!(shape_for(icon, 2), want, "{icon:?}");
        }
    }

    #[test]
    fn version_2_only_shapes_fall_back_to_default_on_a_v1_device() {
        assert_eq!(shape_for(CursorIcon::DndAsk, 1), Shape::Default);
        assert_eq!(shape_for(CursorIcon::AllResize, 1), Shape::Default);
        // An ordinary shape is unaffected by the device version.
        assert_eq!(shape_for(CursorIcon::Pointer, 1), Shape::Pointer);
    }
}
