//! A wheel with `value120` (or legacy `discrete`) steps is `Lines`; a continuous source
//! (touchpad, the palmrest virtual touchpad) is `Pixels`. Wayland's axis sign is the inverse of
//! the DOM's, so both are negated.

use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButtons, PointerCoords, UiEvent,
};
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::pointer::AxisScroll;
use wayland_client::protocol::wl_pointer;

use super::PointerPos;
use crate::scroll::source::{AxisFrame, AxisSource, AxisValue, Relative, Shift, Stop};

/// stops a scroll (`axis_stop`) or carries nothing.
pub fn wheel_delta(horizontal: AxisScroll, vertical: AxisScroll) -> Option<BlitzWheelDelta> {
    let steps = |axis: AxisScroll| {
        if axis.value120 != 0 {
            f64::from(axis.value120) / 120.0
        } else {
            f64::from(axis.discrete)
        }
    };
    let (sx, sy) = (steps(horizontal), steps(vertical));
    if sx != 0.0 || sy != 0.0 {
        return Some(BlitzWheelDelta::Lines(-sx, -sy));
    }
    let (px, py) = (horizontal.absolute, vertical.absolute);
    if px != 0.0 || py != 0.0 {
        return Some(BlitzWheelDelta::Pixels(-px, -py));
    }
    None
}

/// One `wl_pointer.frame`'s axis content for the scroll engine (`scroll::source`).
pub fn axis_frame(
    horizontal: AxisScroll,
    vertical: AxisScroll,
    source: Option<wl_pointer::AxisSource>,
    mods: Modifiers,
) -> AxisFrame {
    let value = |axis: AxisScroll| AxisValue {
        px: axis.absolute,
        v120: axis.value120,
        discrete: axis.discrete,
        stop: if axis.stop {
            Stop::Stopped
        } else {
            Stop::Moving
        },
        relative: match axis.relative_direction {
            Some(wl_pointer::AxisRelativeDirection::Inverted) => Relative::Inverted,
            _ => Relative::Identical,
        },
    };
    AxisFrame {
        source: match source {
            Some(wl_pointer::AxisSource::Wheel) => AxisSource::Wheel,
            Some(wl_pointer::AxisSource::Finger) => AxisSource::Finger,
            Some(wl_pointer::AxisSource::Continuous) => AxisSource::Continuous,
            Some(wl_pointer::AxisSource::WheelTilt) => AxisSource::WheelTilt,
            _ => AxisSource::Unknown,
        },
        horizontal: value(horizontal),
        vertical: value(vertical),
        shift: if mods.contains(Modifiers::SHIFT) {
            Shift::Held
        } else {
            Shift::Up
        },
    }
}

/// The raw wheel event a `data-wheel="capture"` element receives, at `pos`.
pub fn wheel_event(
    delta: BlitzWheelDelta,
    pos: PointerPos,
    buttons: MouseEventButtons,
    mods: Modifiers,
) -> UiEvent {
    let (x, y) = (pos.x as f32, pos.y as f32);
    UiEvent::Wheel(BlitzWheelEvent {
        delta,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        buttons,
        mods,
        element: Default::default(),
    })
}

/// `event` with its page coordinates moved by the viewport's `scroll` from its client
/// coordinates: Blitz hit-tests page coordinates, and the host's events are surface-local, so
/// once the engine scrolls the viewport the two differ.
pub fn scrolled(event: UiEvent, scroll: (f64, f64)) -> UiEvent {
    let shift = |coords: &mut PointerCoords| {
        coords.page_x = coords.client_x + scroll.0 as f32;
        coords.page_y = coords.client_y + scroll.1 as f32;
    };
    match event {
        UiEvent::PointerMove(mut e) => {
            shift(&mut e.coords);
            UiEvent::PointerMove(e)
        }
        UiEvent::PointerDown(mut e) => {
            shift(&mut e.coords);
            UiEvent::PointerDown(e)
        }
        UiEvent::PointerUp(mut e) => {
            shift(&mut e.coords);
            UiEvent::PointerUp(e)
        }
        UiEvent::Wheel(mut e) => {
            shift(&mut e.coords);
            UiEvent::Wheel(e)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::super::events::move_event;
    use super::*;

    fn axis(absolute: f64, discrete: i32, value120: i32) -> AxisScroll {
        AxisScroll {
            absolute,
            discrete,
            value120,
            ..AxisScroll::default()
        }
    }

    fn delta(d: Option<BlitzWheelDelta>) -> Option<(char, f64, f64)> {
        d.map(|d| match d {
            BlitzWheelDelta::Lines(x, y) => ('L', x, y),
            BlitzWheelDelta::Pixels(x, y) => ('P', x, y),
        })
    }

    #[test]
    fn wheel_steps_are_lines_and_continuous_scroll_is_pixels() {
        let none = AxisScroll::default();
        let stop = AxisScroll {
            stop: true,
            ..AxisScroll::default()
        };
        let cases = [
            (
                "one notch down",
                none,
                axis(15.0, 1, 120),
                Some(('L', -0.0, -1.0)),
            ),
            (
                "hi-res half notch",
                none,
                axis(7.5, 0, 60),
                Some(('L', -0.0, -0.5)),
            ),
            (
                "legacy discrete only",
                none,
                axis(10.0, -2, 0),
                Some(('L', -0.0, 2.0)),
            ),
            (
                "horizontal notch",
                axis(15.0, 1, 120),
                none,
                Some(('L', -1.0, -0.0)),
            ),
            (
                "touchpad",
                axis(-3.0, 0, 0),
                axis(12.5, 0, 0),
                Some(('P', 3.0, -12.5)),
            ),
            ("stop only", none, stop, None),
            ("nothing", none, none, None),
        ];
        for (name, h, v, want) in cases {
            assert_eq!(delta(wheel_delta(h, v)), want, "{name}");
        }
    }

    #[test]
    fn a_scrolled_viewport_moves_page_coordinates_only() {
        let pos = PointerPos { x: 10.0, y: 20.0 };
        let moved = scrolled(
            move_event(pos, MouseEventButtons::None, Modifiers::empty()),
            (0.0, 300.0),
        );
        let UiEvent::PointerMove(e) = moved else {
            panic!("not a move");
        };
        assert_eq!((e.client_x(), e.client_y()), (10.0, 20.0));
        assert_eq!((e.page_x(), e.page_y()), (10.0, 320.0));
        let again = scrolled(UiEvent::PointerMove(e), (0.0, 300.0));
        let UiEvent::PointerMove(e) = again else {
            panic!("not a move");
        };
        assert_eq!(
            e.page_y(),
            320.0,
            "from the client coordinates, so idempotent"
        );
    }

    #[test]
    fn an_axis_frame_keeps_source_stop_and_shift() {
        let stop = AxisScroll {
            stop: true,
            ..AxisScroll::default()
        };
        let frame = axis_frame(
            AxisScroll::default(),
            stop,
            Some(wl_pointer::AxisSource::Finger),
            Modifiers::SHIFT,
        );
        assert_eq!(frame.source, AxisSource::Finger);
        assert_eq!(frame.vertical.stop, Stop::Stopped);
        assert_eq!(frame.shift, Shift::Held);
        let frame = axis_frame(stop, stop, None, Modifiers::empty());
        assert_eq!(
            (frame.source, frame.shift),
            (AxisSource::Unknown, Shift::Up)
        );
    }
}
