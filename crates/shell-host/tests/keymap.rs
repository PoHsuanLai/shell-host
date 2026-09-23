//! Keyboard events end to end through `input::key::key_event`: keysym, evdev code and
//! modifiers combined into one `BlitzKeyEvent`. The per-table rows are unit-tested in
//! `src/input/{keysym,keycode,modifiers}.rs`; these check the combination. H2 input fills them.

/// `a` with Shift held: key `Character("A")`, code `KeyA`, modifiers `SHIFT`, text `A`.
#[test]
#[ignore = "H2 fill"]
fn shift_a_is_a_capital_character_on_key_a() {
    todo!()
}

/// Ctrl+C: key `Character("c")` (not U+0003), code `KeyC`, modifiers `CONTROL`, no text.
#[test]
#[ignore = "H2 fill"]
fn ctrl_c_is_the_letter_not_the_control_character() {
    todo!()
}

/// The Super key: key `Meta`, code `MetaLeft`, location `Left`, and `META` set on the next key.
#[test]
#[ignore = "H2 fill"]
fn super_is_meta_everywhere() {
    todo!()
}

/// Keypad Enter: key `Enter`, code `NumpadEnter`, location `Numpad`.
#[test]
#[ignore = "H2 fill"]
fn keypad_enter_is_enter_on_the_numpad() {
    todo!()
}

/// A repeated press sets `is_auto_repeating`; a release has no text.
#[test]
#[ignore = "H2 fill"]
fn repeats_and_releases() {
    todo!()
}
