//! One adapter, one device, one `vello_hybrid::Renderer` shared by every surface.

use std::sync::atomic::{AtomicUsize, Ordering};

use vello_hybrid::{RenderTargetConfig, Renderer, Resources};
use wgpu_context::DeviceHandle;

use super::block_on::block_on;

/// Counts devices created in this process; item (g) asserts it stays at 1.
pub static DEVICES_CREATED: AtomicUsize = AtomicUsize::new(0);

/// The format every surface renders in (what vello_hybrid's own window renderer uses).
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// The shared GPU state.
pub struct Gpu {
    pub handle: DeviceHandle,
    pub renderer: Renderer,
    pub resources: Resources,
    pub adapter_name: String,
}

/// A wgpu instance: Vulkan first, GL second (overridable with `WGPU_BACKEND`).
pub fn instance(conn: &wayland_client::Connection) -> wgpu::Instance {
    let mut desc = match super::raw_handle::Display::new(conn) {
        Some(d) => wgpu::InstanceDescriptor::new_with_display_handle(Box::new(d)),
        None => wgpu::InstanceDescriptor::new_without_display_handle(),
    }
    .with_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    }
    wgpu::Instance::new(desc)
}

impl Gpu {
    /// Picks an adapter compatible with `surface` (honouring `WGPU_ADAPTER_NAME`) and creates
    /// the one device.
    pub fn new(instance: &wgpu::Instance, surface: &wgpu::Surface<'_>) -> Result<Self, String> {
        let adapter = block_on(wgpu::util::initialize_adapter_from_env_or_default(
            instance,
            Some(surface),
        ))
        .map_err(|e| format!("no adapter: {e}"))?;
        let info = adapter.get_info();
        let adapter_name = format!("{} ({:?}, {})", info.name, info.backend, info.driver);
        let features =
            adapter.features() & (wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE);
        let desc = wgpu::DeviceDescriptor {
            label: Some("shell-host spike"),
            required_features: features,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        };
        let (device, queue) =
            block_on(adapter.request_device(&desc)).map_err(|e| format!("no device: {e}"))?;
        DEVICES_CREATED.fetch_add(1, Ordering::SeqCst);
        let renderer = Renderer::new(
            &device,
            &RenderTargetConfig {
                format: FORMAT,
                width: 1,
                height: 1,
            },
        );
        Ok(Self {
            handle: DeviceHandle {
                instance: instance.clone(),
                adapter,
                device,
                queue,
            },
            renderer,
            resources: Resources::new(),
            adapter_name,
        })
    }
}
