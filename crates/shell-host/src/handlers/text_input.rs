//! `zwp_text_input_v3` (IME). sctk 0.20 has no client-side helper, so the manager is bound by
//! hand and both interfaces dispatch directly on `HostState`. Events accumulate in
//! `ime::PendingIme` until `done` (`ime::on_done`).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::{
    self, ZwpTextInputManagerV3,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{self, ZwpTextInputV3};

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
        todo!()
    }

    /// Create the text input for `seat` (once, on the keyboard capability).
    pub fn attach(&mut self, seat: &WlSeat, qh: &QueueHandle<HostState>) {
        todo!()
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for HostState {
    fn event(
        state: &mut Self,
        proxy: &ZwpTextInputManagerV3,
        event: zwp_text_input_manager_v3::Event,
        data: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        todo!()
    }
}

impl Dispatch<ZwpTextInputV3, ()> for HostState {
    /// enter/leave follow keyboard focus; preedit, commit and delete accumulate; `done`
    /// applies them to the focused document.
    fn event(
        state: &mut Self,
        proxy: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        data: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        todo!()
    }
}
