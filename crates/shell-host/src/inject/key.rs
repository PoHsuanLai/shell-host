//! Delivering an injected key through the point a real xkb key reaches: `HostState::key_down`
//! and `key_up`, after `handlers/keyboard.rs` has translated the keysym.

use blitz_traits::SmolStr;
use blitz_traits::events::{BlitzKeyEvent, KeyState};
use keyboard_types::{Code, Key, Location, Modifiers};

use super::{KeyAction, KeyStroke};
use crate::id::SurfaceId;
use crate::input::key::Composing;
use crate::modality::{Stroke, key_kind_of};
use crate::state::HostState;

/// Do `action` with `stroke` on surface `id`. A press notes the modality as a real press
/// does; a tap is a press then a release.
pub(super) fn deliver_key(
    state: &mut HostState,
    id: SurfaceId,
    stroke: &KeyStroke,
    action: KeyAction,
) {
    // The physical key's identity for the scroll engine's held-key tracking, which real input
    // keys by evdev code; a `Code` never collides with one it is not equal to.
    let raw = stroke.code as u32;
    let composing = state.composing();
    if action != KeyAction::Release {
        state.note_stroke(Stroke::KeyPress(key_kind_of(&stroke.key)));
        state.key_down(id, blitz_key(stroke, KeyState::Pressed, composing), raw);
    }
    if action != KeyAction::Press {
        state.key_up(id, blitz_key(stroke, KeyState::Released, composing), raw);
    }
}

/// The Blitz event for `stroke`: the same fields `input::key::key_event` fills from a real key.
/// A character types itself unless Control is held (a real Ctrl+K arrives as a control
/// character, which is not text).
fn blitz_key(stroke: &KeyStroke, state: KeyState, composing: Composing) -> BlitzKeyEvent {
    let text = match (&stroke.key, state) {
        (Key::Character(text), KeyState::Pressed) if !stroke.mods.contains(Modifiers::CONTROL) => {
            Some(SmolStr::from(text.as_str()))
        }
        _ => None,
    };
    BlitzKeyEvent {
        key: stroke.key.clone(),
        code: stroke.code,
        modifiers: stroke.mods,
        location: location_of(stroke.code),
        is_auto_repeating: false,
        is_composing: composing == Composing::Active,
        state,
        text,
    }
}

fn location_of(code: Code) -> Location {
    match code {
        Code::ShiftLeft | Code::ControlLeft | Code::AltLeft | Code::MetaLeft => Location::Left,
        Code::ShiftRight | Code::ControlRight | Code::AltRight | Code::MetaRight => Location::Right,
        _ => Location::Standard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modality::KeyKind;

    fn stroke(key: Key, code: Code, mods: Modifiers) -> KeyStroke {
        KeyStroke { key, code, mods }
    }

    #[test]
    fn a_character_press_types_and_a_release_or_ctrl_press_does_not() {
        let k = || Key::Character("k".to_owned());
        let cases: &[(&str, KeyStroke, KeyState, Option<&str>)] = &[
            (
                "plain press",
                stroke(k(), Code::KeyK, Modifiers::empty()),
                KeyState::Pressed,
                Some("k"),
            ),
            (
                "release",
                stroke(k(), Code::KeyK, Modifiers::empty()),
                KeyState::Released,
                None,
            ),
            (
                "ctrl press",
                stroke(k(), Code::KeyK, Modifiers::CONTROL),
                KeyState::Pressed,
                None,
            ),
            (
                "named key",
                stroke(Key::Tab, Code::Tab, Modifiers::empty()),
                KeyState::Pressed,
                None,
            ),
        ];
        for (name, stroke, state, want) in cases {
            let event = blitz_key(stroke, *state, Composing::Idle);
            assert_eq!(event.text.as_deref(), *want, "{name}");
            assert_eq!(event.state, *state, "{name}");
            assert_eq!(event.modifiers, stroke.mods, "{name}");
            assert!(!event.is_auto_repeating, "{name}");
        }
    }

    #[test]
    fn a_side_specific_modifier_says_which_side() {
        let cases = [
            (Code::ShiftLeft, Location::Left),
            (Code::ControlRight, Location::Right),
            (Code::KeyA, Location::Standard),
        ];
        for (code, want) in cases {
            assert_eq!(location_of(code), want, "{code:?}");
        }
    }

    #[test]
    fn only_modifier_keys_are_modifiers() {
        assert_eq!(key_kind_of(&Key::Shift), KeyKind::Modifier);
        assert_eq!(key_kind_of(&Key::Tab), KeyKind::Other);
        assert_eq!(key_kind_of(&Key::Character("a".to_owned())), KeyKind::Other);
    }
}
