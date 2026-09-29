//! xkb keysym -> `keyboard_types::Key`: the logical key a press means under the current layout.
//!
//! The table is adapted from winit 0.30.13, `src/platform_impl/linux/common/xkb/keymap.rs`
//! (`keysym_to_key`, `keysym_location`), Copyright the winit contributors, licensed under the
//! Apache License 2.0 (https://www.apache.org/licenses/LICENSE-2.0). Changes from the original:
//! it targets keyboard-types 0.7's `Key`/`Location` instead of winit's `NamedKey`/`KeyLocation`;
//! the Super keys map to `Key::Meta` (the W3C name for the Windows/Command key, and what
//! `Modifiers::META` means) instead of winit's `Super`; `space` is `Key::Character(" ")`,
//! because keyboard-types has no named Space; commented-out rows of the original are omitted.
//! FINDINGS.md has the licence note.

use keyboard_types::{Key, Location};
use smithay_client_toolkit::seat::keyboard::Keysym;

/// The named key for `keysym`, or `None` when the keysym stands for text (or for nothing we
/// name).
pub(crate) fn named_key(keysym: Keysym) -> Option<Key> {
    let key = match keysym {
        // TTY function keys
        Keysym::BackSpace => Key::Backspace,
        Keysym::Tab => Key::Tab,
        Keysym::Clear => Key::Clear,
        Keysym::Return => Key::Enter,
        Keysym::Pause => Key::Pause,
        Keysym::Scroll_Lock => Key::ScrollLock,
        Keysym::Sys_Req => Key::PrintScreen,
        Keysym::Escape => Key::Escape,
        Keysym::Delete => Key::Delete,

        // IME keys
        Keysym::Multi_key => Key::Compose,
        Keysym::Codeinput => Key::CodeInput,
        Keysym::SingleCandidate => Key::SingleCandidate,
        Keysym::MultipleCandidate => Key::AllCandidates,
        Keysym::PreviousCandidate => Key::PreviousCandidate,

        // Japanese keys
        Keysym::Kanji => Key::KanjiMode,
        Keysym::Muhenkan => Key::NonConvert,
        Keysym::Henkan_Mode => Key::Convert,
        Keysym::Romaji => Key::Romaji,
        Keysym::Hiragana => Key::Hiragana,
        Keysym::Hiragana_Katakana => Key::HiraganaKatakana,
        Keysym::Zenkaku => Key::Zenkaku,
        Keysym::Hankaku => Key::Hankaku,
        Keysym::Zenkaku_Hankaku => Key::ZenkakuHankaku,
        Keysym::Kana_Lock => Key::KanaMode,
        Keysym::Kana_Shift => Key::KanaMode,
        Keysym::Eisu_Shift => Key::Alphanumeric,
        Keysym::Eisu_toggle => Key::Alphanumeric,

        // Cursor control & motion
        Keysym::Home => Key::Home,
        Keysym::Left => Key::ArrowLeft,
        Keysym::Up => Key::ArrowUp,
        Keysym::Right => Key::ArrowRight,
        Keysym::Down => Key::ArrowDown,
        Keysym::Page_Up => Key::PageUp,
        Keysym::Page_Down => Key::PageDown,
        Keysym::End => Key::End,

        // Misc. functions
        Keysym::Select => Key::Select,
        Keysym::Print => Key::PrintScreen,
        Keysym::Execute => Key::Execute,
        Keysym::Insert => Key::Insert,
        Keysym::Undo => Key::Undo,
        Keysym::Redo => Key::Redo,
        Keysym::Menu => Key::ContextMenu,
        Keysym::Find => Key::Find,
        Keysym::Cancel => Key::Cancel,
        Keysym::Help => Key::Help,
        Keysym::Break => Key::Pause,
        Keysym::Mode_switch => Key::ModeChange,
        Keysym::Num_Lock => Key::NumLock,

        // Keypad keys
        Keysym::KP_Tab => Key::Tab,
        Keysym::KP_Enter => Key::Enter,
        Keysym::KP_F1 => Key::F1,
        Keysym::KP_F2 => Key::F2,
        Keysym::KP_F3 => Key::F3,
        Keysym::KP_F4 => Key::F4,
        Keysym::KP_Home => Key::Home,
        Keysym::KP_Left => Key::ArrowLeft,
        Keysym::KP_Up => Key::ArrowUp,
        Keysym::KP_Right => Key::ArrowRight,
        Keysym::KP_Down => Key::ArrowDown,
        Keysym::KP_Page_Up => Key::PageUp,
        Keysym::KP_Page_Down => Key::PageDown,
        Keysym::KP_End => Key::End,
        Keysym::KP_Insert => Key::Insert,
        Keysym::KP_Delete => Key::Delete,

        // Function keys
        Keysym::F1 => Key::F1,
        Keysym::F2 => Key::F2,
        Keysym::F3 => Key::F3,
        Keysym::F4 => Key::F4,
        Keysym::F5 => Key::F5,
        Keysym::F6 => Key::F6,
        Keysym::F7 => Key::F7,
        Keysym::F8 => Key::F8,
        Keysym::F9 => Key::F9,
        Keysym::F10 => Key::F10,
        Keysym::F11 => Key::F11,
        Keysym::F12 => Key::F12,
        Keysym::F13 => Key::F13,
        Keysym::F14 => Key::F14,
        Keysym::F15 => Key::F15,
        Keysym::F16 => Key::F16,
        Keysym::F17 => Key::F17,
        Keysym::F18 => Key::F18,
        Keysym::F19 => Key::F19,
        Keysym::F20 => Key::F20,
        Keysym::F21 => Key::F21,
        Keysym::F22 => Key::F22,
        Keysym::F23 => Key::F23,
        Keysym::F24 => Key::F24,
        Keysym::F25 => Key::F25,
        Keysym::F26 => Key::F26,
        Keysym::F27 => Key::F27,
        Keysym::F28 => Key::F28,
        Keysym::F29 => Key::F29,
        Keysym::F30 => Key::F30,
        Keysym::F31 => Key::F31,
        Keysym::F32 => Key::F32,
        Keysym::F33 => Key::F33,
        Keysym::F34 => Key::F34,
        Keysym::F35 => Key::F35,

        // Modifiers
        Keysym::Shift_L => Key::Shift,
        Keysym::Shift_R => Key::Shift,
        Keysym::Control_L => Key::Control,
        Keysym::Control_R => Key::Control,
        Keysym::Caps_Lock => Key::CapsLock,
        Keysym::Alt_L => Key::Alt,
        Keysym::Alt_R => Key::Alt,
        Keysym::Super_L => Key::Meta,
        Keysym::Super_R => Key::Meta,
        Keysym::Hyper_L => Key::Hyper,
        Keysym::Hyper_R => Key::Hyper,

        // XKB function and modifier keys
        Keysym::ISO_Level3_Shift => Key::AltGraph,
        Keysym::ISO_Level3_Latch => Key::AltGraph,
        Keysym::ISO_Level3_Lock => Key::AltGraph,
        Keysym::ISO_Next_Group => Key::GroupNext,
        Keysym::ISO_Prev_Group => Key::GroupPrevious,
        Keysym::ISO_First_Group => Key::GroupFirst,
        Keysym::ISO_Last_Group => Key::GroupLast,
        Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::ISO_Enter => Key::Enter,

        // 3270 terminal keys
        Keysym::_3270_EraseEOF => Key::EraseEof,
        Keysym::_3270_Attn => Key::Attn,
        Keysym::_3270_Play => Key::Play,
        Keysym::_3270_ExSelect => Key::ExSel,
        Keysym::_3270_CursorSelect => Key::CrSel,
        Keysym::_3270_PrintScreen => Key::PrintScreen,
        Keysym::_3270_Enter => Key::Enter,

        // XFree86: backlight
        Keysym::XF86_MonBrightnessUp => Key::BrightnessUp,
        Keysym::XF86_MonBrightnessDown => Key::BrightnessDown,

        // XFree86: "Internet"
        Keysym::XF86_Standby => Key::Standby,
        Keysym::XF86_AudioLowerVolume => Key::AudioVolumeDown,
        Keysym::XF86_AudioRaiseVolume => Key::AudioVolumeUp,
        Keysym::XF86_AudioPlay => Key::MediaPlay,
        Keysym::XF86_AudioStop => Key::MediaStop,
        Keysym::XF86_AudioPrev => Key::MediaTrackPrevious,
        Keysym::XF86_AudioNext => Key::MediaTrackNext,
        Keysym::XF86_HomePage => Key::BrowserHome,
        Keysym::XF86_Mail => Key::LaunchMail,
        Keysym::XF86_Search => Key::BrowserSearch,
        Keysym::XF86_AudioRecord => Key::MediaRecord,

        // XFree86: PDA
        Keysym::XF86_Calculator => Key::LaunchApplication2,
        Keysym::XF86_Calendar => Key::LaunchCalendar,
        Keysym::XF86_PowerDown => Key::Power,

        // XFree86: more "Internet"
        Keysym::XF86_Back => Key::BrowserBack,
        Keysym::XF86_Forward => Key::BrowserForward,
        Keysym::XF86_Refresh => Key::BrowserRefresh,
        Keysym::XF86_PowerOff => Key::Power,
        Keysym::XF86_WakeUp => Key::WakeUp,
        Keysym::XF86_Eject => Key::Eject,
        Keysym::XF86_ScreenSaver => Key::LaunchScreenSaver,
        Keysym::XF86_WWW => Key::LaunchWebBrowser,
        Keysym::XF86_Sleep => Key::Standby,
        Keysym::XF86_Favorites => Key::BrowserFavorites,
        Keysym::XF86_AudioPause => Key::MediaPause,
        Keysym::XF86_MyComputer => Key::LaunchApplication1,
        Keysym::XF86_AudioRewind => Key::MediaRewind,
        // libxkbcommon's own spelling, kept alongside Calculator.
        Keysym::XF86_Calculater => Key::LaunchApplication2,
        Keysym::XF86_Close => Key::Close,
        Keysym::XF86_Copy => Key::Copy,
        Keysym::XF86_Cut => Key::Cut,
        Keysym::XF86_Excel => Key::LaunchSpreadsheet,
        Keysym::XF86_LogOff => Key::LogOff,
        Keysym::XF86_MySites => Key::BrowserFavorites,
        Keysym::XF86_New => Key::New,
        Keysym::XF86_Open => Key::Open,
        Keysym::XF86_Paste => Key::Paste,
        Keysym::XF86_Phone => Key::LaunchPhone,
        Keysym::XF86_Reply => Key::MailReply,
        Keysym::XF86_Reload => Key::BrowserRefresh,
        Keysym::XF86_Save => Key::Save,
        Keysym::XF86_Send => Key::MailSend,
        Keysym::XF86_Spell => Key::SpellCheck,
        Keysym::XF86_SplitScreen => Key::SplitScreenToggle,
        Keysym::XF86_Video => Key::LaunchMediaPlayer,
        Keysym::XF86_Word => Key::LaunchWordProcessor,
        Keysym::XF86_ZoomIn => Key::ZoomIn,
        Keysym::XF86_ZoomOut => Key::ZoomOut,
        Keysym::XF86_WebCam => Key::LaunchWebCam,
        Keysym::XF86_MailForward => Key::MailForward,
        Keysym::XF86_Music => Key::LaunchMusicPlayer,
        Keysym::XF86_AudioForward => Key::MediaFastForward,
        Keysym::XF86_AudioRandomPlay => Key::RandomToggle,
        Keysym::XF86_Subtitle => Key::Subtitle,
        Keysym::XF86_AudioCycleTrack => Key::MediaAudioTrack,
        Keysym::XF86_Suspend => Key::Standby,
        Keysym::XF86_Hibernate => Key::Hibernate,
        Keysym::XF86_AudioMute => Key::AudioVolumeMute,
        Keysym::XF86_Next_VMode => Key::VideoModeNext,

        // Sun
        Keysym::SUN_Copy => Key::Copy,
        Keysym::SUN_Open => Key::Open,
        Keysym::SUN_Paste => Key::Paste,
        Keysym::SUN_Cut => Key::Cut,
        Keysym::SUN_AudioLowerVolume => Key::AudioVolumeDown,
        Keysym::SUN_AudioMute => Key::AudioVolumeMute,
        Keysym::SUN_AudioRaiseVolume => Key::AudioVolumeUp,
        Keysym::SUN_VideoLowerBrightness => Key::BrightnessDown,
        Keysym::SUN_VideoRaiseBrightness => Key::BrightnessUp,

        _ => return None,
    };
    Some(key)
}

/// The logical key for a press: the named key if there is one, else the text the press
/// produced, else the keysym's own character, else `Unidentified`.
///
/// `text` is xkb's UTF-8 for the press (sctk's `KeyEvent::utf8`). Control characters in it
/// (Ctrl+C yields U+0003) are not text, so they fall through to the keysym's character (`c`),
/// which is what a shortcut matcher wants to see.
pub(crate) fn logical_key(keysym: Keysym, text: Option<&str>) -> Key {
    if let Some(key) = named_key(keysym) {
        return key;
    }
    let printable = text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
    match (printable, keysym.key_char()) {
        (Some(t), _) => Key::Character(t.to_owned()),
        (None, Some(c)) if !c.is_control() => Key::Character(c.to_string()),
        _ => Key::Unidentified,
    }
}

/// Where on the keyboard the key that produced `keysym` sits.
pub(crate) fn location(keysym: Keysym) -> Location {
    match keysym {
        Keysym::Shift_L
        | Keysym::Control_L
        | Keysym::Meta_L
        | Keysym::Alt_L
        | Keysym::Super_L
        | Keysym::Hyper_L => Location::Left,
        Keysym::Shift_R
        | Keysym::Control_R
        | Keysym::Meta_R
        | Keysym::Alt_R
        | Keysym::Super_R
        | Keysym::Hyper_R => Location::Right,
        Keysym::KP_0
        | Keysym::KP_1
        | Keysym::KP_2
        | Keysym::KP_3
        | Keysym::KP_4
        | Keysym::KP_5
        | Keysym::KP_6
        | Keysym::KP_7
        | Keysym::KP_8
        | Keysym::KP_9
        | Keysym::KP_Space
        | Keysym::KP_Tab
        | Keysym::KP_Enter
        | Keysym::KP_F1
        | Keysym::KP_F2
        | Keysym::KP_F3
        | Keysym::KP_F4
        | Keysym::KP_Home
        | Keysym::KP_Left
        | Keysym::KP_Up
        | Keysym::KP_Right
        | Keysym::KP_Down
        | Keysym::KP_Page_Up
        | Keysym::KP_Page_Down
        | Keysym::KP_End
        | Keysym::KP_Begin
        | Keysym::KP_Insert
        | Keysym::KP_Delete
        | Keysym::KP_Equal
        | Keysym::KP_Multiply
        | Keysym::KP_Add
        | Keysym::KP_Separator
        | Keysym::KP_Subtract
        | Keysym::KP_Decimal
        | Keysym::KP_Divide => Location::Numpad,
        _ => Location::Standard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(group: &str, rows: &[(Keysym, Key)]) {
        for (keysym, want) in rows {
            assert_eq!(
                named_key(*keysym).as_ref(),
                Some(want),
                "{group}: {keysym:?}"
            );
        }
    }

    #[test]
    fn tty_and_cursor_keys() {
        check(
            "tty",
            &[
                (Keysym::BackSpace, Key::Backspace),
                (Keysym::Tab, Key::Tab),
                (Keysym::Return, Key::Enter),
                (Keysym::Escape, Key::Escape),
                (Keysym::Delete, Key::Delete),
                (Keysym::Sys_Req, Key::PrintScreen),
            ],
        );
        check(
            "cursor",
            &[
                (Keysym::Home, Key::Home),
                (Keysym::Left, Key::ArrowLeft),
                (Keysym::Up, Key::ArrowUp),
                (Keysym::Right, Key::ArrowRight),
                (Keysym::Down, Key::ArrowDown),
                (Keysym::Page_Up, Key::PageUp),
                (Keysym::Page_Down, Key::PageDown),
                (Keysym::End, Key::End),
            ],
        );
    }

    #[test]
    fn ime_and_japanese_keys() {
        check(
            "ime",
            &[
                (Keysym::Multi_key, Key::Compose),
                (Keysym::MultipleCandidate, Key::AllCandidates),
                (Keysym::Henkan_Mode, Key::Convert),
                (Keysym::Muhenkan, Key::NonConvert),
                (Keysym::Zenkaku_Hankaku, Key::ZenkakuHankaku),
                (Keysym::Eisu_toggle, Key::Alphanumeric),
            ],
        );
    }

    #[test]
    fn misc_keypad_and_function_keys() {
        check(
            "misc",
            &[
                (Keysym::Menu, Key::ContextMenu),
                (Keysym::Break, Key::Pause),
                (Keysym::Mode_switch, Key::ModeChange),
                (Keysym::Num_Lock, Key::NumLock),
            ],
        );
        check(
            "keypad",
            &[
                (Keysym::KP_Enter, Key::Enter),
                (Keysym::KP_Left, Key::ArrowLeft),
                (Keysym::KP_Delete, Key::Delete),
                (Keysym::KP_F4, Key::F4),
            ],
        );
        check(
            "function",
            &[
                (Keysym::F1, Key::F1),
                (Keysym::F12, Key::F12),
                (Keysym::F35, Key::F35),
            ],
        );
    }

    #[test]
    fn modifier_keys_and_super_is_meta() {
        check(
            "modifiers",
            &[
                (Keysym::Shift_L, Key::Shift),
                (Keysym::Control_R, Key::Control),
                (Keysym::Alt_L, Key::Alt),
                (Keysym::Super_L, Key::Meta),
                (Keysym::Super_R, Key::Meta),
                (Keysym::Hyper_L, Key::Hyper),
                (Keysym::ISO_Level3_Shift, Key::AltGraph),
                (Keysym::ISO_Left_Tab, Key::Tab),
                (Keysym::ISO_Next_Group, Key::GroupNext),
            ],
        );
    }

    #[test]
    fn terminal_media_and_vendor_keys() {
        check(
            "3270",
            &[
                (Keysym::_3270_EraseEOF, Key::EraseEof),
                (Keysym::_3270_Enter, Key::Enter),
            ],
        );
        check(
            "xf86",
            &[
                (Keysym::XF86_MonBrightnessUp, Key::BrightnessUp),
                (Keysym::XF86_AudioRaiseVolume, Key::AudioVolumeUp),
                (Keysym::XF86_AudioMute, Key::AudioVolumeMute),
                (Keysym::XF86_AudioPlay, Key::MediaPlay),
                (Keysym::XF86_Calculater, Key::LaunchApplication2),
                (Keysym::XF86_Sleep, Key::Standby),
            ],
        );
        check(
            "sun",
            &[
                (Keysym::SUN_Copy, Key::Copy),
                (Keysym::SUN_VideoRaiseBrightness, Key::BrightnessUp),
            ],
        );
    }

    #[test]
    fn text_keys_fall_back_to_characters() {
        let cases: &[(&str, Keysym, Option<&str>, Key)] = &[
            ("letter", Keysym::a, Some("a"), Key::Character("a".into())),
            ("shifted", Keysym::A, Some("A"), Key::Character("A".into())),
            (
                "space",
                Keysym::space,
                Some(" "),
                Key::Character(" ".into()),
            ),
            (
                "ctrl+c is c",
                Keysym::c,
                Some("\u{3}"),
                Key::Character("c".into()),
            ),
            (
                "release has no text",
                Keysym::x,
                None,
                Key::Character("x".into()),
            ),
            (
                "named wins over text",
                Keysym::Return,
                Some("\r"),
                Key::Enter,
            ),
            ("nothing", Keysym::NoSymbol, None, Key::Unidentified),
        ];
        for (name, keysym, text, want) in cases {
            assert_eq!(logical_key(*keysym, *text), *want, "{name}");
        }
    }

    #[test]
    fn locations() {
        let cases = [
            (Keysym::Shift_L, Location::Left),
            (Keysym::Super_R, Location::Right),
            (Keysym::KP_5, Location::Numpad),
            (Keysym::KP_Enter, Location::Numpad),
            (Keysym::a, Location::Standard),
            (Keysym::Return, Location::Standard),
        ];
        for (keysym, want) in cases {
            assert_eq!(location(keysym), want, "{keysym:?}");
        }
    }
}
