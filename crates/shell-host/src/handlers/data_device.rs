//! `wl_data_device`: the selection offer (paste) and our own data source (copy). Drag and
//! drop into shell surfaces is not supported in v1; the drag callbacks do nothing.

use std::io::Write;

use smithay_client_toolkit::data_device_manager::WritePipe;
use smithay_client_toolkit::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
use wayland_client::Proxy;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, QueueHandle};

use crate::clipboard::TEXT_MIME;
use crate::state::HostState;

impl DataDeviceHandler for HostState {
    /// Drag-and-drop into a shell surface is not supported in v1 (module doc); no-op.
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
        _wl_surface: &WlSurface,
    ) {
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {}

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
    ) {
    }

    /// Remember the new selection offer. When it is our own (we just called `set_selection`),
    /// `Clipboard::paste`'s `own` cache is read first and this offer is simply never used; when
    /// someone else's, `own`/`source` are cleared by `DataSourceHandler::cancelled` on our old
    /// source, not here.
    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        let offer = data_device
            .data::<DataDeviceData>()
            .and_then(|data| data.selection_offer());
        self.clipboard.offer = offer;
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
    }
}

impl DataOfferHandler for HostState {
    /// Drag-and-drop actions; not supported (module doc). No-op.
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }
}

impl DataSourceHandler for HostState {
    /// Informational only (which mime a paste target accepted); nothing to react to.
    fn accept_mime(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: Option<String>,
    ) {
    }

    /// Write our copied text into `fd`; closing it (by dropping it here) is what tells the
    /// reader it has seen everything.
    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: String,
        mut fd: WritePipe,
    ) {
        if let Some(text) = &self.clipboard.own {
            let _ = fd.write_all(text.as_bytes());
        }
    }

    /// Someone else took the selection: forget our copy and let the source (no longer ours to
    /// keep alive) be destroyed.
    fn cancelled(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, source: &WlDataSource) {
        if self
            .clipboard
            .source
            .as_ref()
            .is_some_and(|owned| owned.inner().id() == source.id())
        {
            self.clipboard.own = None;
            self.clipboard.source = None;
        }
    }

    fn dnd_dropped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _source: &WlDataSource) {
    }

    fn dnd_finished(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _action: DndAction,
    ) {
    }
}

/// Claim the selection with `text` (`wl_data_device.set_selection`): creates a fresh
/// `CopyPasteSource` offering [`TEXT_MIME`], sends it with the serial of the seat's last input
/// event, and updates the cache `Clipboard::copy` maintains. Called by whoever drains
/// `Request::CopyText` (`HostCtx`, H2 core) — `Clipboard::copy` cannot make the wire request
/// itself: its frozen signature has neither a queue handle nor an input serial to give it.
///
/// Not yet called from anywhere in this crate: `Request::CopyText` is drained by `HostCtx`
/// (H2 core, `ctx.rs`), which is `todo!()` in this worktree. `#[allow(dead_code)]` until that
/// wave wires it in; see this crate's H2-input report.
#[allow(dead_code)]
pub(crate) fn claim_selection(state: &mut HostState, qh: &QueueHandle<HostState>, text: String) {
    state.clipboard.copy(text);
    let (Some(manager), Some(device)) = (&state.data_device, &state.input.data_device) else {
        return;
    };
    let source = manager.create_copy_paste_source(qh, TEXT_MIME.iter().copied());
    source.set_selection(device, state.input.last_serial);
    state.clipboard.source = Some(source);
}
