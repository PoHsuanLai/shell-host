//! `zwp_text_input_v3` (IME). sctk 0.20 has no client-side helper, so the manager is bound by
//! hand and both interfaces dispatch directly on `HostState`. Events accumulate in
//! `ime::PendingIme` until `done` (`ime::on_done`).

use blitz_traits::events::UiEvent;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::{
    self, ZwpTextInputManagerV3,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    self, ContentHint, ContentPurpose, ZwpTextInputV3,
};

use crate::dom::shell_provider::ImeRequest;
use crate::ime::{self, DeleteSurrounding, ImeFocus, Preedit};
use crate::region::Rect;
use crate::state::HostState;

/// The bound manager and the seat's text input.
#[derive(Debug)]
pub struct TextInputManager {
    pub(crate) manager: ZwpTextInputManagerV3,
    pub(crate) input: Option<ZwpTextInputV3>,
}

impl TextInputManager {
    /// Bind `zwp_text_input_manager_v3`.
    pub fn bind(
        globals: &GlobalList,
        qh: &QueueHandle<HostState>,
    ) -> Result<TextInputManager, BindError> {
        let manager = globals.bind(qh, 1..=1, ())?;
        Ok(TextInputManager {
            manager,
            input: None,
        })
    }

    /// Create the text input for `seat` (once, on the keyboard capability).
    pub fn attach(&mut self, seat: &WlSeat, qh: &QueueHandle<HostState>) {
        if self.input.is_some() {
            return;
        }
        self.input = Some(self.manager.get_text_input(seat, qh, ()));
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for HostState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpTextInputManagerV3,
        _event: zwp_text_input_manager_v3::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        unreachable!("zwp_text_input_manager_v3 has no events")
    }
}

impl Dispatch<ZwpTextInputV3, ()> for HostState {
    /// enter/leave follow keyboard focus; preedit, commit and delete accumulate; `done`
    /// applies them to the focused document.
    fn event(
        state: &mut Self,
        _proxy: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface: _ } => {
                // A fresh focus starts with nothing pending. The compositor ignores `enable`
                // sent while we had no text-input focus and resets it on `leave`, so what the
                // document asked for is sent again now (a field focused before the keyboard
                // arrived, as the launcher's is, would otherwise never reach the IME).
                state.ime.pending = Default::default();
                if let ImeFocus::Enabled { cursor_area } = state.ime.focus {
                    send_enabled(state, cursor_area, Resend::Enable);
                }
            }
            zwp_text_input_v3::Event::Leave { surface: _ } => {
                // Nothing composed on the surface we are leaving belongs to the next one.
                state.ime.pending = Default::default();
            }
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                let cursor = if cursor_begin == -1 && cursor_end == -1 {
                    None
                } else {
                    Some((cursor_begin.max(0) as usize, cursor_end.max(0) as usize))
                };
                state.ime.pending.preedit = Some(Preedit {
                    text: text.unwrap_or_default(),
                    cursor,
                });
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.ime.pending.commit = Some(text.unwrap_or_default());
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.ime.pending.delete = Some(DeleteSurrounding {
                    before: before_length,
                    after: after_length,
                });
            }
            zwp_text_input_v3::Event::Done { serial: _ } => {
                let pending = std::mem::take(&mut state.ime.pending);
                let (events, rest) = ime::on_done(pending);
                state.ime.pending = rest;
                let Some(id) = state.input.keyboard_focus else {
                    return;
                };
                if let Some(surface) = state.surfaces.get_mut(&id) {
                    for event in events {
                        surface.document.handle_event(UiEvent::Ime(event));
                    }
                }
                state.invalidate(id);
            }
            _ => {}
        }
    }
}

/// Apply a document's IME request (`dom::shell_provider::ImeRequest`, read by whoever drains
/// `HostShellProvider::ime()` at flush — `HostCtx`, H2 core) to the protocol object: `enable` +
/// a default content type, or `disable`, followed by `set_cursor_rectangle` and `commit` (plan
/// "set_ime_enabled -> enable + content type + commit, cursor area -> set_cursor_rectangle").
/// Does nothing without a text input object (no `zwp_text_input_manager_v3`, or no keyboard
/// capability yet). Not gated on the `done` serial matching our own commit count (a compositor
/// that acknowledges a stale commit is treated the same as one that is current) — a known
/// simplification, see this crate's H2-input report.
pub(crate) fn apply_ime_request(state: &mut HostState, request: ImeRequest) {
    let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) else {
        return;
    };
    match request {
        ImeRequest::Disabled => {
            if matches!(state.ime.focus, ImeFocus::Disabled) {
                return;
            }
            input.disable();
            input.commit();
            state.ime.serial += 1;
            state.ime.focus = ImeFocus::Disabled;
        }
        ImeRequest::Enabled { cursor_area } => {
            let resend = match state.ime.focus {
                ImeFocus::Disabled => Resend::Enable,
                ImeFocus::Enabled { .. } => Resend::CursorOnly,
            };
            send_enabled(state, cursor_area, resend);
        }
    }
}

/// Whether `send_enabled` also sends `enable` and the content type, or only the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resend {
    Enable,
    CursorOnly,
}

/// `enable` + content type when asked, then the caret rectangle and `commit`.
fn send_enabled(state: &mut HostState, cursor_area: Rect, resend: Resend) {
    let Some(input) = state.text_input.as_ref().and_then(|m| m.input.as_ref()) else {
        return;
    };
    if resend == Resend::Enable {
        input.enable();
        input.set_content_type(ContentHint::empty(), ContentPurpose::Normal);
    }
    // The document's rectangle, in the surface's coordinates (past any resize band).
    let inset = state
        .input
        .keyboard_focus
        .and_then(|id| state.surfaces.get(&id))
        .map_or(0, |surface| surface.inset.0);
    input.set_cursor_rectangle(
        cursor_area.x + inset,
        cursor_area.y + inset,
        cursor_area.width,
        cursor_area.height,
    );
    input.commit();
    state.ime.serial += 1;
    state.ime.focus = ImeFocus::Enabled { cursor_area };
}
