//! `ime::on_done`: preedit/commit/delete sequences accumulated over several wire events, applied
//! once `done` arrives, against the public API only. The double-buffering itself (accumulate
//! into `PendingIme`, apply and reset on `done`) is what `handlers/text_input.rs`'s `Dispatch`
//! impl drives; these tests build the same `PendingIme` a sequence of events would leave behind
//! and check what `on_done` does with it.

use crate::ime::{DeleteSurrounding, Preedit, on_done};
use blitz_traits::events::BlitzImeEvent;

/// One step in a wire sequence: which buffered field it sets (`PendingIme` is double-buffered,
/// so later events of the same kind in one cycle replace earlier ones; every `done` resets the
/// shown preedit, so anything but a lone new preedit starts with a clear).
#[derive(Debug, Clone, Default)]
struct Pending {
    preedit: Option<Preedit>,
    commit: Option<&'static str>,
    delete: Option<(u32, u32)>,
}

/// The preedit reset every `done` starts with unless a new preedit replaces it.
fn clear() -> BlitzImeEvent {
    BlitzImeEvent::Preedit(String::new(), None)
}

fn pending(steps: Pending) -> crate::ime::PendingIme {
    crate::ime::PendingIme {
        preedit: steps.preedit,
        commit: steps.commit.map(str::to_owned),
        delete: steps
            .delete
            .map(|(before, after)| DeleteSurrounding { before, after }),
    }
}

/// A `done` with nothing buffered (no preedit_string, commit_string or delete_surrounding_text
/// since the last `done`) only clears the shown preedit and leaves the pending state empty.
#[test]
fn done_with_nothing_pending_only_clears_the_preedit() {
    let (events, rest) = on_done(pending(Pending::default()));
    assert_eq!(events, vec![clear()]);
    assert_eq!(rest, crate::ime::PendingIme::default());
}

/// `preedit_string("ni", 0, 2)` then `done`: one `Preedit` event, cursor at the given range.
#[test]
fn a_single_preedit_commits_its_cursor_range() {
    let (events, rest) = on_done(pending(Pending {
        preedit: Some(Preedit {
            text: "ni".to_owned(),
            cursor: Some((0, 2)),
        }),
        ..Pending::default()
    }));
    assert_eq!(
        events,
        vec![BlitzImeEvent::Preedit("ni".to_owned(), Some((0, 2)))]
    );
    assert_eq!(rest, crate::ime::PendingIme::default());
}

/// `preedit_string(_, -1, -1)` hides the cursor (protocol rule: both -1 means hidden).
#[test]
fn a_preedit_with_both_cursors_at_minus_one_hides_the_cursor() {
    let (events, _) = on_done(pending(Pending {
        preedit: Some(Preedit {
            text: "n".to_owned(),
            cursor: None,
        }),
        ..Pending::default()
    }));
    assert_eq!(events, vec![BlitzImeEvent::Preedit("n".to_owned(), None)]);
}

/// `commit_string("你")` then `done`: one `Commit` event.
#[test]
fn a_single_commit_produces_one_event() {
    let (events, rest) = on_done(pending(Pending {
        commit: Some("你"),
        ..Pending::default()
    }));
    assert_eq!(
        events,
        vec![clear(), BlitzImeEvent::Commit("你".to_owned())]
    );
    assert_eq!(rest, crate::ime::PendingIme::default());
}

/// `commit_string("")` is a real event (it clears any shown preedit without inserting text),
/// distinct from no `commit_string` at all.
#[test]
fn an_empty_commit_string_is_still_a_commit_event() {
    let (events, _) = on_done(pending(Pending {
        commit: Some(""),
        ..Pending::default()
    }));
    assert_eq!(events, vec![clear(), BlitzImeEvent::Commit(String::new())]);
}

/// `delete_surrounding_text(1, 0)` then `done`: one `DeleteSurrounding` event.
#[test]
fn a_single_delete_produces_one_event() {
    let (events, rest) = on_done(pending(Pending {
        delete: Some((1, 0)),
        ..Pending::default()
    }));
    assert_eq!(
        events,
        vec![
            clear(),
            BlitzImeEvent::DeleteSurrounding {
                before_bytes: 1,
                after_bytes: 0,
            }
        ]
    );
    assert_eq!(rest, crate::ime::PendingIme::default());
}

/// A full compose-then-commit cycle: `delete_surrounding_text`, `commit_string` and
/// `preedit_string` all buffered before one `done` apply in the protocol's fixed order —
/// clear the old preedit, delete, commit, new preedit — regardless of the order the wire events themselves arrived.
#[test]
fn a_full_cycle_applies_delete_then_commit_then_preedit() {
    let (events, rest) = on_done(pending(Pending {
        preedit: Some(Preedit {
            text: "ni".to_owned(),
            cursor: Some((0, 2)),
        }),
        commit: Some("你"),
        delete: Some((1, 0)),
    }));
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
    assert_eq!(rest, crate::ime::PendingIme::default());
}

/// Two `done` cycles in a row: the second sees only what was buffered since the first (the
/// double-buffering resets, it does not accumulate across cycles).
#[test]
fn a_second_done_only_sees_what_was_buffered_since_the_first() {
    let (first_events, after_first) = on_done(pending(Pending {
        commit: Some("a"),
        ..Pending::default()
    }));
    assert_eq!(
        first_events,
        vec![clear(), BlitzImeEvent::Commit("a".to_owned())]
    );

    // Nothing new buffered before the next `done`: only a clear, not a repeat of "a".
    let (second_events, _) = on_done(after_first);
    assert_eq!(second_events, vec![clear()]);
}

/// Bopomofo's `s u 3 c l 3` then Return, as fcitx5 sends it: the preedit 你好 is replaced by
/// nothing and 你好 is committed in one `done`. Committing with the composition still shown
/// doubled it (你好你好).
#[test]
fn a_commit_of_the_composed_text_clears_the_composition_first() {
    let (events, _) = on_done(pending(Pending {
        preedit: Some(Preedit {
            text: String::new(),
            cursor: Some((0, 0)),
        }),
        commit: Some("你好"),
        ..Pending::default()
    }));
    assert_eq!(
        events,
        vec![
            clear(),
            BlitzImeEvent::Commit("你好".to_owned()),
            BlitzImeEvent::Preedit(String::new(), Some((0, 0))),
        ]
    );
}
