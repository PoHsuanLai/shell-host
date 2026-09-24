//! A surface's scale as live state its document can read ([`crate::handle::use_surface_scale`]).
//!
//! The host learns a surface's scale from the compositor (`wp_fractional_scale_v1`, else
//! `preferred_buffer_scale` or the output's scale) and publishes it here; the surface's
//! `SurfaceHandle` holds a clone, so a component can draw for the device grid it lands on (a
//! design system snapping hairlines at 1.25, 1.5 or 1.75). Shared on the main thread only, like
//! `tick::FrameTicks`, so the sharing is `Rc`.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::scale::Scale;

/// Whether publishing a scale changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Published {
    Changed,
    Unchanged,
}

#[derive(Debug)]
struct ScaleState {
    current: Scale,
    /// How many times the scale has changed; a waiter compares against the count it saw.
    changes: u64,
    waiting: Vec<Waker>,
}

/// One surface's current scale, and the waiters for its next change.
#[derive(Debug, Clone)]
pub struct LiveScale(Rc<RefCell<ScaleState>>);

impl LiveScale {
    /// A surface's scale before the compositor has said anything: `initial`.
    pub fn new(initial: Scale) -> LiveScale {
        LiveScale(Rc::new(RefCell::new(ScaleState {
            current: initial,
            changes: 0,
            waiting: Vec::new(),
        })))
    }

    /// The scale now.
    pub fn get(&self) -> Scale {
        self.0.borrow().current
    }

    /// Changes so far.
    pub fn changes(&self) -> u64 {
        self.0.borrow().changes
    }

    /// Record `scale`, waking every waiter if it differs from the current one. Publishing the
    /// same scale again wakes nobody, so a repeated compositor event costs no render.
    pub fn publish(&self, scale: Scale) -> Published {
        let mut state = self.0.borrow_mut();
        if state.current == scale {
            return Published::Unchanged;
        }
        state.current = scale;
        state.changes += 1;
        state.waiting.drain(..).for_each(Waker::wake);
        Published::Changed
    }

    /// The first change after the `seen`th: resolves with the change count and the scale.
    pub fn changed_after(&self, seen: u64) -> NextScale {
        NextScale {
            live: self.clone(),
            seen,
        }
    }
}

/// The future [`LiveScale::changed_after`] returns.
#[derive(Debug)]
pub struct NextScale {
    live: LiveScale,
    seen: u64,
}

impl Future for NextScale {
    type Output = (u64, Scale);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.live.0.borrow_mut();
        if state.changes > self.seen {
            return Poll::Ready((state.changes, state.current));
        }
        state.waiting.push(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::Wake;

    use super::*;

    #[derive(Default)]
    struct Count(AtomicU32);

    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn only_a_different_scale_is_a_change_and_wakes() {
        let live = LiveScale::new(Scale::ONE);
        let count = Arc::new(Count::default());
        let waker = Waker::from(Arc::clone(&count));
        let mut cx = Context::from_waker(&waker);
        let mut next = std::pin::pin!(live.changed_after(live.changes()));
        assert_eq!(next.as_mut().poll(&mut cx), Poll::Pending);

        assert_eq!(live.publish(Scale::ONE), Published::Unchanged);
        assert_eq!(
            count.0.load(Ordering::SeqCst),
            0,
            "the same scale wakes nobody"
        );

        assert_eq!(live.publish(Scale(180)), Published::Changed);
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(next.poll(&mut cx), Poll::Ready((1, Scale(180))));
        assert_eq!(live.get(), Scale(180));
    }
}
