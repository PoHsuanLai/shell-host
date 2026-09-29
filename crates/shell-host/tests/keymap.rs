//! Keyboard events end to end through `input::key::key_event`: keysym, evdev code and
//! modifiers combined into one `BlitzKeyEvent`. The per-table rows are unit-tested in
//! `src/input/{keysym,keycode,modifiers}.rs`; these check the combination.

use keyboard_types::{Code, Key, Location, Modifiers};
use shell_host::input::key::{Composing, Repeat, key_event};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

fn press(raw_code: u32, keysym: Keysym, utf8: Option<&str>) -> KeyEvent {
    KeyEvent {
        time: 0,
        raw_code,
        keysym,
        utf8: utf8.map(str::to_owned),
    }
}

fn release(raw_code: u32, keysym: Keysym) -> KeyEvent {
    press(raw_code, keysym, None)
}

/// `a` with Shift held: key `Character("A")`, code `KeyA`, modifiers `SHIFT`, text `A`.
#[test]
fn shift_a_is_a_capital_character_on_key_a() {
    let event = press(30, Keysym::A, Some("A"));
    let got = key_event(
        &event,
        Modifiers::SHIFT,
        blitz_traits::events::KeyState::Pressed,
        Repeat::First,
        Composing::Idle,
    );
    assert_eq!(got.key, Key::Character("A".into()));
    assert_eq!(got.code, Code::KeyA);
    assert_eq!(got.modifiers, Modifiers::SHIFT);
    assert_eq!(got.location, Location::Standard);
    assert_eq!(got.text.as_deref(), Some("A"));
}

/// Ctrl+C: key `Character("c")` (not U+0003), code `KeyC`, modifiers `CONTROL`, no text.
/// `Keysym::c` (lowercase) is the unshifted key that xkb reports for a Ctrl+C chord;
/// `Keysym::C` is what Shift also held would report.
#[test]
fn ctrl_c_is_the_letter_not_the_control_character() {
    let event = press(46, Keysym::c, Some("\u{3}"));
    let got = key_event(
        &event,
        Modifiers::CONTROL,
        blitz_traits::events::KeyState::Pressed,
        Repeat::First,
        Composing::Idle,
    );
    assert_eq!(got.key, Key::Character("c".into()));
    assert_eq!(got.code, Code::KeyC);
    assert_eq!(got.modifiers, Modifiers::CONTROL);
    assert_eq!(got.text, None);
}

/// The Super key: key `Meta`, code `MetaLeft`, location `Left`, and `META` set on the next key.
#[test]
fn super_is_meta_everywhere() {
    let super_l = press(125, Keysym::Super_L, None);
    let got = key_event(
        &super_l,
        Modifiers::empty(),
        blitz_traits::events::KeyState::Pressed,
        Repeat::First,
        Composing::Idle,
    );
    assert_eq!(got.key, Key::Meta);
    assert_eq!(got.code, Code::MetaLeft);
    assert_eq!(got.location, Location::Left);

    // Meta does not shift letter case the way Shift does.
    let tab = press(15, Keysym::Tab, None);
    let with_meta_held = key_event(
        &tab,
        Modifiers::META,
        blitz_traits::events::KeyState::Pressed,
        Repeat::First,
        Composing::Idle,
    );
    assert_eq!(with_meta_held.key, Key::Tab);
    assert_eq!(with_meta_held.modifiers, Modifiers::META);
}

/// Keypad Enter: key `Enter`, code `NumpadEnter`, location `Numpad`.
#[test]
fn keypad_enter_is_enter_on_the_numpad() {
    let event = press(96, Keysym::KP_Enter, Some("\r"));
    let got = key_event(
        &event,
        Modifiers::empty(),
        blitz_traits::events::KeyState::Pressed,
        Repeat::First,
        Composing::Idle,
    );
    assert_eq!(got.key, Key::Enter);
    assert_eq!(got.code, Code::NumpadEnter);
    assert_eq!(got.location, Location::Numpad);
}

/// A repeated press sets `is_auto_repeating`; a release has no text.
#[test]
fn repeats_and_releases() {
    let event = press(30, Keysym::A, Some("a"));
    let repeated = key_event(
        &event,
        Modifiers::empty(),
        blitz_traits::events::KeyState::Pressed,
        Repeat::Repeated,
        Composing::Idle,
    );
    assert!(repeated.is_auto_repeating);
    assert_eq!(repeated.text.as_deref(), Some("a"));

    let up = release(30, Keysym::A);
    let released = key_event(
        &up,
        Modifiers::empty(),
        blitz_traits::events::KeyState::Released,
        Repeat::First,
        Composing::Idle,
    );
    assert!(!released.is_auto_repeating);
    assert_eq!(released.text, None);
    assert_eq!(released.state, blitz_traits::events::KeyState::Released);
}

/// Every row group: a `(name, raw_code, keysym, utf8, want_key, want_code, want_location)`
/// table spanning letters, digits, function keys, navigation, modifiers on both sides, the
/// numpad and space (which keyboard-types has no named key for).
struct Case {
    name: &'static str,
    raw_code: u32,
    keysym: Keysym,
    utf8: Option<&'static str>,
    want_key: Key,
    want_code: Code,
    want_location: Location,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "digit 1",
            raw_code: 2,
            keysym: Keysym::_1,
            utf8: Some("1"),
            want_key: Key::Character("1".to_owned()),
            want_code: Code::Digit1,
            want_location: Location::Standard,
        },
        Case {
            name: "escape",
            raw_code: 1,
            keysym: Keysym::Escape,
            utf8: None,
            want_key: Key::Escape,
            want_code: Code::Escape,
            want_location: Location::Standard,
        },
        Case {
            name: "space has no named key",
            raw_code: 57,
            keysym: Keysym::space,
            utf8: Some(" "),
            want_key: Key::Character(" ".to_owned()),
            want_code: Code::Space,
            want_location: Location::Standard,
        },
        Case {
            name: "F5",
            raw_code: 63,
            keysym: Keysym::F5,
            utf8: None,
            want_key: Key::F5,
            want_code: Code::F5,
            want_location: Location::Standard,
        },
        Case {
            name: "Home",
            raw_code: 102,
            keysym: Keysym::Home,
            utf8: None,
            want_key: Key::Home,
            want_code: Code::Home,
            want_location: Location::Standard,
        },
        Case {
            name: "left arrow",
            raw_code: 105,
            keysym: Keysym::Left,
            utf8: None,
            want_key: Key::ArrowLeft,
            want_code: Code::ArrowLeft,
            want_location: Location::Standard,
        },
        Case {
            name: "left shift",
            raw_code: 42,
            keysym: Keysym::Shift_L,
            utf8: None,
            want_key: Key::Shift,
            want_code: Code::ShiftLeft,
            want_location: Location::Left,
        },
        Case {
            name: "right shift",
            raw_code: 54,
            keysym: Keysym::Shift_R,
            utf8: None,
            want_key: Key::Shift,
            want_code: Code::ShiftRight,
            want_location: Location::Right,
        },
        Case {
            name: "left control",
            raw_code: 29,
            keysym: Keysym::Control_L,
            utf8: None,
            want_key: Key::Control,
            want_code: Code::ControlLeft,
            want_location: Location::Left,
        },
        Case {
            name: "left alt",
            raw_code: 56,
            keysym: Keysym::Alt_L,
            utf8: None,
            want_key: Key::Alt,
            want_code: Code::AltLeft,
            want_location: Location::Left,
        },
        Case {
            name: "caps lock",
            raw_code: 58,
            keysym: Keysym::Caps_Lock,
            utf8: None,
            want_key: Key::CapsLock,
            want_code: Code::CapsLock,
            want_location: Location::Standard,
        },
        Case {
            name: "numpad 0",
            raw_code: 82,
            keysym: Keysym::KP_0,
            utf8: Some("0"),
            want_key: Key::Character("0".to_owned()),
            want_code: Code::Numpad0,
            want_location: Location::Numpad,
        },
    ]
}

#[test]
fn every_case_maps_key_code_and_location() {
    for case in cases() {
        let event = press(case.raw_code, case.keysym, case.utf8);
        let got = key_event(
            &event,
            Modifiers::empty(),
            blitz_traits::events::KeyState::Pressed,
            Repeat::First,
            Composing::Idle,
        );
        assert_eq!(got.key, case.want_key, "{}: key", case.name);
        assert_eq!(got.code, case.want_code, "{}: code", case.name);
        assert_eq!(got.location, case.want_location, "{}: location", case.name);
    }
}
