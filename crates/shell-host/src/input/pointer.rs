//! Pointer input -> Blitz events: the pure part.
//!
//! The rules (plan "Input", SPIKE.md S1 finding 2, design/13 §13.3.8):
//!
//! - Buttons are evdev codes 272..=276 (`BTN_LEFT`..`BTN_EXTRA`); anything else is ignored.
//! - A press is preceded by a synthesized `PointerMove` to the same point, because a surface
//!   under a still pointer may never have received a motion (cosmic-comp #2230), and Blitz
//!   resolves the press target from its hover state.
//! - An enter is followed by the same synthesized move to its position: a pointer that reaches
//!   a surface in one motion gets the enter and no motion, and would hover nothing until it
//!   moved again (FINDINGS F57).
//! - Leaving a surface is a move to (-1, -1), which unhovers through Blitz's own hover change
//!   (so `mouseleave` handlers run), then `clear_hover()` as a backstop; both are the caller's
//!   effect (`SurfaceDocument::pointer_left`).
//! - A wheel with `value120` (or legacy `discrete`) steps is `Lines`; a continuous source
//!   (touchpad, the palmrest virtual touchpad) is `Pixels`. Wayland's axis sign is the inverse
//!   of the DOM's, so both are negated.
//! - A press within the first-mouse window after a toplevel was activated is swallowed unless
//!   the element opts in with `data-first-mouse` (the caller checks the DOM).
//!
//! Coordinates are surface-local logical pixels, as `wl_pointer` delivers them.

use std::time::Duration;

use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, MouseEventButton, MouseEventButtons,
    PointerCoords, PointerDetails, UiEvent,
};
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::pointer::AxisScroll;

/// A pointer position in surface-local logical pixels, fractional as `wl_pointer` sends it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerPos {
    pub x: f64,
    pub y: f64,
}

impl PointerPos {
    /// Where a leaving pointer is parked: outside every element.
    pub const OUTSIDE: PointerPos = PointerPos { x: -1.0, y: -1.0 };
}

/// Whether a button went down or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonChange {
    Pressed,
    Released,
}

/// Whether a press reaches the document or is swallowed as the click that activated a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstMouse {
    Deliver,
    /// Swallow it, unless the element under it (or an ancestor) carries `data-first-mouse`.
    Swallow,
}

/// The DOM button for evdev code `code`; `None` for the buttons Blitz has no name for.
pub fn button_from_evdev(code: u32) -> Option<MouseEventButton> {
    match code {
        0x110 => Some(MouseEventButton::Main),
        0x111 => Some(MouseEventButton::Secondary),
        0x112 => Some(MouseEventButton::Auxiliary),
        0x113 => Some(MouseEventButton::Fourth),
        0x114 => Some(MouseEventButton::Fifth),
        _ => None,
    }
}

/// The held-buttons set after `button` changed.
pub fn buttons_after(
    held: MouseEventButtons,
    button: MouseEventButton,
    change: ButtonChange,
) -> MouseEventButtons {
    match change {
        ButtonChange::Pressed => held | MouseEventButtons::from(button),
        ButtonChange::Released => held - MouseEventButtons::from(button),
    }
}

/// The wheel delta for one `wl_pointer.frame`'s axis values, or `None` for a frame that only
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

/// A mouse pointer event at `pos`.
pub fn pointer_event(
    pos: PointerPos,
    button: MouseEventButton,
    buttons: MouseEventButtons,
    mods: Modifiers,
) -> BlitzPointerEvent {
    // Blitz stores coordinates as f32 logical px; the narrowing is the boundary's.
    let (x, y) = (pos.x as f32, pos.y as f32);
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods,
        details: PointerDetails::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    }
}

/// A plain motion to `pos` with `held` buttons down.
pub fn move_event(pos: PointerPos, held: MouseEventButtons, mods: Modifiers) -> UiEvent {
    UiEvent::PointerMove(pointer_event(pos, MouseEventButton::Main, held, mods))
}

/// The events for pressing `button` at `pos`: a move to the same point first, then the down.
/// `held` is the set before the press.
pub fn press_events(
    pos: PointerPos,
    button: MouseEventButton,
    held: MouseEventButtons,
    mods: Modifiers,
) -> [UiEvent; 2] {
    let after = buttons_after(held, button, ButtonChange::Pressed);
    [
        move_event(pos, held, mods),
        UiEvent::PointerDown(pointer_event(pos, button, after, mods)),
    ]
}

/// The event for releasing `button` at `pos`. `held` is the set before the release.
pub fn release_event(
    pos: PointerPos,
    button: MouseEventButton,
    held: MouseEventButtons,
    mods: Modifiers,
) -> UiEvent {
    let after = buttons_after(held, button, ButtonChange::Released);
    UiEvent::PointerUp(pointer_event(pos, button, after, mods))
}

/// The event sent when the pointer leaves a surface, before `clear_hover()`.
pub fn leave_event(mods: Modifiers) -> UiEvent {
    move_event(PointerPos::OUTSIDE, MouseEventButtons::None, mods)
}

/// Whether a press is the one that activated its toplevel. `since_activation` is how long
/// before the press the toplevel became active (its `activated` configure state, or
/// `wl_keyboard.enter`); `None` when it was already active. `window` is the settings key
/// `menus.first_mouse_window_ms` (design/22-SETTINGS.md, default 100 ms).
pub fn first_mouse(since_activation: Option<Duration>, window: Duration) -> FirstMouse {
    match since_activation {
        Some(elapsed) if elapsed <= window => FirstMouse::Swallow,
        _ => FirstMouse::Deliver,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_buttons_272_to_276_map_and_others_do_not() {
        let cases = [
            (271, None),
            (272, Some(MouseEventButton::Main)),
            (273, Some(MouseEventButton::Secondary)),
            (274, Some(MouseEventButton::Auxiliary)),
            (275, Some(MouseEventButton::Fourth)),
            (276, Some(MouseEventButton::Fifth)),
            (277, None),
            (0x117, None),
        ];
        for (code, want) in cases {
            assert_eq!(button_from_evdev(code), want, "{code}");
        }
    }

    #[test]
    fn held_buttons_track_presses_and_releases() {
        let none = MouseEventButtons::None;
        let cases = [
            (
                "press main",
                none,
                MouseEventButton::Main,
                ButtonChange::Pressed,
                MouseEventButtons::Primary,
            ),
            (
                "press secondary while main held",
                MouseEventButtons::Primary,
                MouseEventButton::Secondary,
                ButtonChange::Pressed,
                MouseEventButtons::Primary | MouseEventButtons::Secondary,
            ),
            (
                "release main keeps secondary",
                MouseEventButtons::Primary | MouseEventButtons::Secondary,
                MouseEventButton::Main,
                ButtonChange::Released,
                MouseEventButtons::Secondary,
            ),
            (
                "release of an unheld button",
                none,
                MouseEventButton::Fifth,
                ButtonChange::Released,
                none,
            ),
        ];
        for (name, held, button, change, want) in cases {
            assert_eq!(buttons_after(held, button, change), want, "{name}");
        }
    }

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
    fn a_press_is_a_move_then_a_down_at_the_same_point() {
        let pos = PointerPos { x: 50.5, y: 15.0 };
        let [first, second] = press_events(
            pos,
            MouseEventButton::Main,
            MouseEventButtons::None,
            Modifiers::empty(),
        );
        let UiEvent::PointerMove(mv) = first else {
            panic!("first event is not a move: {first:?}");
        };
        let UiEvent::PointerDown(down) = second else {
            panic!("second event is not a down: {second:?}");
        };
        assert_eq!((mv.client_x(), mv.client_y()), (50.5, 15.0));
        assert_eq!((down.client_x(), down.client_y()), (50.5, 15.0));
        assert_eq!(mv.buttons, MouseEventButtons::None);
        assert_eq!(down.buttons, MouseEventButtons::Primary);
        assert_eq!(down.button, MouseEventButton::Main);
    }

    #[test]
    fn a_release_clears_its_button_and_a_leave_parks_outside() {
        let pos = PointerPos { x: 4.0, y: 4.0 };
        let UiEvent::PointerUp(up) = release_event(
            pos,
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            Modifiers::empty(),
        ) else {
            panic!("release is not an up");
        };
        assert_eq!(up.buttons, MouseEventButtons::None);
        assert_eq!(up.button, MouseEventButton::Secondary);
        let UiEvent::PointerMove(leave) = leave_event(Modifiers::SHIFT) else {
            panic!("leave is not a move");
        };
        assert_eq!((leave.client_x(), leave.client_y()), (-1.0, -1.0));
        assert_eq!(leave.mods, Modifiers::SHIFT);
    }

    #[test]
    fn only_a_press_inside_the_window_after_activation_is_swallowed() {
        let window = Duration::from_millis(100);
        let cases = [
            ("already active", None, FirstMouse::Deliver),
            (
                "activated by this click",
                Some(Duration::from_millis(3)),
                FirstMouse::Swallow,
            ),
            (
                "at the edge",
                Some(Duration::from_millis(100)),
                FirstMouse::Swallow,
            ),
            (
                "long after",
                Some(Duration::from_millis(101)),
                FirstMouse::Deliver,
            ),
        ];
        for (name, since, want) in cases {
            assert_eq!(first_mouse(since, window), want, "{name}");
        }
    }
}
