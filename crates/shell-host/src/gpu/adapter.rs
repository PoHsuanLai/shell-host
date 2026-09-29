//! Validating an adapter by a first present: the ranking is `blitz_kit::adapter::rank`, but a
//! cross-GPU adapter can report the surface as supported and still have its first dmabuf
//! rejected with a fatal protocol error (RADV under KWin on this machine, `zwp_linux_dmabuf`
//! error 7, "importing the supplied dmabufs failed"). The probe runs on a throwaway Wayland
//! connection so the error kills the probe, not the host.
//!
//! See FINDINGS F16.

use crate::error::Result;
use crate::gpu::probe;

/// Whether `adapter` survives a first present: open a second connection, map a 1x1
/// transparent surface with an empty input region, present one frame through `adapter`, round
/// trip, and check the connection for a protocol error.
///
/// wgpu binds an instance to one `wl_display` and refuses a surface from another, so the probe
/// cannot use `instance` (the host connection's): it opens its own instance on the throwaway
/// connection and presents through the adapter there whose `AdapterInfo` equals `adapter`'s.
pub(crate) fn validate(instance: &wgpu::Instance, adapter: &wgpu::Adapter) -> Result<()> {
    let _ = instance;
    probe::first_present(adapter)
}
