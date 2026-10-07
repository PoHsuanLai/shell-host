//! The wheel events of a surface, from the host to the components that listen (`use_wheel`).
//! Each listener has its own queue, so a slow component loses no event and a fast one takes
//! none from another. The host publishes, the surface's `SurfaceHandle` holds a clone, and a
//! hook's task waits for its next event. Shared on the main thread only, so the sharing is `Rc`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::scroll::wheel::WheelEvent;

#[derive(Debug)]
struct Listener {
    id: u64,
    pending: VecDeque<WheelEvent>,
    waker: Option<Waker>,
}

#[derive(Debug, Default)]
struct Listeners {
    next: u64,
    entries: Vec<Listener>,
}

/// One surface's wheel listeners.
#[derive(Debug, Clone, Default)]
pub(crate) struct WheelFeed(Rc<RefCell<Listeners>>);

impl WheelFeed {
    /// Queue `event` for every listener and wake them. With no listener it costs nothing.
    pub(crate) fn publish(&self, event: WheelEvent) {
        for listener in &mut self.0.borrow_mut().entries {
            listener.pending.push_back(event);
            if let Some(waker) = listener.waker.take() {
                waker.wake();
            }
        }
    }

    /// Start listening: events published from now on queue for the returned listener, until it
    /// is dropped.
    pub(crate) fn listen(&self) -> WheelListener {
        let mut listeners = self.0.borrow_mut();
        listeners.next += 1;
        let id = listeners.next;
        listeners.entries.push(Listener {
            id,
            pending: VecDeque::new(),
            waker: None,
        });
        WheelListener {
            feed: self.clone(),
            id,
        }
    }
}

/// One component's place in a [`WheelFeed`]; leaving it is dropping it.
#[derive(Debug)]
pub(crate) struct WheelListener {
    feed: WheelFeed,
    id: u64,
}

impl WheelListener {
    /// The next event this listener has not seen.
    pub(crate) fn next(&self) -> NextWheel<'_> {
        NextWheel { listener: self }
    }
}

impl Drop for WheelListener {
    fn drop(&mut self) {
        self.feed.0.borrow_mut().entries.retain(|l| l.id != self.id);
    }
}

/// The future [`WheelListener::next`] returns.
#[derive(Debug)]
pub(crate) struct NextWheel<'a> {
    listener: &'a WheelListener,
}

impl Future for NextWheel<'_> {
    type Output = WheelEvent;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<WheelEvent> {
        let mut listeners = self.listener.feed.0.borrow_mut();
        let id = self.listener.id;
        let Some(listener) = listeners.entries.iter_mut().find(|l| l.id == id) else {
            return Poll::Pending;
        };
        match listener.pending.pop_front() {
            Some(event) => Poll::Ready(event),
            None => {
                listener.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::modifiers::Modifiers;
    use crate::scroll::wheel::{WheelMotion, WheelPhase, WheelSource};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::Wake;

    #[derive(Default)]
    struct Wakes(AtomicU32);

    impl Wake for Wakes {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn event(y: f64) -> WheelEvent {
        WheelEvent {
            source: WheelSource::Finger,
            phase: WheelPhase::Changed,
            by: WheelMotion { x: 0.0, y },
            at: Point::default(),
            held: Modifiers::default(),
        }
    }

    fn poll(listener: &WheelListener, wakes: &Arc<Wakes>) -> Poll<WheelEvent> {
        let waker = Waker::from(Arc::clone(wakes));
        let mut cx = Context::from_waker(&waker);
        Pin::new(&mut listener.next()).poll(&mut cx)
    }

    #[test]
    fn each_listener_gets_every_event_in_order_and_is_woken() {
        let feed = WheelFeed::default();
        let (a, b) = (feed.listen(), feed.listen());
        let (wa, wb) = (Arc::new(Wakes::default()), Arc::new(Wakes::default()));
        assert_eq!(poll(&a, &wa), Poll::Pending);
        assert_eq!(poll(&b, &wb), Poll::Pending);
        feed.publish(event(1.0));
        feed.publish(event(2.0));
        assert_eq!(
            (wa.0.load(Ordering::SeqCst), wb.0.load(Ordering::SeqCst)),
            (1, 1)
        );
        assert_eq!(poll(&a, &wa), Poll::Ready(event(1.0)));
        assert_eq!(poll(&a, &wa), Poll::Ready(event(2.0)));
        assert_eq!(poll(&b, &wb), Poll::Ready(event(1.0)));
    }

    #[test]
    fn a_dropped_listener_stops_queueing_and_a_new_one_starts_empty() {
        let feed = WheelFeed::default();
        drop(feed.listen());
        feed.publish(event(1.0));
        assert!(
            feed.0.borrow().entries.is_empty(),
            "nothing queues for nobody"
        );
        let late = feed.listen();
        assert_eq!(poll(&late, &Arc::new(Wakes::default())), Poll::Pending);
    }
}
