//! Wakers for Dioxus tasks: a task that becomes ready marks its surface as woken and pings the
//! loop, which polls only the woken documents (zero wakeups when nothing is ready).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::sync::{Arc, Mutex};
use std::task::Waker;

use calloop::ping::Ping;

use crate::id::SurfaceId;

/// The surfaces whose documents have ready tasks, shared between every waker and the loop.
#[derive(Debug, Clone, Default)]
pub struct WokenSet(Arc<Mutex<Vec<SurfaceId>>>);

impl WokenSet {
    /// Mark `id` as woken (idempotent).
    pub fn mark(&self, id: SurfaceId) {
        todo!()
    }

    /// Every woken surface since the last take, each once.
    pub fn take(&self) -> Vec<SurfaceId> {
        todo!()
    }
}

/// The waker for surface `id`'s document: marks it in `woken`, then pings `ping`.
pub fn surface_waker(id: SurfaceId, woken: WokenSet, ping: Ping) -> Waker {
    todo!()
}
