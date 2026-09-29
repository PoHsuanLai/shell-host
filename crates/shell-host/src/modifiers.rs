//! The modifier keys held by the seat's keyboard, published to whichever surface has keyboard
//! focus (`use_modifiers`). Distinct from `input::modifiers::modifiers_from`, which
//! feeds Blitz's own key events their `keyboard_types::Modifiers` bitflags unchanged: this is the
//! typed, no-`bool` shape the rest of the host's public surface uses.

use smithay_client_toolkit::seat::keyboard::Modifiers as SctkModifiers;

/// Whether a modifier key is held down right now (pressed or latched).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Modifier {
    #[default]
    Released,
    Held,
}

/// The wire form: sctk reports a held modifier as a bare `bool`.
impl From<bool> for Modifier {
    fn from(wire: bool) -> Modifier {
        match wire {
            true => Modifier::Held,
            false => Modifier::Released,
        }
    }
}

/// Whether a lock key's toggle is engaged. Distinct from [`Modifier`]: Caps Lock and Num Lock
/// are xkb "lock" modifiers, switched by a tap of the key rather than held for the duration of
/// an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LockKey {
    #[default]
    Unlocked,
    Locked,
}

/// The wire form: sctk reports an engaged lock as a bare `bool`.
impl From<bool> for LockKey {
    fn from(wire: bool) -> LockKey {
        match wire {
            true => LockKey::Locked,
            false => LockKey::Unlocked,
        }
    }
}

/// The keyboard's modifiers, as of the focused surface's last `wl_keyboard.modifiers` (the
/// compositor sends one right after `wl_keyboard.enter`, and again whenever the state changes
/// with no key event, such as a bare Shift press). Empty for a surface that has never had
/// keyboard focus, and reset to empty when it loses focus (`use_modifiers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: Modifier,
    pub ctrl: Modifier,
    pub alt: Modifier,
    pub logo: Modifier,
    pub caps_lock: LockKey,
    pub num_lock: LockKey,
}

impl Modifiers {
    /// From sctk's decoded modifier state: already resolved against the keymap, so `shift`,
    /// `ctrl`, `alt` and `logo` are the currently-held (depressed or latched) modifiers, and
    /// `caps_lock`/`num_lock` are the currently-locked ones.
    pub fn from_sctk(state: SctkModifiers) -> Modifiers {
        Modifiers {
            shift: Modifier::from(state.shift),
            ctrl: Modifier::from(state.ctrl),
            alt: Modifier::from(state.alt),
            logo: Modifier::from(state.logo),
            caps_lock: LockKey::from(state.caps_lock),
            num_lock: LockKey::from(state.num_lock),
        }
    }
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
    fn each_field_maps_to_its_own_enum() {
        let cases: Vec<(&str, SctkModifiers, Modifiers)> = vec![
            ("none", state(|_| {}), Modifiers::default()),
            (
                "shift held",
                state(|s| s.shift = true),
                Modifiers {
                    shift: Modifier::Held,
                    ..Modifiers::default()
                },
            ),
            (
                "ctrl held",
                state(|s| s.ctrl = true),
                Modifiers {
                    ctrl: Modifier::Held,
                    ..Modifiers::default()
                },
            ),
            (
                "alt held",
                state(|s| s.alt = true),
                Modifiers {
                    alt: Modifier::Held,
                    ..Modifiers::default()
                },
            ),
            (
                "logo held",
                state(|s| s.logo = true),
                Modifiers {
                    logo: Modifier::Held,
                    ..Modifiers::default()
                },
            ),
            (
                "num lock locked",
                state(|s| s.num_lock = true),
                Modifiers {
                    num_lock: LockKey::Locked,
                    ..Modifiers::default()
                },
            ),
            (
                "everything at once",
                state(|s| {
                    s.ctrl = true;
                    s.shift = true;
                    s.logo = true;
                    s.caps_lock = true;
                }),
                Modifiers {
                    shift: Modifier::Held,
                    ctrl: Modifier::Held,
                    logo: Modifier::Held,
                    caps_lock: LockKey::Locked,
                    ..Modifiers::default()
                },
            ),
        ];
        for (name, input, want) in cases {
            assert_eq!(Modifiers::from_sctk(input), want, "{name}");
        }
    }

    /// The distinction the two enums exist for: a plain modifier held down (Ctrl) is `Modifier`,
    /// a lock key toggled on (Caps Lock) is `LockKey`, and the two are not the same field even
    /// though sctk reports both as a bare `bool`.
    #[test]
    fn locked_caps_is_not_a_held_modifier() {
        let held_ctrl = Modifiers::from_sctk(state(|s| s.ctrl = true));
        assert_eq!(held_ctrl.ctrl, Modifier::Held);
        assert_eq!(held_ctrl.caps_lock, LockKey::Unlocked);

        let locked_caps = Modifiers::from_sctk(state(|s| s.caps_lock = true));
        assert_eq!(locked_caps.caps_lock, LockKey::Locked);
        assert_eq!(locked_caps.ctrl, Modifier::Released);
    }
}
