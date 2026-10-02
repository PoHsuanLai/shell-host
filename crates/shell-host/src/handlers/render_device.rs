//! The compositor's main render device, from `zwp_linux_dmabuf_v1`'s default feedback (v4+),
//! for `blur_health` and for the adapter choice (`render_gpu`). wgpu does its own dmabuf
//! negotiation; this binds a second manager object only to read `main_device`, looks its PCI
//! ids up in sysfs once, and then destroys the feedback object. The event arrives in the
//! roundtrip `Host::connect` runs before any surface exists, so the ids are known when the GPU
//! context is created; a compositor below dmabuf v4 leaves them unknown.

use std::path::Path;

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::{
    self, ZwpLinuxDmabufFeedbackV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::{
    self, ZwpLinuxDmabufV1,
};

use crate::blur_health::{RenderVendor, render_vendor};
use crate::render_gpu::{DeviceNumber, PciIds, pci_ids};

/// The bound manager and what its default feedback said about the main device.
#[derive(Debug)]
pub(crate) struct RenderDevice {
    manager: ZwpLinuxDmabufV1,
    ids: Option<PciIds>,
}

impl RenderDevice {
    /// Bind `zwp_linux_dmabuf_v1` at v4 or later and request the default feedback; the vendor
    /// is known after the next roundtrip.
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        qh: &QueueHandle<D>,
    ) -> Result<RenderDevice, BindError>
    where
        D: Dispatch<ZwpLinuxDmabufV1, ()> + Dispatch<ZwpLinuxDmabufFeedbackV1, ()> + 'static,
    {
        let manager: ZwpLinuxDmabufV1 = globals.bind(qh, 4..=5, ())?;
        manager.get_default_feedback(qh, ());
        Ok(RenderDevice { manager, ids: None })
    }

    /// The main device's PCI ids; `None` until the feedback arrived or when they cannot be read.
    pub(crate) fn ids(&self) -> Option<PciIds> {
        self.ids
    }

    /// The main device's vendor; `Unknown` when the ids are not known.
    pub(crate) fn vendor(&self) -> RenderVendor {
        self.ids
            .map_or(RenderVendor::Unknown, |ids| render_vendor(ids.vendor))
    }
}

/// The PCI ids of `device`, read from sysfs. Any failure is `None`.
fn ids_of(device: &[u8]) -> Option<PciIds> {
    pci_ids(Path::new("/sys"), DeviceNumber::from_wire(device)?)
}

impl<D> Dispatch<ZwpLinuxDmabufV1, (), D> for RenderDevice
where
    D: Dispatch<ZwpLinuxDmabufV1, ()> + 'static,
{
    fn event(
        _state: &mut D,
        _manager: &ZwpLinuxDmabufV1,
        _event: zwp_linux_dmabuf_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        // `format` and `modifier` are not sent at v4+; nothing else is an event.
    }
}

impl<D> Dispatch<ZwpLinuxDmabufFeedbackV1, (), D> for RenderDevice
where
    D: Dispatch<ZwpLinuxDmabufFeedbackV1, ()> + AsMut<RenderDevice> + 'static,
{
    fn event(
        state: &mut D,
        feedback: &ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } => {
                state.as_mut().ids = ids_of(&device);
            }
            // One set of feedback is all this needs; the format table fd is dropped with its
            // event.
            zwp_linux_dmabuf_feedback_v1::Event::Done => feedback.destroy(),
            _ => {}
        }
    }
}

impl Drop for RenderDevice {
    fn drop(&mut self) {
        self.manager.destroy();
    }
}
