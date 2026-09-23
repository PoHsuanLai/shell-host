//! The CPU presenter: `anyrender_vello_cpu` into `wl_shm` buffers. The fallback when no adapter
//! survives validation (`Backend::Cpu`, or `Backend::Gpu` with nothing usable). No
//! custom-widget textures on this path, as on the headless one.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::shm::slot::SlotPool;

use crate::scale::Size;

/// A surface's shm buffer pool.
#[derive(Debug)]
pub struct ShmBuffers {
    pub(crate) pool: SlotPool,
    pub(crate) buffer: Size,
}

impl ShmBuffers {
    /// A pool for `buffer`-sized ARGB8888 frames.
    pub fn new(shm: &Shm, buffer: Size) -> crate::error::Result<ShmBuffers> {
        todo!()
    }
}
