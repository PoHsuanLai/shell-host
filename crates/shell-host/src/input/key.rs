//! A keyboard press or release -> `BlitzKeyEvent`, combining the keysym table, the evdev table
//! and the modifier state.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use blitz_traits::events::{BlitzKeyEvent, KeyState};
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::keyboard::KeyEvent;

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
    todo!()
}
