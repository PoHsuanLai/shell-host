//! A beat the loop thread makes on a timer, so a thread that watches the shell can tell that the
//! loop is turning. The timer is a source of the loop itself: it fires only when the loop
//! thread is dispatching, so a loop stuck in a blocking call stops the beat, which an idle loop
//! (nothing to do, asleep in `dispatch`) does not. A watchdog that tells the service manager
//! "alive" only when [`Pulse::beats`] has moved since it last looked therefore restarts the shell
//! after a hang and never during a long, quiet idle.
//!
//! See FINDINGS, HOST-08.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use calloop::LoopHandle;
use calloop::timer::{TimeoutAction, Timer};

use crate::error::{HostError, Result};

/// The shortest beat interval, so a pulse cannot keep the loop spinning.
pub(crate) const SHORTEST_BEAT: Duration = Duration::from_millis(100);

/// How many times the loop thread has beaten. Clones share the count.
#[derive(Debug, Clone, Default)]
pub struct Pulse(Arc<AtomicU64>);

impl Pulse {
    /// The beats so far.
    pub fn beats(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn beat(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// Make `handle`'s loop beat every `every` (at least [`SHORTEST_BEAT`]), counting on the returned
/// pulse.
pub(crate) fn start<D: 'static>(handle: &LoopHandle<'static, D>, every: Duration) -> Result<Pulse> {
    let every = every.max(SHORTEST_BEAT);
    let pulse = Pulse::default();
    let beating = pulse.clone();
    handle
        .insert_source(Timer::from_duration(every), move |_, _, _| {
            beating.beat();
            TimeoutAction::ToDuration(every)
        })
        .map_err(|error| HostError::EventLoop(error.to_string()))?;
    Ok(pulse)
}

#[cfg(test)]
mod tests {
    use calloop::EventLoop;

    use super::*;

    #[test]
    fn the_loop_beats_while_it_dispatches_and_not_otherwise() {
        let mut event_loop = EventLoop::<()>::try_new().expect("a loop");
        let pulse = start(&event_loop.handle(), Duration::from_millis(1)).expect("a pulse");
        assert_eq!(pulse.beats(), 0, "nothing beats before the loop turns");
        // The interval is lifted to the shortest, so wait past it.
        std::thread::sleep(SHORTEST_BEAT + Duration::from_millis(20));
        event_loop
            .dispatch(Some(Duration::ZERO), &mut ())
            .expect("a dispatch");
        assert_eq!(pulse.beats(), 1, "one dispatch, one beat");
        std::thread::sleep(SHORTEST_BEAT * 3);
        assert_eq!(pulse.beats(), 1, "a loop that is not dispatching does not beat");
        event_loop
            .dispatch(Some(Duration::ZERO), &mut ())
            .expect("a dispatch");
        assert_eq!(pulse.beats(), 2, "a late dispatch beats once, not for each missed beat");
    }
}
