//! A value the host learns from the compositor and publishes to a surface's document as live
//! state: the surface's scale (`use_surface_scale`) and a toplevel's configure state
//! (`use_toplevel_state`). The host publishes, the surface's `SurfaceHandle` holds a clone, and
//! a hook's task waits for the next change. Shared on the main thread only, like
//! `tick::FrameTicks`, so the sharing is `Rc`.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Whether publishing a value changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Published {
    Changed,
    Unchanged,
}

#[derive(Debug)]
struct LiveState<T> {
    current: T,
    /// How many times the value has changed; a waiter compares against the count it saw.
    changes: u64,
    waiting: Vec<Waker>,
}

/// One surface's current value of `T`, and the waiters for its next change.
#[derive(Debug)]
pub(crate) struct Live<T>(Rc<RefCell<LiveState<T>>>);

// Not derived: a derive would require `T: Clone` for what is only an `Rc` clone.
impl<T> Clone for Live<T> {
    fn clone(&self) -> Live<T> {
        Live(Rc::clone(&self.0))
    }
}

impl<T: Copy + PartialEq> Live<T> {
    /// The value before the compositor has said anything: `initial`.
    pub(crate) fn new(initial: T) -> Live<T> {
        Live(Rc::new(RefCell::new(LiveState {
            current: initial,
            changes: 0,
            waiting: Vec::new(),
        })))
    }

    /// The value now.
    pub(crate) fn get(&self) -> T {
        self.0.borrow().current
    }

    /// Changes so far.
    pub(crate) fn changes(&self) -> u64 {
        self.0.borrow().changes
    }

    /// Record `value`, waking every waiter if it differs from the current one. Publishing the
    /// same value again wakes nobody, so a repeated compositor event costs no render.
    pub(crate) fn publish(&self, value: T) -> Published {
        let mut state = self.0.borrow_mut();
        if state.current == value {
            return Published::Unchanged;
        }
        state.current = value;
        state.changes += 1;
        state.waiting.drain(..).for_each(Waker::wake);
        Published::Changed
    }

    /// The first change after the `seen`th: resolves with the change count and the value.
    pub(crate) fn changed_after(&self, seen: u64) -> NextValue<T> {
        NextValue {
            live: self.clone(),
            seen,
        }
    }
}

/// The future [`Live::changed_after`] returns.
#[derive(Debug)]
pub(crate) struct NextValue<T> {
    live: Live<T>,
    seen: u64,
}

impl<T: Copy> Future for NextValue<T> {
    type Output = (u64, T);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.live.0.borrow_mut();
        if state.changes > self.seen {
            return Poll::Ready((state.changes, state.current));
        }
        state.waiting.push(cx.waker().clone());
        Poll::Pending
    }
}
