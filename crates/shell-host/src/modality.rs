//! How the person last drove the seat, pointer or keyboard, published to every surface's
//! document (`use_input_modality`): what a design system's
//! `:focus-visible`-like focus ring follows. Blitz never matches `:focus-visible` (a quire
//! finding), so quire draws its ring under `.ds[data-modality=keyboard]` and asks the host, which
//! sees the raw input, which modality is current (`ds::HostModality`).
//!
//! One value per seat, shared by every surface: a key press in the launcher and a Tab into a
//! bar menu are the same person at the same keyboard, and a surface mapped mid-session (a popup
//! opened with a key) starts from the current value rather than from `Pointer`. The rule is
//! quire's own host's (`ds-native`'s `modality_after`) and the one browsers use for
//! `:focus-visible`: a key press that is not a lone modifier turns it `Keyboard`, a pointer
//! button press turns it `Pointer`, and nothing else changes it — not pointer motion, not a
//! scroll, not a release or an auto-repeat. Motion is deliberately left out: a hand brushing
//! the mouse (or a palm on the touchpad) in the middle of a Tab walk would otherwise hide the
//! ring it is following.

use keyboard_types::Key;
use smithay_client_toolkit::seat::keyboard::Keysym;

/// The kind of input the seat last saw that decides the modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputModality {
    /// The last deciding input was a pointer press (and before any input at all): no focus
    /// rings.
    #[default]
    Pointer,
    /// The last deciding input was a key: focus rings show.
    Keyboard,
}

impl InputModality {
    /// The conventional attribute value: quire's `data-modality`.
    pub fn slug(self) -> &'static str {
        match self {
            InputModality::Pointer => "pointer",
            InputModality::Keyboard => "keyboard",
        }
    }
}

/// Whether a pressed key is a modifier on its own (Shift, Ctrl, Alt, Super, a level shift, a
/// lock key): usually half of a pointer gesture (Shift+click), so it keeps the modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Modifier,
    Other,
}

/// One input event, as far as the modality cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    /// A key went down (the first press, not an auto-repeat).
    KeyPress(KeyKind),
    /// A key came up.
    KeyRelease,
    /// A pointer button went down (a real `wl_pointer.button`, or an injected press).
    PointerPress,
    /// A pointer button came up.
    PointerRelease,
    /// The pointer moved, entered or left.
    PointerMotion,
    /// A wheel or touchpad scroll.
    Scroll,
}

/// The modality after `stroke`, from `current`.
pub fn after(current: InputModality, stroke: Stroke) -> InputModality {
    match stroke {
        Stroke::KeyPress(KeyKind::Other) => InputModality::Keyboard,
        Stroke::PointerPress => InputModality::Pointer,
        Stroke::KeyPress(KeyKind::Modifier)
        | Stroke::KeyRelease
        | Stroke::PointerRelease
        | Stroke::PointerMotion
        | Stroke::Scroll => current,
    }
}

/// Whether the key `keysym` names is a lone modifier (xkb's own list: Shift, Control, Alt,
/// Meta, Super, Hyper, the ISO level shifts and the lock keys).
pub fn key_kind(keysym: Keysym) -> KeyKind {
    if keysym.is_modifier_key() {
        KeyKind::Modifier
    } else {
        KeyKind::Other
    }
}

/// Whether the DOM key `key` is a lone modifier: `key_kind` for a key that has no keysym.
pub fn key_kind_of(key: &Key) -> KeyKind {
    match key {
        Key::Alt
        | Key::AltGraph
        | Key::CapsLock
        | Key::Control
        | Key::Fn
        | Key::FnLock
        | Key::Meta
        | Key::NumLock
        | Key::ScrollLock
        | Key::Shift
        | Key::Symbol
        | Key::SymbolLock
        | Key::Hyper
        | Key::Super => KeyKind::Modifier,
        _ => KeyKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use InputModality::{Keyboard, Pointer};

    #[test]
    fn only_a_key_press_or_a_pointer_press_decides() {
        let cases: &[(&str, InputModality, Stroke, InputModality)] = &[
            (
                "Tab shows the ring",
                Pointer,
                Stroke::KeyPress(KeyKind::Other),
                Keyboard,
            ),
            (
                "another key keeps it",
                Keyboard,
                Stroke::KeyPress(KeyKind::Other),
                Keyboard,
            ),
            ("a press hides it", Keyboard, Stroke::PointerPress, Pointer),
            (
                "a press on pointer stays",
                Pointer,
                Stroke::PointerPress,
                Pointer,
            ),
            (
                "a lone Shift keeps pointer",
                Pointer,
                Stroke::KeyPress(KeyKind::Modifier),
                Pointer,
            ),
            (
                "a lone Shift keeps keyboard",
                Keyboard,
                Stroke::KeyPress(KeyKind::Modifier),
                Keyboard,
            ),
            (
                "a key release changes nothing",
                Pointer,
                Stroke::KeyRelease,
                Pointer,
            ),
            (
                "a button release changes nothing",
                Keyboard,
                Stroke::PointerRelease,
                Keyboard,
            ),
            (
                "motion keeps the ring",
                Keyboard,
                Stroke::PointerMotion,
                Keyboard,
            ),
            (
                "motion keeps pointer",
                Pointer,
                Stroke::PointerMotion,
                Pointer,
            ),
            (
                "a scroll keeps the ring",
                Keyboard,
                Stroke::Scroll,
                Keyboard,
            ),
        ];
        for &(name, current, stroke, expected) in cases {
            assert_eq!(after(current, stroke), expected, "{name}");
        }
    }

    #[test]
    fn modifier_keys_are_told_apart_from_the_rest() {
        let cases: &[(Keysym, KeyKind)] = &[
            (Keysym::Tab, KeyKind::Other),
            (Keysym::Right, KeyKind::Other),
            (Keysym::Return, KeyKind::Other),
            (Keysym::Escape, KeyKind::Other),
            (Keysym::space, KeyKind::Other),
            (Keysym::a, KeyKind::Other),
            (Keysym::Shift_L, KeyKind::Modifier),
            (Keysym::Control_R, KeyKind::Modifier),
            (Keysym::Alt_L, KeyKind::Modifier),
            (Keysym::Super_L, KeyKind::Modifier),
            (Keysym::ISO_Level3_Shift, KeyKind::Modifier),
            (Keysym::Caps_Lock, KeyKind::Modifier),
        ];
        for &(keysym, expected) in cases {
            assert_eq!(key_kind(keysym), expected, "{keysym:?}");
        }
    }

    #[test]
    fn the_slug_is_quires_data_modality() {
        assert_eq!(Pointer.slug(), "pointer");
        assert_eq!(Keyboard.slug(), "keyboard");
        assert_eq!(InputModality::default(), Pointer);
    }
}
