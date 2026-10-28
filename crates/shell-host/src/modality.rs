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

#[cfg(feature = "debug-inject")]
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

/// One input event, as far as the modality cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// A key went down (the first press, not an auto-repeat). `modifier`: it is a modifier on
    /// its own (Shift, Ctrl, Alt, Super, a level shift, a lock key), usually half of a pointer
    /// gesture (Shift+click), so it keeps the modality.
    KeyPress { modifier: bool },
    /// A pointer button went down (a real `wl_pointer.button`, or an injected press).
    PointerPress,
}

/// The modality after `stroke`, from `current`.
pub(crate) fn after(current: InputModality, stroke: Stroke) -> InputModality {
    match stroke {
        Stroke::KeyPress { modifier: false } => InputModality::Keyboard,
        Stroke::PointerPress => InputModality::Pointer,
        Stroke::KeyPress { modifier: true } => current,
    }
}

/// Whether the key `keysym` names is a lone modifier (xkb's own list: Shift, Control, Alt,
/// Meta, Super, Hyper, the ISO level shifts and the lock keys).
pub(crate) fn is_modifier(keysym: Keysym) -> bool {
    keysym.is_modifier_key()
}

#[cfg(feature = "debug-inject")]
/// Whether the DOM key `key` is a lone modifier: `is_modifier` for a key that has no keysym.
pub(crate) fn is_modifier_key(key: &Key) -> bool {
    matches!(
        key,
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
            | Key::Super
    )
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
                Stroke::KeyPress { modifier: false },
                Keyboard,
            ),
            (
                "another key keeps it",
                Keyboard,
                Stroke::KeyPress { modifier: false },
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
                Stroke::KeyPress { modifier: true },
                Pointer,
            ),
            (
                "a lone Shift keeps keyboard",
                Keyboard,
                Stroke::KeyPress { modifier: true },
                Keyboard,
            ),
        ];
        for &(name, current, stroke, expected) in cases {
            assert_eq!(after(current, stroke), expected, "{name}");
        }
    }

    #[test]
    fn modifier_keys_are_told_apart_from_the_rest() {
        let cases: &[(Keysym, bool)] = &[
            (Keysym::Tab, false),
            (Keysym::Right, false),
            (Keysym::Return, false),
            (Keysym::Escape, false),
            (Keysym::space, false),
            (Keysym::a, false),
            (Keysym::Shift_L, true),
            (Keysym::Control_R, true),
            (Keysym::Alt_L, true),
            (Keysym::Super_L, true),
            (Keysym::ISO_Level3_Shift, true),
            (Keysym::Caps_Lock, true),
        ];
        for &(keysym, expected) in cases {
            assert_eq!(is_modifier(keysym), expected, "{keysym:?}");
        }
    }

    #[test]
    fn the_slug_is_quires_data_modality() {
        assert_eq!(Pointer.slug(), "pointer");
        assert_eq!(Keyboard.slug(), "keyboard");
        assert_eq!(InputModality::default(), Pointer);
    }
}
