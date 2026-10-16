//! When a lock surface that failed is made again, and when the host stops trying. A locked
//! output without a lock surface shows the compositor's solid colour and takes no password, so
//! it is remade with a growing pause. If it is the only output there is no way to unlock at all,
//! and the host exits instead, so that whatever started it starts it again and locks again.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::id::OutputId;

/// The pause before the first retry.
pub(crate) const FIRST_DELAY: Duration = Duration::from_millis(250);

/// The longest pause between retries.
pub(crate) const MAX_DELAY: Duration = Duration::from_secs(8);

/// How many failures in a row on the only output end the host.
pub(crate) const SINGLE_OUTPUT_FAILURES: u32 = 4;

/// What to do about a failed lock surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retry {
    /// Make it again after this pause.
    After(Duration),
    /// Stop the host.
    GiveUp,
}

/// What to do after the `failures`th failure in a row (counting this one) of an output's lock
/// surface, while the lock covers `outputs` outputs.
pub(crate) fn retry(failures: u32, outputs: usize) -> Retry {
    if outputs <= 1 && failures >= SINGLE_OUTPUT_FAILURES {
        return Retry::GiveUp;
    }
    let doublings = failures.saturating_sub(1).min(8);
    Retry::After((FIRST_DELAY * 2u32.pow(doublings)).min(MAX_DELAY))
}

/// Consecutive failures per output, until its surface presents a frame.
#[derive(Debug, Default)]
pub(crate) struct Failures(BTreeMap<OutputId, u32>);

impl Failures {
    /// Note a failure on `output`; how many in a row that makes.
    pub(crate) fn failed(&mut self, output: OutputId) -> u32 {
        let count = self.0.entry(output).or_insert(0);
        *count = count.saturating_add(1);
        *count
    }

    /// `output` has a working surface (or none is wanted any more).
    pub(crate) fn recovered(&mut self, output: OutputId) {
        self.0.remove(&output);
    }

    /// Whether no output has failed.
    pub(crate) fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONLY: usize = 1;
    const SEVERAL: usize = 3;

    #[test]
    fn the_pause_doubles_and_stops_growing() {
        let ms = |ms| Retry::After(Duration::from_millis(ms));
        let cases: &[(&str, u32, usize, Retry)] = &[
            ("first", 1, SEVERAL, ms(250)),
            ("second", 2, SEVERAL, ms(500)),
            ("third", 3, SEVERAL, ms(1000)),
            ("sixth", 6, SEVERAL, ms(8000)),
            ("many outputs never give up", 100, SEVERAL, ms(8000)),
            ("the only output, before the limit", 3, ONLY, ms(1000)),
            (
                "the only output, at the limit",
                SINGLE_OUTPUT_FAILURES,
                ONLY,
                Retry::GiveUp,
            ),
            ("the only output, past it", 99, ONLY, Retry::GiveUp),
        ];
        for (name, failures, outputs, want) in cases {
            assert_eq!(retry(*failures, *outputs), *want, "{name}");
        }
    }

    #[test]
    fn failures_count_per_output_until_a_surface_recovers() {
        let (a, b) = (OutputId(1), OutputId(2));
        let mut failures = Failures::default();
        assert!(failures.is_clear());
        assert_eq!(failures.failed(a), 1);
        assert_eq!(failures.failed(a), 2);
        assert_eq!(failures.failed(b), 1, "another output counts on its own");
        failures.recovered(a);
        assert_eq!(failures.failed(a), 1, "a recovered output starts again");
        failures.recovered(a);
        failures.recovered(b);
        assert!(failures.is_clear());
    }
}
