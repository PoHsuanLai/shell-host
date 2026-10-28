//! `ime::decide`: what to send about the text around the caret.

use blitz_dom::node::SurroundingText;

use crate::ime::surrounding::Outgoing;
use crate::ime::{SentSurrounding, decide};

fn text(text: &str, cursor: usize, anchor: usize) -> SurroundingText {
    SurroundingText {
        text: text.to_owned(),
        cursor,
        anchor,
    }
}

fn sent(text: Option<SurroundingText>) -> SentSurrounding {
    SentSurrounding { text }
}

fn outgoing(surrounding: SurroundingText, by_input_method: bool) -> Option<Outgoing> {
    Some(Outgoing {
        surrounding,
        by_input_method,
    })
}

#[test]
fn sends_only_what_changed_since_the_last_send() {
    let abc = text("abc", 3, 3);
    // (last sent, current, cause, expected)
    let cases = [
        // A field just focused: nothing sent yet, so everything is.
        (None, Some(abc.clone()), false, outgoing(abc.clone(), false)),
        // Nothing changed: nothing sent, whatever the cause.
        (Some(abc.clone()), Some(abc.clone()), true, None),
        // The text changed after an input-method `done`.
        (
            Some(abc.clone()),
            Some(text("abcd", 4, 4)),
            true,
            outgoing(text("abcd", 4, 4), true),
        ),
        // Only the caret moved (a click or an arrow key).
        (
            Some(abc.clone()),
            Some(text("abc", 1, 1)),
            false,
            outgoing(text("abc", 1, 1), false),
        ),
        // Only the selection's fixed end moved.
        (
            Some(text("abc", 1, 1)),
            Some(text("abc", 1, 0)),
            false,
            outgoing(text("abc", 1, 0), false),
        ),
        // No text field has the focus: nothing to say, sent or not.
        (None, None, false, None),
        (Some(abc), None, false, None),
    ];
    for (last, current, cause, expected) in cases {
        assert_eq!(
            decide(&sent(last.clone()), current.clone(), cause),
            expected,
            "{last:?} -> {current:?} (by input method: {cause})"
        );
    }
}
