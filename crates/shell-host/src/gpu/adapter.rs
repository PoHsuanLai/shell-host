//! Validating an adapter by a first present: the ranking is `blitz_kit::adapter::rank`, but a
//! cross-GPU adapter can report the surface as supported and still have its first dmabuf
//! rejected with a fatal protocol error (RADV under KWin on this machine, `zwp_linux_dmabuf`
//! error 7, "importing the supplied dmabufs failed"). The probe runs on a throwaway Wayland
//! connection so the error kills the probe, not the host.
//!
//! See FINDINGS F16.

use std::time::Duration;

use blitz_kit::adapter::AdapterFacts;

use crate::error::{HostError, Result};
use crate::gpu::deadline::{Finished, run_within};
use crate::gpu::probe;

/// How long a candidate adapter has to present its first frame. A driver that hangs here would
/// otherwise hang the shell before it drew anything, with no way to fall back to the CPU.
const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// Whether `adapter` survives a first present: open a second connection, map a 1x1
/// transparent surface with an empty input region, present one frame through `adapter`, round
/// trip, and check the connection for a protocol error.
///
/// wgpu binds an instance to one `wl_display` and refuses a surface from another, so the probe
/// cannot use `instance` (the host connection's): it opens its own instance on the throwaway
/// connection and presents through the adapter there whose `AdapterInfo` equals `adapter`'s.
///
/// The probe runs on its own thread and is given [`PROBE_DEADLINE`]; an adapter that has not
/// answered by then is rejected, so the next one, and in the end the CPU backend, is tried.
pub(crate) fn validate(instance: &wgpu::Instance, adapter: &wgpu::Adapter) -> Result<()> {
    let _ = instance;
    let label = AdapterFacts::of(adapter).label();
    let probed = adapter.clone();
    let rejected = |reason: &str| HostError::AdapterRejected {
        adapter: label.clone(),
        reason: reason.to_owned(),
    };
    match run_within(PROBE_DEADLINE, move || probe::first_present(&probed)) {
        Finished::In(verdict) => verdict,
        Finished::TimedOut => Err(rejected("the driver gave no answer in time")),
        Finished::Failed => Err(rejected("the probe stopped on an error")),
    }
}
