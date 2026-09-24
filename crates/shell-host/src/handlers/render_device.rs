//! The compositor's main render device, from `zwp_linux_dmabuf_v1`'s default feedback (v4+),
//! for `blur_health` (FINDINGS F29). wgpu does its own dmabuf negotiation; this binds a second
//! manager object only to read `main_device`, looks its PCI vendor up in sysfs once, and then
//! destroys the feedback object.

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::{
    self, ZwpLinuxDmabufFeedbackV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::{
    self, ZwpLinuxDmabufV1,
};

use crate::blur_health::{DeviceNumber, RenderVendor, render_vendor};

/// The bound manager and what its default feedback said about the main device.
#[derive(Debug)]
pub struct RenderDevice {
    manager: ZwpLinuxDmabufV1,
    vendor: RenderVendor,
}

impl RenderDevice {
    /// Bind `zwp_linux_dmabuf_v1` at v4 or later and request the default feedback; the vendor
    /// is known after the next roundtrip.
    pub fn bind<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Result<RenderDevice, BindError>
    where
        D: Dispatch<ZwpLinuxDmabufV1, ()> + Dispatch<ZwpLinuxDmabufFeedbackV1, ()> + 'static,
    {
        let manager: ZwpLinuxDmabufV1 = globals.bind(qh, 4..=5, ())?;
        manager.get_default_feedback(qh, ());
        Ok(RenderDevice {
            manager,
            vendor: RenderVendor::Unknown,
        })
    }

    /// The main device's vendor; `Unknown` until the feedback arrived or when it cannot be read.
    pub fn vendor(&self) -> RenderVendor {
        self.vendor
    }
}

/// The vendor of `device`, read from sysfs. Any failure is `Unknown`.
fn vendor_of(device: &[u8]) -> RenderVendor {
    DeviceNumber::from_wire(device)
        .and_then(|number| std::fs::read_to_string(number.sysfs_vendor_path()).ok())
        .map_or(RenderVendor::Unknown, |text| render_vendor(&text))
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
                state.as_mut().vendor = vendor_of(&device);
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
