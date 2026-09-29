//! The shared GPU state: the instance, the one validated adapter and device, the one
//! `vello_hybrid` renderer with its resources, and the per-device image cache.
//!
//! See FINDINGS F1.

use std::sync::atomic::{AtomicU32, Ordering};

use blitz_kit::adapter::{AdapterFacts, AdapterPref, rank};
use rustc_hash::FxHashMap;
use vello_common::paint::ImageId;
use vello_hybrid::{RenderTargetConfig, Renderer, Resources};
use wayland_client::Connection;
use wgpu_context::DeviceHandle;

use crate::config::ADAPTER_ENV;
use crate::error::{HostError, Result};
use crate::gpu::adapter;
use crate::gpu::block_on::block_on;
use crate::gpu::context_proxy::ContextProxy;
use crate::gpu::raw_handle::WaylandDisplay;

/// Every surface renders in this format (what vello_hybrid's own window renderer uses).
pub(crate) const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

static DEVICES_CREATED: AtomicU32 = AtomicU32::new(0);

/// GPU devices this process has created. Item (g) asserts it stays at 1.
///
/// The throwaway devices `adapter::validate` opens to probe a candidate are not counted: they
/// never render a surface and are gone before the real device exists.
pub(crate) fn devices_created() -> u32 {
    DEVICES_CREATED.load(Ordering::Relaxed)
}

/// The one device every surface shares.
pub(crate) struct GpuContext {
    pub(crate) device: DeviceHandle,
    pub(crate) renderer: Renderer,
    pub(crate) resources: Resources,
    /// Uploaded images by content hash, kept across frames.
    pub(crate) images: FxHashMap<u64, ImageId>,
    pub(crate) widgets: ContextProxy,
    pub(crate) adapter: AdapterFacts,
}

impl GpuContext {
    /// A wgpu instance for `conn`: Vulkan, then GL (overridable with `WGPU_BACKEND`), with the
    /// display handle in the descriptor (GL needs it; `raw_handle::WaylandDisplay`).
    pub(crate) fn instance(conn: &Connection) -> wgpu::Instance {
        let desc = match WaylandDisplay::new(conn) {
            Some(display) => wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display)),
            None => wgpu::InstanceDescriptor::new_without_display_handle(),
        };
        wgpu::Instance::new(with_backends(desc))
    }

    /// Rank the adapters compatible with `probe_surface` under `pref` (after the
    /// `WGPU_ADAPTER_NAME` override), validate them in order, and create the device on the
    /// first that survives. `HostError::NoAdapter` (a `Fallback`) when none does: the host then
    /// switches to the CPU backend.
    pub(crate) fn new(
        instance: &wgpu::Instance,
        probe_surface: &wgpu::Surface<'_>,
        pref: &AdapterPref,
    ) -> Result<GpuContext> {
        let compatible: Vec<wgpu::Adapter> = enumerate(instance)
            .into_iter()
            .filter(|a| a.is_surface_supported(probe_surface))
            .collect();
        let mut rejected = Vec::new();
        for adapter in ranked(compatible, pref) {
            match adapter::validate(instance, &adapter) {
                Ok(()) => return create(instance, adapter),
                Err(error) if error.recovery() == crate::error::Recovery::Fallback => {
                    eprintln!("shell-host: {error}; trying the next adapter");
                    rejected.push(error.to_string());
                }
                Err(error) => return Err(error),
            }
        }
        Err(HostError::NoAdapter(no_adapter_reason(&rejected)))
    }

    /// A device with no surface to present to, for offscreen rendering (tests, headless GPU
    /// work, debug captures): the adapters are ranked under `pref` (after the
    /// `WGPU_ADAPTER_NAME` override) and the first that yields a device is used. There is
    /// nothing to validate by presenting, so no probe runs.
    #[cfg(test)]
    pub(crate) fn offscreen(pref: &AdapterPref) -> Result<GpuContext> {
        let instance = wgpu::Instance::new(with_backends(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        ));
        let mut failed = Vec::new();
        for adapter in ranked(enumerate(&instance), pref) {
            match create(&instance, adapter) {
                Ok(gpu) => return Ok(gpu),
                Err(error) => failed.push(error.to_string()),
            }
        }
        Err(HostError::NoAdapter(no_adapter_reason(&failed)))
    }

    /// The adapter in use.
    pub(crate) fn adapter(&self) -> &AdapterFacts {
        &self.adapter
    }
}

/// Vulkan then GL unless `WGPU_BACKEND` says otherwise, plus wgpu's other environment knobs.
pub(crate) fn with_backends(desc: wgpu::InstanceDescriptor) -> wgpu::InstanceDescriptor {
    let mut desc = desc.with_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    }
    desc
}

pub(crate) fn enumerate(instance: &wgpu::Instance) -> Vec<wgpu::Adapter> {
    block_on(instance.enumerate_adapters(wgpu::Backends::all()))
}

/// `adapters` in the order `adapter::rank` gives under the effective preference.
fn ranked(adapters: Vec<wgpu::Adapter>, pref: &AdapterPref) -> Vec<wgpu::Adapter> {
    let pref = pref
        .clone()
        .with_env(std::env::var(ADAPTER_ENV).ok().as_deref());
    let facts: Vec<AdapterFacts> = adapters.iter().map(AdapterFacts::of).collect();
    let order = rank(&pref, &facts);
    let mut slots: Vec<Option<wgpu::Adapter>> = adapters.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

fn no_adapter_reason(reasons: &[String]) -> String {
    if reasons.is_empty() {
        "no adapter can present to this surface".into()
    } else {
        reasons.join("; ")
    }
}

/// The one device, and the renderer every surface shares.
fn create(instance: &wgpu::Instance, adapter: wgpu::Adapter) -> Result<GpuContext> {
    let facts = AdapterFacts::of(&adapter);
    let features =
        adapter.features() & (wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE);
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("shell-host"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    }))
    .map_err(|e| HostError::NoAdapter(format!("{}: no device: {e}", facts.label())))?;
    DEVICES_CREATED.fetch_add(1, Ordering::Relaxed);
    // The render target size is rewritten by every `render`, so one renderer serves
    // surfaces of every size.
    let renderer = Renderer::new(
        &device,
        &RenderTargetConfig {
            format: SURFACE_FORMAT,
            width: 1,
            height: 1,
        },
    );
    Ok(GpuContext {
        device: DeviceHandle {
            instance: instance.clone(),
            adapter,
            device,
            queue,
        },
        renderer,
        resources: Resources::new(),
        images: FxHashMap::default(),
        widgets: ContextProxy::default(),
        adapter: facts,
    })
}

impl std::fmt::Debug for GpuContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuContext")
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}
