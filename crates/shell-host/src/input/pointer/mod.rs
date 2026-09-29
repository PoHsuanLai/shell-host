//! Pointer input -> Blitz events: the pure part.
//!
//! Coordinates are surface-local logical pixels, as `wl_pointer` delivers them.

mod button;
mod events;
mod first_mouse;
mod wheel;

pub(crate) use button::{ButtonChange, button_from_evdev, buttons_after};
pub(crate) use events::{leave_event, move_event, press_events, release_event};
pub(crate) use first_mouse::{FirstMouse, first_mouse};
pub(crate) use wheel::{axis_frame, scrolled, wheel_delta, wheel_event};

/// A pointer position in surface-local logical pixels, fractional as `wl_pointer` sends it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PointerPos {
    pub x: f64,
    pub y: f64,
}

impl PointerPos {
    /// Where a leaving pointer is parked: outside every element.
    pub(crate) const OUTSIDE: PointerPos = PointerPos { x: -1.0, y: -1.0 };
}
