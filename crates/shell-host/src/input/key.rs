//! A keyboard press or release -> `BlitzKeyEvent`, combining the keysym table, the evdev table
//! and the modifier state.

use blitz_traits::SmolStr;
use blitz_traits::events::{BlitzKeyEvent, KeyState};
use keyboard_types::{Key, Modifiers};
use smithay_client_toolkit::seat::keyboard::KeyEvent;

use crate::input::{keycode, keysym};

/// Whether a key event is the first press or a compositor/sctk key repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repeat {
    First,
    Repeated,
}

/// Whether an IME composition is in progress on the focused surface (`BlitzKeyEvent::is_composing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Composing {
    Idle,
    Active,
}

/// The Blitz event for `event`: `key` from `keysym::logical_key`, `code` from
/// `keycode::code_from_evdev(event.raw_code)`, `location` from `keysym::location`, `text` from
/// the printable part of `event.utf8` (presses only).
pub(crate) fn key_event(
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
    let key = keysym::logical_key(event.keysym, event.utf8.as_deref());
    BlitzKeyEvent {
        key_without_modifiers: unshifted(&key),
        key,
        code: keycode::code_from_evdev(event.raw_code),
        modifiers: mods,
        location: keysym::location(event.keysym),
        is_auto_repeating: repeat == Repeat::Repeated,
        is_composing: composing == Composing::Active,
        state,
        text,
    }
}

/// `key` with Shift taken off, as far as the keysym tells: a letter is lower case. The layout's
/// unshifted symbol for other keys (`!` on `1`) is not in the keysym, so those stay as they are.
pub(crate) fn unshifted(key: &Key) -> Key {
    match key {
        Key::Character(text) if text.chars().count() == 1 => Key::Character(text.to_lowercase()),
        other => other.clone(),
    }
}

/// `raw`, unless it is empty or contains a control character (Ctrl+C's U+0003 is not text).
fn printable(raw: Option<&str>) -> Option<&str> {
    raw.filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
}

#[cfg(test)]
mod tests;
