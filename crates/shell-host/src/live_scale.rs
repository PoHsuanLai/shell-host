//! A surface's scale as live state its document can read ([`crate::handle::use_surface_scale`]).
//!
//! The host learns a surface's scale from the compositor (`wp_fractional_scale_v1`, else
//! `preferred_buffer_scale` or the output's scale) and publishes it here; the surface's
//! `SurfaceHandle` holds a clone, so a component can draw for the device grid it lands on (a
//! design system snapping hairlines at 1.25, 1.5 or 1.75). The mechanism is [`crate::live`]'s,
//! shared with the toplevel state.
//!
//! See FINDINGS F43.

use crate::live::Live;
use crate::scale::Scale;

/// One surface's current scale, and the waiters for its next change.
pub(crate) type LiveScale = Live<Scale>;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::Wake;

    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::live::Published;

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
