//! The one clock every document is resolved against (`BaseDocument::resolve(t)`), so CSS
//! transitions on different surfaces stay in step and a test can pass its own instants.

use std::time::Instant;

/// Seconds since the host started, for `resolve(t)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostClock {
    origin: Instant,
}

impl HostClock {
    /// A clock whose zero is `origin` (the host passes `Instant::now()` once, at connect).
    pub fn starting_at(origin: Instant) -> HostClock {
        HostClock { origin }
    }

    /// The document time at `now`, in seconds. An instant before the origin is time zero.
    pub fn seconds_at(self, now: Instant) -> f64 {
        now.saturating_duration_since(self.origin).as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn seconds_count_from_the_origin_and_never_go_negative() {
        let origin = Instant::now();
        let clock = HostClock::starting_at(origin);
        let cases = [
            (Duration::ZERO, 0.0),
            (Duration::from_millis(170), 0.17),
            (Duration::from_secs(3), 3.0),
        ];
        for (after, want) in cases {
            let got = clock.seconds_at(origin + after);
            assert!((got - want).abs() < 1e-9, "{after:?}: {got}");
        }
        let later = HostClock::starting_at(origin + Duration::from_secs(1));
        assert_eq!(later.seconds_at(origin), 0.0);
    }
}
