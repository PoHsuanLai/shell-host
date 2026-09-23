//! Wakers for Dioxus tasks: a task that becomes ready marks its surface as woken and pings the
//! loop, which polls only the woken documents (zero wakeups when nothing is ready).

use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Wake, Waker};

use calloop::ping::Ping;

use crate::id::SurfaceId;

/// The surfaces whose documents have ready tasks, shared between every waker and the loop.
#[derive(Debug, Clone, Default)]
pub struct WokenSet(Arc<Mutex<Vec<SurfaceId>>>);

impl WokenSet {
    /// Mark `id` as woken (idempotent).
    pub fn mark(&self, id: SurfaceId) {
        let mut woken = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !woken.contains(&id) {
            woken.push(id);
        }
    }

    /// Every woken surface since the last take, each once.
    pub fn take(&self) -> Vec<SurfaceId> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// The waker for one surface's document: marks it woken, then pings the loop.
struct SurfaceWaker {
    id: SurfaceId,
    woken: WokenSet,
    ping: Ping,
}

impl Wake for SurfaceWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.mark(self.id);
        self.ping.ping();
    }
}

/// The waker for surface `id`'s document: marks it in `woken`, then pings `ping`.
pub fn surface_waker(id: SurfaceId, woken: WokenSet, ping: Ping) -> Waker {
    Waker::from(Arc::new(SurfaceWaker { id, woken, ping }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marking_is_idempotent_and_take_drains_once() {
        let set = WokenSet::default();
        set.mark(SurfaceId(1));
        set.mark(SurfaceId(1));
        set.mark(SurfaceId(2));
        let mut got = set.take();
        got.sort();
        assert_eq!(got, vec![SurfaceId(1), SurfaceId(2)]);
        assert_eq!(set.take(), Vec::new());
    }

    #[test]
    fn waking_marks_the_surface_and_pings() {
        let (ping, mut source) = calloop::ping::make_ping().unwrap();
        let woken = WokenSet::default();
        let waker = surface_waker(SurfaceId(7), woken.clone(), ping);
        waker.wake();
        assert_eq!(woken.take(), vec![SurfaceId(7)]);

        let mut loop_ = calloop::EventLoop::<bool>::try_new().unwrap();
        loop_
            .handle()
            .insert_source(&mut source, |_, _, fired: &mut bool| *fired = true)
            .unwrap();
        let mut fired = false;
        loop_
            .dispatch(std::time::Duration::from_millis(200), &mut fired)
            .unwrap();
        assert!(fired, "waking the surface waker should ping the loop");
    }
}
