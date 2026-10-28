//! Delivering an injected key through the point a real xkb key reaches: `HostState::key_down`
//! and `key_up`, after `handlers/keyboard.rs` has translated the keysym.

use blitz_traits::SmolStr;
use blitz_traits::events::{BlitzKeyEvent, KeyState};
use keyboard_types::{Code, Key, Location, Modifiers};

use super::{KeyAction, KeyStroke};
use crate::id::SurfaceId;
use crate::input::key::unshifted;
use crate::modality::{Stroke, is_modifier_key};
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
        state.note_stroke(Stroke::KeyPress {
            modifier: is_modifier_key(&stroke.key),
        });
        state.key_down(id, blitz_key(stroke, KeyState::Pressed, composing), raw);
    }
    if action != KeyAction::Press {
        state.key_up(id, blitz_key(stroke, KeyState::Released, composing), raw);
    }
    let after = modifiers_after(state.input.modifiers, stroke, action);
    state.set_modifiers(id, after);
}

/// The DOM modifier flag a modifier key holds, if it is one.
fn flag_of(key: &Key) -> Modifiers {
    match key {
        Key::Alt => Modifiers::ALT,
        Key::Control => Modifiers::CONTROL,
        Key::Shift => Modifiers::SHIFT,
        Key::Meta => Modifiers::META,
        Key::Super => Modifiers::SUPER,
        _ => Modifiers::empty(),
    }
}

/// The seat's modifiers after `action` with `stroke`, as the compositor's `wl_keyboard.modifiers`
/// would report them: a press holds the key's own flag and the stroke's `mods`, a release drops
/// them, and a tap holds them only for its own press, so it leaves whatever was held before it
/// (a modifier tapped on its own ends released). Locks (caps, num) are never touched.
fn modifiers_after(before: Modifiers, stroke: &KeyStroke, action: KeyAction) -> Modifiers {
    let flags = flag_of(&stroke.key) | (stroke.mods & !Modifiers::CAPS_LOCK & !Modifiers::NUM_LOCK);
    match action {
        KeyAction::Press => before | flags,
        KeyAction::Release => before & !flags,
        KeyAction::Tap => before & !flag_of(&stroke.key),
    }
}

/// The Blitz event for `stroke`: the same fields `input::key::key_event` fills from a real key.
/// A character types itself unless Control is held (a real Ctrl+K arrives as a control
/// character, which is not text).
fn blitz_key(stroke: &KeyStroke, state: KeyState, composing: bool) -> BlitzKeyEvent {
    let text = match (&stroke.key, state) {
        (Key::Character(text), KeyState::Pressed) if !stroke.mods.contains(Modifiers::CONTROL) => {
            Some(SmolStr::from(text.as_str()))
        }
        _ => None,
    };
    BlitzKeyEvent {
        key_without_modifiers: unshifted(&stroke.key),
        key: stroke.key.clone(),
        code: stroke.code,
        modifiers: stroke.mods,
        location: location_of(stroke.code),
        is_auto_repeating: false,
        is_composing: composing,
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
    use crate::modifiers::{Modifier, Modifiers as HeldModifiers};

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
            let event = blitz_key(stroke, *state, false);
            assert_eq!(event.text.as_deref(), *want, "{name}");
            assert_eq!(event.state, *state, "{name}");
            assert_eq!(event.modifiers, stroke.mods, "{name}");
            assert!(!event.is_auto_repeating, "{name}");
        }
    }

    #[test]
    fn injected_modifier_keys_and_mods_move_the_seat_modifiers() {
        let key = |key: Key, code, mods| KeyStroke { key, code, mods };
        let alt = || key(Key::Alt, Code::AltLeft, Modifiers::empty());
        let tab = |mods| key(Key::Tab, Code::Tab, mods);
        let none = Modifiers::empty();
        let cases: &[(&str, Modifiers, KeyStroke, KeyAction, Modifiers)] = &[
            ("alt press", none, alt(), KeyAction::Press, Modifiers::ALT),
            (
                "alt release",
                Modifiers::ALT,
                alt(),
                KeyAction::Release,
                none,
            ),
            ("alt tap", none, alt(), KeyAction::Tap, none),
            (
                "tab while alt held",
                Modifiers::ALT,
                tab(none),
                KeyAction::Tap,
                Modifiers::ALT,
            ),
            (
                "tap with mods leaves nothing stuck",
                none,
                tab(Modifiers::ALT | Modifiers::SHIFT),
                KeyAction::Tap,
                none,
            ),
            (
                "tap with mods keeps an earlier hold",
                Modifiers::ALT,
                tab(Modifiers::ALT),
                KeyAction::Tap,
                Modifiers::ALT,
            ),
            (
                "press with mods holds them",
                none,
                tab(Modifiers::CONTROL),
                KeyAction::Press,
                Modifiers::CONTROL,
            ),
            (
                "release with mods drops them",
                Modifiers::CONTROL | Modifiers::ALT,
                tab(Modifiers::CONTROL),
                KeyAction::Release,
                Modifiers::ALT,
            ),
            (
                "locks are untouched",
                Modifiers::CAPS_LOCK,
                alt(),
                KeyAction::Press,
                Modifiers::CAPS_LOCK | Modifiers::ALT,
            ),
        ];
        for (name, before, stroke, action, want) in cases {
            assert_eq!(modifiers_after(*before, stroke, *action), *want, "{name}");
        }
        let held = HeldModifiers::default().with_dom(Modifiers::ALT);
        assert_eq!(held.alt, Modifier::Held);
        assert_eq!(held.shift, Modifier::Released);
        assert_eq!(
            HeldModifiers::default().with_dom(Modifiers::SUPER).logo,
            Modifier::Held
        );
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
        assert!(is_modifier_key(&Key::Shift));
        assert!(!is_modifier_key(&Key::Tab));
        assert!(!is_modifier_key(&Key::Character("a".to_owned())));
    }
}
