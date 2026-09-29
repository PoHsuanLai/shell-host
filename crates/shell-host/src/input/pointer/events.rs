//! The pointer events a press, release, move and leave produce.
//!
//! - A press is preceded by a synthesized `PointerMove` to the same point, because a surface
//!   under a still pointer may never have received a motion (cosmic-comp #2230), and Blitz
//!   resolves the press target from its hover state.
//! - An enter is followed by the same synthesized move to its position: a pointer that reaches
//!   a surface in one motion gets the enter and no motion, and would hover nothing until it
//!   moved again.
//! - Leaving a surface is a move to (-1, -1), which unhovers through Blitz's own hover change
//!   (so `mouseleave` handlers run), then `clear_hover()` as a backstop; both are the caller's
//!   effect (`SurfaceDocument::pointer_left`).

use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
    PointerDetails, UiEvent,
};
use keyboard_types::Modifiers;

use super::PointerPos;
use super::button::{ButtonChange, buttons_after};

/// A mouse pointer event at `pos`.
pub(crate) fn pointer_event(
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
pub(crate) fn move_event(pos: PointerPos, held: MouseEventButtons, mods: Modifiers) -> UiEvent {
    UiEvent::PointerMove(pointer_event(pos, MouseEventButton::Main, held, mods))
}

/// The events for pressing `button` at `pos`: a move to the same point first, then the down.
/// `held` is the set before the press.
pub(crate) fn press_events(
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
pub(crate) fn release_event(
    pos: PointerPos,
    button: MouseEventButton,
    held: MouseEventButtons,
    mods: Modifiers,
) -> UiEvent {
    let after = buttons_after(held, button, ButtonChange::Released);
    UiEvent::PointerUp(pointer_event(pos, button, after, mods))
}

/// The event sent when the pointer leaves a surface, before `clear_hover()`.
pub(crate) fn leave_event(mods: Modifiers) -> UiEvent {
    move_event(PointerPos::OUTSIDE, MouseEventButtons::None, mods)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
