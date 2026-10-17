//! What to tell the input method about the text around the caret, and when.
//!
//! `set_surrounding_text` and `set_text_change_cause` are double-buffered like the rest of
//! `zwp_text_input_v3` and take effect on `commit`. The host sends them only when the focused
//! field's text, caret or selection differs from what it sent last, so a frame that changed
//! nothing about the field adds no request. Blitz gives the text (`blitz_dom::node::SurroundingText`,
//! at most 4000 bytes, as the protocol caps it); this module decides. Pure.

use blitz_dom::node::SurroundingText;

/// Why the surrounding text changed, as `set_text_change_cause` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ChangeCause {
    /// The input method's own `done` (a commit, a deletion or a pre-edit) was applied.
    InputMethod,
    /// Typing, the pointer, a script, or a field that just took the focus.
    #[default]
    Other,
}

/// The surrounding text last sent with the current enable, if any. The compositor forgets it on
/// `enable`, `disable` and `leave`, so those clear it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SentSurrounding {
    pub text: Option<SurroundingText>,
}

/// One `set_surrounding_text` + `set_text_change_cause` pair to send before `commit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outgoing {
    pub surrounding: SurroundingText,
    pub cause: ChangeCause,
}

/// The pair to send for `current` (the focused field's surrounding text, `None` when no text
/// field has the focus), or `None` when nothing needs sending: no field, or the same as last
/// sent. Pure.
pub(crate) fn decide(
    sent: &SentSurrounding,
    current: Option<SurroundingText>,
    cause: ChangeCause,
) -> Option<Outgoing> {
    let surrounding = current?;
    if sent.text.as_ref() == Some(&surrounding) {
        return None;
    }
    Some(Outgoing { surrounding, cause })
}
