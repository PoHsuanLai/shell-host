//! Sends the focused field's surrounding text to the input method.
//!
//! `set_surrounding_text` and `set_text_change_cause` are queued on the protocol object and take
//! effect on `commit`, so a caller that is already about to commit uses `queue` and one that is
//! not uses `sync`. What to send is `ime::decide`; this file only reads the field and writes the
//! requests. Blitz hands over the text (`SurfaceDocument::surrounding_text`), already cut to the
//! 4000 bytes the protocol allows.

use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ChangeCause as WireCause;

use crate::ime::{self, SentSurrounding};
use crate::state::HostState;

/// Forget what was sent: the compositor does too, on `enable`, `disable` and `leave`.
pub(crate) fn forget(state: &mut HostState) {
    state.ime.surrounding = SentSurrounding::default();
}

/// Queue the focused field's surrounding text and the cause of the change, when it differs from
/// what was last sent. The cause then goes back to "other": only a `done` makes it the input method.
/// Whether it wrote requests that now wait for a `commit`.
pub(crate) fn queue(state: &mut HostState) -> bool {
    let by_input_method = std::mem::take(&mut state.ime.by_input_method);
    let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) else {
        return false;
    };
    let current = state
        .input
        .keyboard_focus
        .and_then(|id| state.surfaces.get(&id))
        .and_then(|surface| surface.document.surrounding_text());
    let Some(outgoing) = ime::decide(&state.ime.surrounding, current, by_input_method) else {
        return false;
    };
    let surrounding = outgoing.surrounding;
    // At most 4000 bytes, so the offsets fit.
    let offset = |at: usize| i32::try_from(at).unwrap_or(i32::MAX);
    input.set_surrounding_text(
        surrounding.text.clone(),
        offset(surrounding.cursor),
        offset(surrounding.anchor),
    );
    input.set_text_change_cause(if outgoing.by_input_method {
        WireCause::InputMethod
    } else {
        WireCause::Other
    });
    state.ime.surrounding = SentSurrounding {
        text: Some(surrounding),
    };
    true
}

/// Send the surrounding text now, with its own `commit`, when the IME is enabled and the text
/// changed.
pub(crate) fn sync(state: &mut HostState) {
    if state.ime.focus.is_none() {
        state.ime.by_input_method = false;
        return;
    }
    if !queue(state) {
        return;
    }
    if let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) {
        input.commit();
        state.ime.serial += 1;
    }
}
