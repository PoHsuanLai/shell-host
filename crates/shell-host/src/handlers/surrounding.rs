//! Sends the focused field's surrounding text to the input method.
//!
//! `set_surrounding_text` and `set_text_change_cause` are queued on the protocol object and take
//! effect on `commit`, so a caller that is already about to commit uses `queue` and one that is
//! not uses `sync`. What to send is `ime::decide`; this file only reads the field and writes the
//! requests. Blitz hands over the text (`SurfaceDocument::surrounding_text`), already cut to the
//! 4000 bytes the protocol allows.

use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ChangeCause as WireCause;

use crate::ime::{self, ChangeCause, ImeFocus, SentSurrounding};
use crate::state::HostState;

/// Whether `queue` wrote requests that now wait for a `commit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Queued {
    Requests,
    Nothing,
}

/// Forget what was sent: the compositor does too, on `enable`, `disable` and `leave`.
pub(crate) fn forget(state: &mut HostState) {
    state.ime.surrounding = SentSurrounding::default();
}

/// Queue the focused field's surrounding text and the cause of the change, when it differs from
/// what was last sent. The cause then goes back to `Other`: only a `done` makes it `InputMethod`.
pub(crate) fn queue(state: &mut HostState) -> Queued {
    let cause = std::mem::take(&mut state.ime.cause);
    let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) else {
        return Queued::Nothing;
    };
    let current = state
        .input
        .keyboard_focus
        .and_then(|id| state.surfaces.get(&id))
        .and_then(|surface| surface.document.surrounding_text());
    let Some(outgoing) = ime::decide(&state.ime.surrounding, current, cause) else {
        return Queued::Nothing;
    };
    let surrounding = outgoing.surrounding;
    // At most 4000 bytes, so the offsets fit.
    let offset = |at: usize| i32::try_from(at).unwrap_or(i32::MAX);
    input.set_surrounding_text(
        surrounding.text.clone(),
        offset(surrounding.cursor),
        offset(surrounding.anchor),
    );
    input.set_text_change_cause(match outgoing.cause {
        ChangeCause::InputMethod => WireCause::InputMethod,
        ChangeCause::Other => WireCause::Other,
    });
    state.ime.surrounding = SentSurrounding {
        text: Some(surrounding),
    };
    Queued::Requests
}

/// Send the surrounding text now, with its own `commit`, when the IME is enabled and the text
/// changed.
pub(crate) fn sync(state: &mut HostState) {
    if matches!(state.ime.focus, ImeFocus::Disabled) {
        state.ime.cause = ChangeCause::Other;
        return;
    }
    if queue(state) == Queued::Nothing {
        return;
    }
    if let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) {
        input.commit();
        state.ime.serial += 1;
    }
}
