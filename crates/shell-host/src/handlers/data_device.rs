//! `wl_data_device`: the selection offer (paste) and our own data source (copy). Drag and
//! drop into shell surfaces is not supported in v1; the drag callbacks do nothing.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::data_device_manager::WritePipe;
use smithay_client_toolkit::data_device_manager::data_device::DataDeviceHandler;
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl DataDeviceHandler for HostState {
    fn enter(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &WlSurface,
    ) {
        todo!()
    }

    fn leave(&mut self, conn: &Connection, qh: &QueueHandle<Self>, data_device: &WlDataDevice) {
        todo!()
    }

    fn motion(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        todo!()
    }

    /// Remember the new selection offer (or drop ours if someone else took the selection).
    fn selection(&mut self, conn: &Connection, qh: &QueueHandle<Self>, data_device: &WlDataDevice) {
        todo!()
    }

    fn drop_performed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        todo!()
    }
}

impl DataOfferHandler for HostState {
    fn source_actions(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        actions: DndAction,
    ) {
        todo!()
    }

    fn selected_action(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        actions: DndAction,
    ) {
        todo!()
    }
}

impl DataSourceHandler for HostState {
    fn accept_mime(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: Option<String>,
    ) {
        todo!()
    }

    /// Write our copied text into `fd`.
    fn send_request(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        fd: WritePipe,
    ) {
        todo!()
    }

    /// Someone else took the selection: forget our copy.
    fn cancelled(&mut self, conn: &Connection, qh: &QueueHandle<Self>, source: &WlDataSource) {
        todo!()
    }

    fn dnd_dropped(&mut self, conn: &Connection, qh: &QueueHandle<Self>, source: &WlDataSource) {
        todo!()
    }

    fn dnd_finished(&mut self, conn: &Connection, qh: &QueueHandle<Self>, source: &WlDataSource) {
        todo!()
    }

    fn action(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        source: &WlDataSource,
        action: DndAction,
    ) {
        todo!()
    }
}
