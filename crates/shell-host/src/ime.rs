//! IME over `zwp_text_input_v3`. The protocol is double-buffered: `preedit_string`,
//! `commit_string` and `delete_surrounding_text` accumulate until `done`, which applies them in
//! the protocol's order: remove the current preedit, delete, commit, insert the new preedit
//! (empty unless one was sent). Only then are Blitz IME events produced.

use blitz_traits::events::BlitzImeEvent;

use crate::region::Rect;

mod surrounding;

pub(crate) use surrounding::{ChangeCause, SentSurrounding, decide};

/// A preedit string with its cursor, as byte offsets into the string (`None` hides the cursor).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

/// How much text around the cursor to delete, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DeleteSurrounding {
    pub before: u32,
    pub after: u32,
}

/// The state accumulated since the last `done`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PendingIme {
    pub preedit: Option<Preedit>,
    pub commit: Option<String>,
    pub delete: Option<DeleteSurrounding>,
}

/// Whether text input is enabled for the focused surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImeFocus {
    Disabled,
    /// Enabled, with the caret rectangle last sent (`set_cursor_rectangle`).
    Enabled {
        cursor_area: Rect,
    },
}

/// One seat's text-input state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ime {
    pub pending: PendingIme,
    pub focus: ImeFocus,
    /// The `done` serial the compositor expects our next `commit` to match.
    pub serial: u32,
    /// The surrounding text last sent to the compositor.
    pub surrounding: SentSurrounding,
    /// Why the next change to the field's text or caret happened: `InputMethod` once a `done`
    /// was applied, taken back to `Other` when the surrounding text is next considered.
    pub cause: ChangeCause,
}

/// Apply `done`: the Blitz events for everything pending, in protocol order, and an empty
/// pending state. The current preedit goes first whenever something other than a new preedit
/// is pending (an empty `Preedit`, which Blitz takes as "clear the composition"): Blitz inserts
/// a commit next to a live composition rather than over it, so a commit of the composed text
/// with the composition still shown doubled it. A lone new preedit replaces
/// the old one by itself. Pure.
pub(crate) fn on_done(pending: PendingIme) -> (Vec<BlitzImeEvent>, PendingIme) {
    let mut events = Vec::new();
    if pending.commit.is_some() || pending.delete.is_some() || pending.preedit.is_none() {
        events.push(BlitzImeEvent::Preedit(String::new(), None));
    }
    if let Some(delete) = pending.delete {
        events.push(BlitzImeEvent::DeleteSurrounding {
            before_bytes: delete.before as usize,
            after_bytes: delete.after as usize,
        });
    }
    if let Some(commit) = pending.commit {
        events.push(BlitzImeEvent::Commit(commit));
    }
    if let Some(preedit) = pending.preedit {
        events.push(BlitzImeEvent::Preedit(preedit.text, preedit.cursor));
    }
    (events, PendingIme::default())
}

#[cfg(test)]
mod tests;
