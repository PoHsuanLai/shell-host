mod done;

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
