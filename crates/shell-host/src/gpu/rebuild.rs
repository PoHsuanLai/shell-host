//! How often the host rebuilds its GPU state after a lost device before it gives up: a driver
//! that loses the device again and again will not recover by being asked once more, and a
//! process that exits is restarted by its unit with a fresh driver connection.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The most rebuilds allowed within [`REBUILD_WINDOW`]; the next one is refused.
pub(crate) const MAX_REBUILDS: usize = 3;

/// How far back rebuilds count.
pub(crate) const REBUILD_WINDOW: Duration = Duration::from_secs(60);

/// What to do about a lost device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Rebuild,
    /// The device keeps getting lost: stop the host.
    GiveUp,
}

/// When the host last rebuilt.
#[derive(Debug, Default)]
pub(crate) struct Rebuilds {
    at: VecDeque<Instant>,
}

impl Rebuilds {
    /// Note a rebuild wanted at `now`, and say whether to do it.
    pub(crate) fn record(&mut self, now: Instant) -> Verdict {
        while self
            .at
            .front()
            .is_some_and(|&then| now.saturating_duration_since(then) > REBUILD_WINDOW)
        {
            self.at.pop_front();
        }
        if self.at.len() >= MAX_REBUILDS {
            return Verdict::GiveUp;
        }
        self.at.push_back(now);
        Verdict::Rebuild
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_lost_again_and_again_ends_the_host() {
        let start = Instant::now();
        let mut rebuilds = Rebuilds::default();
        let step = REBUILD_WINDOW / 10;
        let verdicts: Vec<Verdict> = (0..MAX_REBUILDS + 1)
            .map(|n| rebuilds.record(start + step * u32::try_from(n).expect("small")))
            .collect();
        let mut want = vec![Verdict::Rebuild; MAX_REBUILDS];
        want.push(Verdict::GiveUp);
        assert_eq!(verdicts, want);
    }

    #[test]
    fn losses_far_apart_are_each_rebuilt() {
        let start = Instant::now();
        let mut rebuilds = Rebuilds::default();
        for n in 0..(MAX_REBUILDS * 3) {
            let at = start
                + (REBUILD_WINDOW + Duration::from_secs(1)) * u32::try_from(n).expect("small");
            assert_eq!(rebuilds.record(at), Verdict::Rebuild, "loss {n}");
        }
    }
}
