//! How long the host waits before it looks for the palmrest daemon again after it was not there.
//!
//! A missing daemon is looked for on a timer, not only when the pointer enters one of our
//! surfaces: while the session is locked the pointer stays inside the lock surface, and a daemon
//! that restarted during the lock would otherwise never hear `Locked` and would run gesture
//! actions on the lock screen. The waits double from a short first one up to a longest one that
//! depends on the lock: short while locked (the daemon must hear it soon), long otherwise (an
//! absent daemon harms nobody, and an idle host should wake rarely).

use std::time::Duration;

use palmrest_protocol::LockState;

/// The wait after the first miss.
const FIRST: Duration = Duration::from_millis(250);

/// The longest wait while the session is locked.
const LONGEST_LOCKED: Duration = Duration::from_secs(2);

/// The longest wait while it is not: a host without palmrest wakes once in this long.
const LONGEST_UNLOCKED: Duration = Duration::from_secs(30);

/// The most doublings counted: past this the longest wait has been reached anyway.
const MOST_DOUBLINGS: u32 = 16;

/// How many attempts in a row ended with no usable daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Misses(u32);

impl Misses {
    /// One more miss.
    pub(crate) fn more(self) -> Misses {
        Misses(self.0.saturating_add(1))
    }

    /// How long to wait before the next attempt.
    pub(crate) fn wait(self, lock: LockState) -> Duration {
        let longest = match lock {
            LockState::Locked => LONGEST_LOCKED,
            LockState::Unlocked => LONGEST_UNLOCKED,
        };
        let doublings = self.0.saturating_sub(1).min(MOST_DOUBLINGS);
        FIRST.saturating_mul(1 << doublings).min(longest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waits_double_up_to_the_longest_for_the_lock() {
        let ms = Duration::from_millis;
        // `(misses, locked wait, unlocked wait)`.
        let cases: &[(u32, Duration, Duration)] = &[
            (0, ms(250), ms(250)),
            (1, ms(250), ms(250)),
            (2, ms(500), ms(500)),
            (3, ms(1000), ms(1000)),
            (4, ms(2000), ms(2000)),
            (5, ms(2000), ms(4000)),
            (8, ms(2000), ms(30_000)),
            (u32::MAX, ms(2000), ms(30_000)),
        ];
        for (count, locked, unlocked) in cases {
            let misses = (0..(*count).min(40)).fold(Misses::default(), |m, _| m.more());
            let misses = if *count == u32::MAX {
                Misses(u32::MAX)
            } else {
                misses
            };
            assert_eq!(misses.wait(LockState::Locked), *locked, "{count} locked");
            assert_eq!(
                misses.wait(LockState::Unlocked),
                *unlocked,
                "{count} unlocked"
            );
        }
    }
}
