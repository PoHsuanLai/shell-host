//! sctk's modifier state -> `keyboard_types::Modifiers`.

use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::keyboard::Modifiers as SctkModifiers;

/// The keyboard-types set for sctk's modifier state. The logo key is `META`, the W3C name
/// (what `KeyboardEvent.metaKey` reports), matching `keysym::named_key`'s `Key::Meta`.
pub(crate) fn modifiers_from(state: SctkModifiers) -> Modifiers {
    [
        (state.ctrl, Modifiers::CONTROL),
        (state.alt, Modifiers::ALT),
        (state.shift, Modifiers::SHIFT),
        (state.logo, Modifiers::META),
        (state.caps_lock, Modifiers::CAPS_LOCK),
        (state.num_lock, Modifiers::NUM_LOCK),
    ]
    .into_iter()
    .filter(|&(held, _)| held)
    .fold(Modifiers::empty(), |set, (_, flag)| set | flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(f: impl FnOnce(&mut SctkModifiers)) -> SctkModifiers {
        let mut s = SctkModifiers::default();
        f(&mut s);
        s
    }

    #[test]
    fn each_modifier_maps_to_its_flag() {
        let cases: Vec<(&str, SctkModifiers, Modifiers)> = vec![
            ("none", state(|_| {}), Modifiers::empty()),
            ("ctrl", state(|s| s.ctrl = true), Modifiers::CONTROL),
            ("alt", state(|s| s.alt = true), Modifiers::ALT),
            ("shift", state(|s| s.shift = true), Modifiers::SHIFT),
            ("logo is meta", state(|s| s.logo = true), Modifiers::META),
            ("caps", state(|s| s.caps_lock = true), Modifiers::CAPS_LOCK),
            ("num", state(|s| s.num_lock = true), Modifiers::NUM_LOCK),
            (
                "ctrl+shift+logo",
                state(|s| {
                    s.ctrl = true;
                    s.shift = true;
                    s.logo = true;
                }),
                Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::META,
            ),
        ];
        for (name, input, want) in cases {
            assert_eq!(modifiers_from(input), want, "{name}");
        }
    }
}
