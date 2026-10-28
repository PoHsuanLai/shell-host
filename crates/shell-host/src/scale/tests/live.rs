//! A surface's scale as live state: only a different scale is a change.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::Wake;

use std::future::Future;
use std::task::{Context, Poll, Waker};

use crate::scale::{LiveScale, Scale120};

#[derive(Default)]
struct Count(AtomicU32);

impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn only_a_different_scale_is_a_change_and_wakes() {
    let live = LiveScale::new(Scale120::ONE);
    let count = Arc::new(Count::default());
    let waker = Waker::from(Arc::clone(&count));
    let mut cx = Context::from_waker(&waker);
    let mut next = std::pin::pin!(live.changed_after(live.changes()));
    assert_eq!(next.as_mut().poll(&mut cx), Poll::Pending);

    assert!(!live.publish(Scale120::ONE));
    assert_eq!(
        count.0.load(Ordering::SeqCst),
        0,
        "the same scale wakes nobody"
    );

    assert!(live.publish(Scale120(180)));
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(next.poll(&mut cx), Poll::Ready((1, Scale120(180))));
    assert_eq!(live.get(), Scale120(180));
}
