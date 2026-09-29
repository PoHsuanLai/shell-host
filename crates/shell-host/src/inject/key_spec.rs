//! The `key` verb's spelling of a keystroke: `<name>[+mod[+mod]]`, where `name` is a W3C key
//! value (`Tab`, `Enter`, `Escape`, `ArrowDown`, `F5`, `a`, `Shift`) or `Space`, and a `mod` is
//! `shift`, `ctrl`, `alt` or `super`. `+` splits, so it cannot itself be a key name.

use std::str::FromStr;

use keyboard_types::{Code, Key, Modifiers};

use super::KeyStroke;
use super::command::CommandError;

/// The keystroke `spec` names (`Tab`, `k+ctrl`, `Tab+shift`): the `key` verb's spelling.
pub fn parse_key(spec: &str) -> Result<KeyStroke, CommandError> {
    let mut parts = spec.split('+');
    let name = parts.next().unwrap_or_default();
    let key = key_named(name).ok_or_else(|| CommandError::NotAKey(name.to_owned()))?;
    let code = code_of(&key).ok_or_else(|| CommandError::NotAKey(name.to_owned()))?;
    let mods = parts.try_fold(Modifiers::empty(), |held, word| {
        modifier(word)
            .map(|one| held | one)
            .ok_or_else(|| CommandError::NotAModifier(word.to_owned()))
    })?;
    Ok(KeyStroke { key, code, mods })
}

fn key_named(name: &str) -> Option<Key> {
    match name {
        "Space" => Some(Key::Character(" ".to_owned())),
        _ => Key::from_str(name).ok(),
    }
}

fn modifier(word: &str) -> Option<Modifiers> {
    match word {
        "shift" => Some(Modifiers::SHIFT),
        "ctrl" => Some(Modifiers::CONTROL),
        "alt" => Some(Modifiers::ALT),
        "super" => Some(Modifiers::SUPER),
        _ => None,
    }
}

/// The physical key that types `key` on a US layout: a named key's code has its name, a
/// character is a letter, digit or punctuation key, a modifier is its left-hand key.
fn code_of(key: &Key) -> Option<Code> {
    match key {
        Key::Character(text) => text
            .chars()
            .next()
            .filter(|_| text.chars().count() == 1)
            .and_then(code_of_char),
        Key::Shift => Some(Code::ShiftLeft),
        Key::Control => Some(Code::ControlLeft),
        Key::Alt => Some(Code::AltLeft),
        Key::Meta | Key::Super => Some(Code::MetaLeft),
        named => Code::from_str(&named.to_string()).ok(),
    }
}

fn code_of_char(c: char) -> Option<Code> {
    let name = match c {
        'a'..='z' => format!("Key{}", c.to_ascii_uppercase()),
        'A'..='Z' => format!("Key{c}"),
        '0'..='9' => format!("Digit{c}"),
        ' ' => "Space".to_owned(),
        _ => {
            return PUNCTUATION
                .iter()
                .find(|(ch, _)| *ch == c)
                .map(|(_, code)| *code);
        }
    };
    Code::from_str(&name).ok()
}

const PUNCTUATION: &[(char, Code)] = &[
    ('-', Code::Minus),
    ('=', Code::Equal),
    ('[', Code::BracketLeft),
    (']', Code::BracketRight),
    ('\\', Code::Backslash),
    (';', Code::Semicolon),
    ('\'', Code::Quote),
    ('`', Code::Backquote),
    (',', Code::Comma),
    ('.', Code::Period),
    ('/', Code::Slash),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: Key, code: Code, mods: Modifiers) -> KeyStroke {
        KeyStroke { key, code, mods }
    }

    #[test]
    fn a_spec_names_key_code_and_modifiers() {
        let ch = |c: &str| Key::Character(c.to_owned());
        let cases: Vec<(&str, Result<KeyStroke, CommandError>)> = vec![
            ("Tab", Ok(stroke(Key::Tab, Code::Tab, Modifiers::empty()))),
            (
                "Enter",
                Ok(stroke(Key::Enter, Code::Enter, Modifiers::empty())),
            ),
            (
                "ArrowDown",
                Ok(stroke(Key::ArrowDown, Code::ArrowDown, Modifiers::empty())),
            ),
            (
                "Home",
                Ok(stroke(Key::Home, Code::Home, Modifiers::empty())),
            ),
            ("F5", Ok(stroke(Key::F5, Code::F5, Modifiers::empty()))),
            ("a", Ok(stroke(ch("a"), Code::KeyA, Modifiers::empty()))),
            ("7", Ok(stroke(ch("7"), Code::Digit7, Modifiers::empty()))),
            (
                "Space",
                Ok(stroke(ch(" "), Code::Space, Modifiers::empty())),
            ),
            (",", Ok(stroke(ch(","), Code::Comma, Modifiers::empty()))),
            (
                "Shift",
                Ok(stroke(Key::Shift, Code::ShiftLeft, Modifiers::empty())),
            ),
            (
                "k+ctrl",
                Ok(stroke(ch("k"), Code::KeyK, Modifiers::CONTROL)),
            ),
            (
                "Tab+shift",
                Ok(stroke(Key::Tab, Code::Tab, Modifiers::SHIFT)),
            ),
            (
                "y+super+alt",
                Ok(stroke(
                    ch("y"),
                    Code::KeyY,
                    Modifiers::SUPER | Modifiers::ALT,
                )),
            ),
            ("Tabb", Err(CommandError::NotAKey("Tabb".into()))),
            ("", Err(CommandError::NotAKey(String::new()))),
            ("Tab+hyper", Err(CommandError::NotAModifier("hyper".into()))),
        ];
        for (spec, want) in cases {
            assert_eq!(parse_key(spec), want, "{spec:?}");
        }
    }
}
