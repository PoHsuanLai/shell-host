//! IME over `zwp_text_input_v3`. The protocol is double-buffered: `preedit_string`,
//! `commit_string` and `delete_surrounding_text` accumulate until `done`, which applies them in
//! the protocol's order (delete, commit, preedit). Only then are Blitz IME events produced.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use blitz_traits::events::BlitzImeEvent;

use crate::region::Rect;

/// A preedit string with its cursor, as byte offsets into the string (`None` hides the cursor).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

/// How much text around the cursor to delete, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeleteSurrounding {
    pub before: u32,
    pub after: u32,
}

/// The state accumulated since the last `done`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PendingIme {
    pub preedit: Option<Preedit>,
    pub commit: Option<String>,
    pub delete: Option<DeleteSurrounding>,
}

/// Whether text input is enabled for the focused surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeFocus {
    Disabled,
    /// Enabled, with the caret rectangle last sent (`set_cursor_rectangle`).
    Enabled {
        cursor_area: Rect,
    },
}

/// One seat's text-input state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ime {
    pub pending: PendingIme,
    pub focus: ImeFocus,
    /// The `done` serial the compositor expects our next `commit` to match.
    pub serial: u32,
}

/// Apply `done`: the Blitz events for everything pending, in protocol order, and an empty
/// pending state. Pure.
pub fn on_done(pending: PendingIme) -> (Vec<BlitzImeEvent>, PendingIme) {
    todo!()
}
