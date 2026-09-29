//! GPU rendering: one adapter, one device and one `vello_hybrid::Renderer` for every surface
//! (the second surface costs ~1.8 MB, so per-surface devices are
//! unnecessary), with a CPU `wl_shm` presenter as the fallback.

pub mod adapter;
mod block_on;
pub mod context;
pub mod context_proxy;
pub mod presenter;
mod probe;
pub mod raw_handle;
pub mod render;
pub mod shm;
pub mod swapchain;
