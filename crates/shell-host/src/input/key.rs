//! A keyboard press or release -> `BlitzKeyEvent`, combining the keysym table, the evdev table
//! and the modifier state.

use blitz_traits::SmolStr;
use blitz_traits::events::{BlitzKeyEvent, KeyState};
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::keyboard::KeyEvent;

use crate::input::{keycode, keysym};

/// Whether a key event is the first press or a compositor/sctk key repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    First,
    Repeated,
}

/// Whether an IME composition is in progress on the focused surface (`BlitzKeyEvent::is_composing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Composing {
    Idle,
    Active,
}

/// The Blitz event for `event`: `key` from `keysym::logical_key`, `code` from
/// `keycode::code_from_evdev(event.raw_code)`, `location` from `keysym::location`, `text` from
/// the printable part of `event.utf8` (presses only).
pub fn key_event(
    event: &KeyEvent,
    mods: Modifiers,
    state: KeyState,
    repeat: Repeat,
    composing: Composing,
) -> BlitzKeyEvent {
    let text = match state {
        KeyState::Pressed => printable(event.utf8.as_deref()).map(SmolStr::from),
        KeyState::Released => None,
    };
    BlitzKeyEvent {
        key: keysym::logical_key(event.keysym, event.utf8.as_deref()),
        code: keycode::code_from_evdev(event.raw_code),
        modifiers: mods,
        location: keysym::location(event.keysym),
        is_auto_repeating: repeat == Repeat::Repeated,
        is_composing: composing == Composing::Active,
        state,
        text,
    }
}

/// `raw`, unless it is empty or contains a control character (Ctrl+C's U+0003 is not text).
fn printable(raw: Option<&str>) -> Option<&str> {
    raw.filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
}

#[cfg(test)]
mod tests {
    use blitz_traits::events::KeyState::{Pressed, Released};
    use keyboard_types::{Code, Key, Location};
    use smithay_client_toolkit::seat::keyboard::Keysym;

    use super::*;

    fn event(raw_code: u32, keysym: Keysym, utf8: Option<&str>) -> KeyEvent {
        KeyEvent {
            time: 0,
            raw_code,
            keysym,
            utf8: utf8.map(str::to_owned),
        }
    }

    #[test]
    fn shift_a_is_a_capital_character_on_key_a() {
        let e = event(30, Keysym::A, Some("A"));
        let got = key_event(
            &e,
            Modifiers::SHIFT,
            Pressed,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(got.key, Key::Character("A".into()));
        assert_eq!(got.code, Code::KeyA);
        assert_eq!(got.modifiers, Modifiers::SHIFT);
        assert_eq!(got.text.as_deref(), Some("A"));
        assert!(!got.is_auto_repeating);
        assert!(!got.is_composing);
        assert_eq!(got.state, Pressed);
    }

    #[test]
    fn ctrl_c_is_the_letter_not_the_control_character() {
        // xkb reports the control character as the composed text; the key itself stays "c".
        // `Keysym::c` (lowercase) is the unshifted key; `Keysym::C` is what Shift also held
        // would report.
        let e = event(46, Keysym::c, Some("\u{3}"));
        let got = key_event(
            &e,
            Modifiers::CONTROL,
            Pressed,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(got.key, Key::Character("c".into()));
        assert_eq!(got.code, Code::KeyC);
        assert_eq!(got.modifiers, Modifiers::CONTROL);
        assert_eq!(got.text, None);
    }

    #[test]
    fn super_is_meta_everywhere() {
        let e = event(125, Keysym::Super_L, None);
        let got = key_event(
            &e,
            Modifiers::empty(),
            Pressed,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(got.key, Key::Meta);
        assert_eq!(got.code, Code::MetaLeft);
        assert_eq!(got.location, Location::Left);

        // The next key, with META held (Meta does not shift letter case the way Shift does).
        let a = event(30, Keysym::a, Some("a"));
        let next = key_event(&a, Modifiers::META, Pressed, Repeat::First, Composing::Idle);
        assert_eq!(next.modifiers, Modifiers::META);
    }

    #[test]
    fn keypad_enter_is_enter_on_the_numpad() {
        let e = event(96, Keysym::KP_Enter, Some("\r"));
        let got = key_event(
            &e,
            Modifiers::empty(),
            Pressed,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(got.key, Key::Enter);
        assert_eq!(got.code, Code::NumpadEnter);
        assert_eq!(got.location, Location::Numpad);
    }

    #[test]
    fn repeats_and_releases() {
        let e = event(30, Keysym::A, Some("a"));
        let repeated = key_event(
            &e,
            Modifiers::empty(),
            Pressed,
            Repeat::Repeated,
            Composing::Idle,
        );
        assert!(repeated.is_auto_repeating);
        assert_eq!(repeated.text.as_deref(), Some("a"));

        let release = event(30, Keysym::A, None);
        let up = key_event(
            &release,
            Modifiers::empty(),
            Released,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(up.state, Released);
        assert_eq!(up.text, None);
        assert!(!up.is_auto_repeating);
    }
}
