//! `xdg_activation_v1`: tokens for launching an app with focus handed over
//! (`HostCtx::activation_token`, `SurfaceHandle::activation_token`).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::sync::Mutex;

use smithay_client_toolkit::activation::{ActivationHandler, RequestDataExt};
use tokio::sync::oneshot;
use wayland_client::protocol::{wl_seat::WlSeat, wl_surface::WlSurface};

use crate::state::HostState;

/// One pending token request and where its answer goes.
#[derive(Debug)]
pub struct ActivationRequest {
    pub(crate) app_id: Option<String>,
    pub(crate) seat_and_serial: Option<(WlSeat, u32)>,
    pub(crate) surface: Option<WlSurface>,
    /// Taken when the token arrives (`RequestDataExt` is `Sync`, and `new_token` gets `&self`).
    pub(crate) reply: Mutex<Option<oneshot::Sender<Option<String>>>>,
}

impl RequestDataExt for ActivationRequest {
    fn app_id(&self) -> Option<&str> {
        todo!()
    }

    fn seat_and_serial(&self) -> Option<(&WlSeat, u32)> {
        todo!()
    }

    fn surface(&self) -> Option<&WlSurface> {
        todo!()
    }
}

impl ActivationHandler for HostState {
    type RequestData = ActivationRequest;

    /// Send `token` to the request's reply channel.
    fn new_token(&mut self, token: String, data: &ActivationRequest) {
        todo!()
    }
}
