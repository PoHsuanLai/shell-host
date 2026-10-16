//! GPU rendering: one adapter, one device and one `vello_hybrid::Renderer` for every surface
//! (the second surface costs ~1.8 MB, so per-surface devices are
//! unnecessary), with a CPU `wl_shm` presenter as the fallback.

pub(crate) mod adapter;
pub(crate) mod context;
pub(crate) mod context_proxy;
pub(crate) mod presenter;
mod probe;
pub(crate) mod raw_handle;
pub(crate) mod rebuild;
pub(crate) mod render;
pub(crate) mod shm;
pub(crate) mod swapchain;

#[cfg(test)]
mod tests;
