//! Choosing the adapter: rank the candidates by preference, then validate each by a first
//! present, because a cross-GPU adapter can report the surface as supported and still have its
//! first dmabuf rejected with a fatal protocol error (SPIKE.md S1: RADV under KWin on this
//! machine, `zwp_linux_dmabuf` error 7, "importing the supplied dmabufs failed"). The probe runs
//! on a throwaway Wayland connection so the error kills the probe, not the host.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use crate::config::AdapterPref;
use crate::error::Result;

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
        todo!()
    }

    /// `name (backend, driver)`, as `FrameStats::adapter` reports it.
    pub fn label(&self) -> String {
        todo!()
    }
}

/// The order to try `candidates` in (indices into it) under `pref`: a `Named` match first
/// (case-insensitive substring), then by kind as the preference says, Vulkan before GL, and
/// software rasterizers last. Pure.
pub fn rank(pref: &AdapterPref, candidates: &[AdapterFacts]) -> Vec<usize> {
    todo!()
}

/// Whether `adapter` survives a first present: open a second connection, map a 1x1
/// transparent surface with an empty input region, present one frame through `adapter`, round
/// trip, and check the connection for a protocol error.
pub fn validate(instance: &wgpu::Instance, adapter: &wgpu::Adapter) -> Result<()> {
    todo!()
}
