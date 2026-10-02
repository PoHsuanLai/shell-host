//! The GPU the compositor renders on, as PCI ids, so the shell's adapter can follow it.
//!
//! `zwp_linux_dmabuf_feedback_v1.main_device` is a `dev_t`; sysfs maps that character device to
//! its PCI function, whose `vendor` and `device` files wgpu's `AdapterInfo` agrees with. Pure
//! over a sysfs root (`/sys` in the host, a fake tree in the tests); the handler that receives
//! the event is `handlers::render_device`.

use std::path::{Path, PathBuf};

use blitz_kit::adapter::{AdapterPref, PciDevice, PciVendor};

/// A Linux device number, as `zwp_linux_dmabuf_feedback_v1.main_device` carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceNumber {
    pub major: u32,
    pub minor: u32,
}

/// The PCI function a device sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PciIds {
    pub vendor: PciVendor,
    pub device: PciDevice,
}

impl DeviceNumber {
    /// Decode the wire's `dev_t` (native-endian bytes) with glibc's `major`/`minor` split.
    /// `None` unless it is exactly the 8 bytes of a Linux `dev_t`.
    pub(crate) fn from_wire(bytes: &[u8]) -> Option<DeviceNumber> {
        let dev = u64::from_ne_bytes(bytes.try_into().ok()?);
        let major = ((dev >> 32) & 0xffff_f000) | ((dev >> 8) & 0x0000_0fff);
        let minor = ((dev >> 12) & 0xffff_ff00) | (dev & 0x0000_00ff);
        Some(DeviceNumber {
            major: u32::try_from(major).ok()?,
            minor: u32::try_from(minor).ok()?,
        })
    }

    /// `<root>/dev/char/<major>:<minor>/device/<file>`: the PCI function's attribute `file`.
    fn attribute(self, root: &Path, file: &str) -> PathBuf {
        root.join(format!("dev/char/{}:{}/device", self.major, self.minor))
            .join(file)
    }
}

/// The PCI ids of the function `number` sits on, read under `sysfs_root`. `None` when the
/// device has no PCI function (a virtual or SoC GPU), or a file is missing or unparsable.
pub(crate) fn pci_ids(sysfs_root: &Path, number: DeviceNumber) -> Option<PciIds> {
    let id = |file: &str| {
        std::fs::read_to_string(number.attribute(sysfs_root, file))
            .ok()
            .and_then(|text| parse_id(&text))
    };
    Some(PciIds {
        vendor: PciVendor(id("vendor")?),
        device: PciDevice(id("device")?),
    })
}

/// An id as sysfs prints it (`0x10de\n`).
fn parse_id(text: &str) -> Option<u32> {
    let hex = text.trim().strip_prefix("0x")?;
    u32::from_str_radix(hex, 16).ok()
}

/// `configured`, unless it is `Auto` and the compositor's GPU is known: then that GPU. An
/// explicit configured preference is the person's choice and stands; so does
/// `WGPU_ADAPTER_NAME`, which `AdapterPref::with_env` applies on top of whatever this returns.
pub(crate) fn follow_compositor(
    configured: &AdapterPref,
    compositor: Option<PciIds>,
) -> AdapterPref {
    match (configured, compositor) {
        (AdapterPref::Auto, Some(ids)) => AdapterPref::Device {
            vendor: ids.vendor,
            device: ids.device,
        },
        _ => configured.clone(),
    }
}

#[cfg(test)]
mod tests;
