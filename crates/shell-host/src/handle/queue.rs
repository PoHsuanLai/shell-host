//! The queue a handle pushes requests into.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use calloop::ping::Ping;

use crate::msg::Request;

/// The main-thread queue a `SurfaceHandle` pushes into, and the ping that wakes the loop to
/// drain it. Ops queue while a document is borrowed and apply after `poll()`, before paint.
#[derive(Debug, Clone)]
pub(crate) struct RequestQueue {
    queue: Rc<RefCell<VecDeque<Request>>>,
    wake: Ping,
}

impl RequestQueue {
    /// A queue that pings `wake` on every push.
    pub(crate) fn new(wake: Ping) -> RequestQueue {
        RequestQueue {
            queue: Rc::default(),
            wake,
        }
    }

    /// Queue `request` and wake the loop.
    pub(crate) fn push(&self, request: Request) {
        self.queue.borrow_mut().push_back(request);
        self.wake.ping();
    }

    /// Everything queued so far, oldest first.
    pub(crate) fn drain(&self) -> Vec<Request> {
        self.queue.borrow_mut().drain(..).collect()
    }

    /// The ping this queue wakes the loop with, shared by every waker and shell provider.
    pub(crate) fn ping(&self) -> Ping {
        self.wake.clone()
    }
}

#[cfg(test)]
mod tests {
    use calloop::ping::make_ping;

    use super::*;
    use crate::id::SurfaceId;

    #[test]
    fn the_request_queue_is_first_in_first_out() {
        let (ping, _source) = make_ping().expect("an eventfd for the ping");
        let queue = RequestQueue::new(ping);
        let ids = [3, 1, 2].map(SurfaceId);
        for id in ids {
            queue.push(Request::Close(id));
        }
        let drained: Vec<SurfaceId> = queue
            .drain()
            .into_iter()
            .filter_map(|request| match request {
                Request::Close(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(drained, ids);
        assert!(queue.drain().is_empty());
    }
}
