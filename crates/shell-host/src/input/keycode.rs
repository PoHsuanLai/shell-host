//! evdev scancode -> `keyboard_types::Code`: the physical key, whatever the layout.
//!
//! `wl_keyboard.key` carries the Linux evdev code (xkb keycode minus 8), which is what sctk's
//! `KeyEvent::raw_code` holds. The table is adapted from winit 0.30.13,
//! `src/platform_impl/linux/common/xkb/keymap.rs` (`scancode_to_physicalkey`), Copyright the
//! winit contributors, licensed under the Apache License 2.0
//! (https://www.apache.org/licenses/LICENSE-2.0). Changes from the original: it targets
//! keyboard-types 0.7's `Code`; the Super keys (125, 126) are `MetaLeft`/`MetaRight`, the W3C
//! names; codes winit leaves unidentified are `Code::Unidentified`; commented-out rows are
//! omitted. The numbers are the kernel's `linux/input-event-codes.h`. FINDINGS.md has the
//! licence note.

use keyboard_types::Code;

/// The physical key for evdev code `scancode`.
pub(crate) fn code_from_evdev(scancode: u32) -> Code {
    match scancode {
        1 => Code::Escape,
        2 => Code::Digit1,
        3 => Code::Digit2,
        4 => Code::Digit3,
        5 => Code::Digit4,
        6 => Code::Digit5,
        7 => Code::Digit6,
        8 => Code::Digit7,
        9 => Code::Digit8,
        10 => Code::Digit9,
        11 => Code::Digit0,
        12 => Code::Minus,
        13 => Code::Equal,
        14 => Code::Backspace,
        15 => Code::Tab,
        16 => Code::KeyQ,
        17 => Code::KeyW,
        18 => Code::KeyE,
        19 => Code::KeyR,
        20 => Code::KeyT,
        21 => Code::KeyY,
        22 => Code::KeyU,
        23 => Code::KeyI,
        24 => Code::KeyO,
        25 => Code::KeyP,
        26 => Code::BracketLeft,
        27 => Code::BracketRight,
        28 => Code::Enter,
        29 => Code::ControlLeft,
        30 => Code::KeyA,
        31 => Code::KeyS,
        32 => Code::KeyD,
        33 => Code::KeyF,
        34 => Code::KeyG,
        35 => Code::KeyH,
        36 => Code::KeyJ,
        37 => Code::KeyK,
        38 => Code::KeyL,
        39 => Code::Semicolon,
        40 => Code::Quote,
        41 => Code::Backquote,
        42 => Code::ShiftLeft,
        43 => Code::Backslash,
        44 => Code::KeyZ,
        45 => Code::KeyX,
        46 => Code::KeyC,
        47 => Code::KeyV,
        48 => Code::KeyB,
        49 => Code::KeyN,
        50 => Code::KeyM,
        51 => Code::Comma,
        52 => Code::Period,
        53 => Code::Slash,
        54 => Code::ShiftRight,
        55 => Code::NumpadMultiply,
        56 => Code::AltLeft,
        57 => Code::Space,
        58 => Code::CapsLock,
        59 => Code::F1,
        60 => Code::F2,
        61 => Code::F3,
        62 => Code::F4,
        63 => Code::F5,
        64 => Code::F6,
        65 => Code::F7,
        66 => Code::F8,
        67 => Code::F9,
        68 => Code::F10,
        69 => Code::NumLock,
        70 => Code::ScrollLock,
        71 => Code::Numpad7,
        72 => Code::Numpad8,
        73 => Code::Numpad9,
        74 => Code::NumpadSubtract,
        75 => Code::Numpad4,
        76 => Code::Numpad5,
        77 => Code::Numpad6,
        78 => Code::NumpadAdd,
        79 => Code::Numpad1,
        80 => Code::Numpad2,
        81 => Code::Numpad3,
        82 => Code::Numpad0,
        83 => Code::NumpadDecimal,
        85 => Code::Lang5,
        86 => Code::IntlBackslash,
        87 => Code::F11,
        88 => Code::F12,
        89 => Code::IntlRo,
        90 => Code::Lang3,
        91 => Code::Lang4,
        92 => Code::Convert,
        93 => Code::KanaMode,
        94 => Code::NonConvert,
        96 => Code::NumpadEnter,
        97 => Code::ControlRight,
        98 => Code::NumpadDivide,
        99 => Code::PrintScreen,
        100 => Code::AltRight,
        102 => Code::Home,
        103 => Code::ArrowUp,
        104 => Code::PageUp,
        105 => Code::ArrowLeft,
        106 => Code::ArrowRight,
        107 => Code::End,
        108 => Code::ArrowDown,
        109 => Code::PageDown,
        110 => Code::Insert,
        111 => Code::Delete,
        113 => Code::AudioVolumeMute,
        114 => Code::AudioVolumeDown,
        115 => Code::AudioVolumeUp,
        117 => Code::NumpadEqual,
        119 => Code::Pause,
        121 => Code::NumpadComma,
        122 => Code::Lang1,
        123 => Code::Lang2,
        124 => Code::IntlYen,
        125 => Code::MetaLeft,
        126 => Code::MetaRight,
        127 => Code::ContextMenu,
        163 => Code::MediaTrackNext,
        164 => Code::MediaPlayPause,
        165 => Code::MediaTrackPrevious,
        166 => Code::MediaStop,
        183 => Code::F13,
        184 => Code::F14,
        185 => Code::F15,
        186 => Code::F16,
        187 => Code::F17,
        188 => Code::F18,
        189 => Code::F19,
        190 => Code::F20,
        191 => Code::F21,
        192 => Code::F22,
        193 => Code::F23,
        194 => Code::F24,
        _ => Code::Unidentified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(group: &str, rows: &[(u32, Code)]) {
        for (scancode, want) in rows {
            assert_eq!(code_from_evdev(*scancode), *want, "{group}: {scancode}");
        }
    }

    #[test]
    fn digits_and_letters() {
        check(
            "digits",
            &[(2, Code::Digit1), (10, Code::Digit9), (11, Code::Digit0)],
        );
        check(
            "letters",
            &[
                (16, Code::KeyQ),
                (25, Code::KeyP),
                (30, Code::KeyA),
                (38, Code::KeyL),
                (44, Code::KeyZ),
                (50, Code::KeyM),
            ],
        );
    }

    #[test]
    fn modifiers_and_super_is_meta() {
        check(
            "modifiers",
            &[
                (29, Code::ControlLeft),
                (97, Code::ControlRight),
                (42, Code::ShiftLeft),
                (54, Code::ShiftRight),
                (56, Code::AltLeft),
                (100, Code::AltRight),
                (125, Code::MetaLeft),
                (126, Code::MetaRight),
                (58, Code::CapsLock),
            ],
        );
    }

    #[test]
    fn navigation_and_editing() {
        check(
            "navigation",
            &[
                (102, Code::Home),
                (103, Code::ArrowUp),
                (105, Code::ArrowLeft),
                (106, Code::ArrowRight),
                (108, Code::ArrowDown),
                (104, Code::PageUp),
                (109, Code::PageDown),
                (107, Code::End),
                (110, Code::Insert),
                (111, Code::Delete),
                (14, Code::Backspace),
                (15, Code::Tab),
                (28, Code::Enter),
                (57, Code::Space),
                (1, Code::Escape),
            ],
        );
    }

    #[test]
    fn keypad_function_and_media() {
        check(
            "keypad",
            &[
                (71, Code::Numpad7),
                (82, Code::Numpad0),
                (83, Code::NumpadDecimal),
                (96, Code::NumpadEnter),
                (98, Code::NumpadDivide),
                (117, Code::NumpadEqual),
            ],
        );
        check(
            "function",
            &[
                (59, Code::F1),
                (68, Code::F10),
                (87, Code::F11),
                (88, Code::F12),
                (194, Code::F24),
            ],
        );
        check(
            "media",
            &[
                (113, Code::AudioVolumeMute),
                (115, Code::AudioVolumeUp),
                (164, Code::MediaPlayPause),
                (166, Code::MediaStop),
            ],
        );
    }

    #[test]
    fn international_and_unknown() {
        check(
            "intl",
            &[
                (86, Code::IntlBackslash),
                (89, Code::IntlRo),
                (124, Code::IntlYen),
                (122, Code::Lang1),
            ],
        );
        check(
            "unknown",
            &[
                (0, Code::Unidentified),
                (84, Code::Unidentified),
                (240, Code::Unidentified),
                (999, Code::Unidentified),
            ],
        );
    }
}
