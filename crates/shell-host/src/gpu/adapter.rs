//! Choosing the adapter: rank the candidates by preference, then validate each by a first
//! present, because a cross-GPU adapter can report the surface as supported and still have its
//! first dmabuf rejected with a fatal protocol error (SPIKE.md S1: RADV under KWin on this
//! machine, `zwp_linux_dmabuf` error 7, "importing the supplied dmabufs failed"). The probe runs
//! on a throwaway Wayland connection so the error kills the probe, not the host.

use crate::config::AdapterPref;
use crate::error::Result;
use crate::gpu::probe;

/// What kind of device an adapter is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Discrete,
    Integrated,
    Virtual,
    /// A software rasterizer (llvmpipe, lavapipe).
    Cpu,
    Other,
}

/// Which wgpu backend an adapter runs on. Vulkan first, GL second (plan "GPU").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuBackend {
    Vulkan,
    Gl,
}

/// The facts about an adapter the ranking and the stats need.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterFacts {
    pub name: String,
    pub kind: DeviceKind,
    pub backend: GpuBackend,
    pub driver: String,
}

impl AdapterFacts {
    /// The facts of a wgpu adapter.
    pub fn of(adapter: &wgpu::Adapter) -> AdapterFacts {
        let info = adapter.get_info();
        AdapterFacts {
            name: info.name,
            kind: kind_of(info.device_type),
            backend: backend_of(info.backend),
            driver: [info.driver, info.driver_info]
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// `name (backend, driver)`, as `FrameStats::adapter` reports it.
    pub fn label(&self) -> String {
        format!("{} ({:?}, {})", self.name, self.backend, self.driver)
    }
}

fn kind_of(device: wgpu::DeviceType) -> DeviceKind {
    match device {
        wgpu::DeviceType::DiscreteGpu => DeviceKind::Discrete,
        wgpu::DeviceType::IntegratedGpu => DeviceKind::Integrated,
        wgpu::DeviceType::VirtualGpu => DeviceKind::Virtual,
        wgpu::DeviceType::Cpu => DeviceKind::Cpu,
        wgpu::DeviceType::Other => DeviceKind::Other,
    }
}

/// The instance enables only Vulkan and GL (`GpuContext::instance`), and Linux has no other
/// native backend, so anything that is not GL is Vulkan.
fn backend_of(backend: wgpu::Backend) -> GpuBackend {
    match backend {
        wgpu::Backend::Gl => GpuBackend::Gl,
        _ => GpuBackend::Vulkan,
    }
}

/// The order to try `candidates` in (indices into it) under `pref`: a `Named` match first
/// (case-insensitive substring), then by kind as the preference says, Vulkan before GL, and
/// software rasterizers last. Pure.
pub fn rank(pref: &AdapterPref, candidates: &[AdapterFacts]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    // A stable sort keeps the enumeration order (wgpu's own) among equals.
    order.sort_by_key(|&i| sort_key(pref, &candidates[i]));
    order
}

/// Smaller is tried first: (name miss, software, kind rank, backend rank).
fn sort_key(pref: &AdapterPref, facts: &AdapterFacts) -> (u8, u8, u8, u8) {
    let name_miss = match pref {
        AdapterPref::Named(wanted) => u8::from(!names_match(&facts.name, wanted)),
        _ => 0,
    };
    let software = u8::from(facts.kind == DeviceKind::Cpu);
    let backend = match facts.backend {
        GpuBackend::Vulkan => 0,
        GpuBackend::Gl => 1,
    };
    (name_miss, software, kind_rank(pref, facts.kind), backend)
}

/// The adapter name contains `wanted`, ignoring case. A substring by definition: this is
/// `WGPU_ADAPTER_NAME`'s own contract ("NVIDIA" names "NVIDIA GeForce RTX 5070 Ti").
fn names_match(name: &str, wanted: &str) -> bool {
    name.to_lowercase().contains(&wanted.to_lowercase())
}

fn kind_rank(pref: &AdapterPref, kind: DeviceKind) -> u8 {
    let order: [DeviceKind; 5] = match pref {
        AdapterPref::Integrated => [
            DeviceKind::Integrated,
            DeviceKind::Discrete,
            DeviceKind::Virtual,
            DeviceKind::Other,
            DeviceKind::Cpu,
        ],
        // Auto is wgpu's high-performance order; Named falls back to it after the matches.
        AdapterPref::Auto | AdapterPref::Discrete | AdapterPref::Named(_) => [
            DeviceKind::Discrete,
            DeviceKind::Integrated,
            DeviceKind::Virtual,
            DeviceKind::Other,
            DeviceKind::Cpu,
        ],
    };
    order
        .iter()
        .position(|k| *k == kind)
        .map_or(u8::MAX, |p| p as u8)
}

/// Whether `adapter` survives a first present: open a second connection, map a 1x1
/// transparent surface with an empty input region, present one frame through `adapter`, round
/// trip, and check the connection for a protocol error.
///
/// wgpu binds an instance to one `wl_display` and refuses a surface from another, so the probe
/// cannot use `instance` (the host connection's): it opens its own instance on the throwaway
/// connection and presents through the adapter there whose `AdapterInfo` equals `adapter`'s.
pub fn validate(instance: &wgpu::Instance, adapter: &wgpu::Adapter) -> Result<()> {
    let _ = instance;
    probe::first_present(adapter)
}
