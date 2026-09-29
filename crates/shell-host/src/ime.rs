//! IME over `zwp_text_input_v3`. The protocol is double-buffered: `preedit_string`,
//! `commit_string` and `delete_surrounding_text` accumulate until `done`, which applies them in
//! the protocol's order: remove the current preedit, delete, commit, insert the new preedit
//! (empty unless one was sent). Only then are Blitz IME events produced.

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
/// pending state. The current preedit goes first whenever something other than a new preedit
/// is pending (an empty `Preedit`, which Blitz takes as "clear the composition"): Blitz inserts
/// a commit next to a live composition rather than over it, so a commit of the composed text
/// with the composition still shown doubled it. A lone new preedit replaces
/// the old one by itself. Pure.
pub fn on_done(pending: PendingIme) -> (Vec<BlitzImeEvent>, PendingIme) {
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
mod tests {
    use super::*;

    #[test]
    fn nothing_pending_only_clears_the_preedit() {
        // The protocol resets the preedit on every `done`; one that sends none leaves it empty.
        let (events, rest) = on_done(PendingIme::default());
        assert_eq!(events, vec![clear()]);
        assert_eq!(rest, PendingIme::default());
    }

    fn clear() -> BlitzImeEvent {
        BlitzImeEvent::Preedit(String::new(), None)
    }

    #[test]
    fn a_commit_of_the_composed_text_clears_the_composition_first() {
        // Chewing's `s u 3 c l 3` then Return, as fcitx5 sends it: the preedit 你好 is replaced
        // by nothing and 你好 is committed in one `done`. Committing with the composition still
        // shown doubled it (你好你好).
        let pending = PendingIme {
            preedit: Some(Preedit {
                text: String::new(),
                cursor: Some((0, 0)),
            }),
            commit: Some("你好".to_owned()),
            delete: None,
        };
        let (events, _) = on_done(pending);
        assert_eq!(
            events,
            vec![
                clear(),
                BlitzImeEvent::Commit("你好".to_owned()),
                BlitzImeEvent::Preedit(String::new(), Some((0, 0))),
            ]
        );
    }

    #[test]
    fn a_commit_alone_produces_one_event_and_clears() {
        let pending = PendingIme {
            commit: Some("hello".to_owned()),
            ..PendingIme::default()
        };
        let (events, rest) = on_done(pending);
        assert_eq!(
            events,
            vec![clear(), BlitzImeEvent::Commit("hello".to_owned())]
        );
        assert_eq!(rest, PendingIme::default());
    }

    #[test]
    fn a_preedit_alone_carries_its_cursor() {
        let pending = PendingIme {
            preedit: Some(Preedit {
                text: "ni".to_owned(),
                cursor: Some((0, 2)),
            }),
            ..PendingIme::default()
        };
        let (events, _) = on_done(pending);
        assert_eq!(
            events,
            vec![BlitzImeEvent::Preedit("ni".to_owned(), Some((0, 2)))]
        );
    }

    #[test]
    fn delete_then_commit_then_preedit_in_protocol_order() {
        let pending = PendingIme {
            preedit: Some(Preedit {
                text: "ni".to_owned(),
                cursor: Some((0, 2)),
            }),
            commit: Some("你".to_owned()),
            delete: Some(DeleteSurrounding {
                before: 1,
                after: 0,
            }),
        };
        let (events, rest) = on_done(pending);
        assert_eq!(
            events,
            vec![
                clear(),
                BlitzImeEvent::DeleteSurrounding {
                    before_bytes: 1,
                    after_bytes: 0,
                },
                BlitzImeEvent::Commit("你".to_owned()),
                BlitzImeEvent::Preedit("ni".to_owned(), Some((0, 2))),
            ]
        );
        assert_eq!(rest, PendingIme::default());
    }

    #[test]
    fn an_empty_commit_string_still_produces_a_commit_event() {
        // zwp_text_input_v3's `commit_string("")` is meaningful (clears any pending preedit
        // without inserting text), so an empty string is not the same as "no commit".
        let pending = PendingIme {
            commit: Some(String::new()),
            ..PendingIme::default()
        };
        let (events, _) = on_done(pending);
        assert_eq!(events, vec![clear(), BlitzImeEvent::Commit(String::new())]);
    }
}
