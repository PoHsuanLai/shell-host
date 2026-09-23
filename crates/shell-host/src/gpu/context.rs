//! The shared GPU state: the instance, the one validated adapter and device, the one
//! `vello_hybrid` renderer with its resources, and the per-device image cache.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::sync::atomic::{AtomicU32, Ordering};

use rustc_hash::FxHashMap;
use vello_common::paint::ImageId;
use vello_hybrid::{Renderer, Resources};
use wayland_client::Connection;
use wgpu_context::DeviceHandle;

use crate::config::AdapterPref;
use crate::error::Result;
use crate::gpu::adapter::AdapterFacts;
use crate::gpu::context_proxy::ContextProxy;

/// Every surface renders in this format (what vello_hybrid's own window renderer uses).
pub const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

static DEVICES_CREATED: AtomicU32 = AtomicU32::new(0);

/// GPU devices this process has created. Item (g) asserts it stays at 1.
pub fn devices_created() -> u32 {
    DEVICES_CREATED.load(Ordering::Relaxed)
}

/// The one device every surface shares.
pub struct GpuContext {
    pub(crate) device: DeviceHandle,
    pub(crate) renderer: Renderer,
    pub(crate) resources: Resources,
    /// Uploaded images by content hash, kept across frames (SPIKE.md S1 finding 5; the spike
    /// rebuilt it every frame because the types were not in the pinned block).
    pub(crate) images: FxHashMap<u64, ImageId>,
    pub(crate) widgets: ContextProxy,
    pub(crate) adapter: AdapterFacts,
}

impl GpuContext {
    /// A wgpu instance for `conn`: Vulkan, then GL (overridable with `WGPU_BACKEND`), with the
    /// display handle in the descriptor (GL needs it; `raw_handle::WaylandDisplay`).
    pub fn instance(conn: &Connection) -> wgpu::Instance {
        todo!()
    }

    /// Rank the adapters compatible with `probe_surface` under `pref` (after the
    /// `WGPU_ADAPTER_NAME` override), validate them in order, and create the device on the
    /// first that survives. `HostError::NoAdapter` (a `Fallback`) when none does: the host then
    /// switches to the CPU backend.
    pub fn new(
        instance: &wgpu::Instance,
        probe_surface: &wgpu::Surface<'_>,
        pref: &AdapterPref,
    ) -> Result<GpuContext> {
        todo!()
    }

    /// The adapter in use.
    pub fn adapter(&self) -> &AdapterFacts {
        &self.adapter
    }
}

impl std::fmt::Debug for GpuContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuContext")
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}
