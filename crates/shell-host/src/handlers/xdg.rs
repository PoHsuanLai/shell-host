//! Toplevel and popup configure, close and dismissal.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl WindowHandler for HostState {
    /// `HostEvent::SurfaceClosed{reason: UserClose}`.
    fn request_close(&mut self, conn: &Connection, qh: &QueueHandle<Self>, window: &Window) {
        todo!()
    }

    /// Size, `activated` state (for the first-mouse filter), invalidate.
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        todo!()
    }
}

impl PopupHandler for HostState {
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        todo!()
    }

    /// `xdg_popup.popup_done`: `SurfaceClosed{reason: PopupDismissed}`, and restore the
    /// parent layer's keyboard mode. A client cannot trigger this itself (SPIKE.md S2), so the
    /// scripted demos dismiss by hit-testing and closing instead.
    fn done(&mut self, conn: &Connection, qh: &QueueHandle<Self>, popup: &Popup) {
        todo!()
    }
}
