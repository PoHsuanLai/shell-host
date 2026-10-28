//! Buttons are evdev codes 272..=276 (`BTN_LEFT`..`BTN_EXTRA`); anything else is ignored.

use blitz_traits::events::{MouseEventButton, MouseEventButtons};

/// The DOM button for evdev code `code`; `None` for the buttons Blitz has no name for.
pub(crate) fn button_from_evdev(code: u32) -> Option<MouseEventButton> {
    match code {
        0x110 => Some(MouseEventButton::Main),
        0x111 => Some(MouseEventButton::Secondary),
        0x112 => Some(MouseEventButton::Auxiliary),
        0x113 => Some(MouseEventButton::Fourth),
        0x114 => Some(MouseEventButton::Fifth),
        _ => None,
    }
}

/// The held-buttons set after `button` was `pressed` (or released).
pub(crate) fn buttons_after(
    held: MouseEventButtons,
    button: MouseEventButton,
    pressed: bool,
) -> MouseEventButtons {
    if pressed {
        held | MouseEventButtons::from(button)
    } else {
        held - MouseEventButtons::from(button)
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
                true,
                MouseEventButtons::Primary,
            ),
            (
                "press secondary while main held",
                MouseEventButtons::Primary,
                MouseEventButton::Secondary,
                true,
                MouseEventButtons::Primary | MouseEventButtons::Secondary,
            ),
            (
                "release main keeps secondary",
                MouseEventButtons::Primary | MouseEventButtons::Secondary,
                MouseEventButton::Main,
                false,
                MouseEventButtons::Secondary,
            ),
            (
                "release of an unheld button",
                none,
                MouseEventButton::Fifth,
                false,
                none,
            ),
        ];
        for (name, held, button, change, want) in cases {
            assert_eq!(buttons_after(held, button, change), want, "{name}");
        }
    }
}
