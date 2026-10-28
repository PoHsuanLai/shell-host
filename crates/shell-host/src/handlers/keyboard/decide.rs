//! The decisions behind the keyboard handler: key repeat's timing and when a release stops it.

use std::time::Duration;

use smithay_client_toolkit::seat::keyboard::RepeatInfo;

/// The fastest key repeat accepted, in keys per second: a rate beyond this is a compositor bug,
/// and would fire the repeat timer back to back and flood the focused document with keys.
const MAX_REPEAT_RATE: u32 = 1000;

/// The key repeat the compositor asks of the repeat timer: after `delay`, then every
/// `interval`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RepeatRate {
    pub delay: Duration,
    pub interval: Duration,
}

/// The rate `info` asks for; `None` when the compositor turned repeat off.
pub(super) fn repeat_rate(info: RepeatInfo) -> Option<RepeatRate> {
    match info {
        RepeatInfo::Repeat { rate, delay } => Some(RepeatRate {
            delay: Duration::from_millis(u64::from(delay)),
            interval: Duration::from_secs_f64(1.0 / f64::from(rate.get().min(MAX_REPEAT_RATE))),
        }),
        RepeatInfo::Disable => None,
    }
}

/// Whether releasing `released` (a raw key code) stops the repeat in progress, where `repeating`
/// is the raw code of the key that repeats, if any. Releasing another key (or none repeating)
/// leaves the repeat as it was.
pub(super) fn stops_repeat(repeating: Option<u32>, released: u32) -> bool {
    repeating == Some(released)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn repeat_settings_become_a_delay_and_an_interval_or_off() {
        let rate = |hz: u32| NonZeroU32::new(hz).expect("non-zero test rate");
        let cases: &[(&str, RepeatInfo, Option<RepeatRate>)] = &[
            (
                "25 per second after 600 ms",
                RepeatInfo::Repeat {
                    rate: rate(25),
                    delay: 600,
                },
                Some(RepeatRate {
                    delay: Duration::from_millis(600),
                    interval: Duration::from_millis(40),
                }),
            ),
            (
                "one per second",
                RepeatInfo::Repeat {
                    rate: rate(1),
                    delay: 0,
                },
                Some(RepeatRate {
                    delay: Duration::ZERO,
                    interval: Duration::from_secs(1),
                }),
            ),
            (
                "a rate beyond any keyboard is held to the fastest accepted",
                RepeatInfo::Repeat {
                    rate: rate(u32::MAX),
                    delay: 0,
                },
                Some(RepeatRate {
                    delay: Duration::ZERO,
                    interval: Duration::from_millis(1),
                }),
            ),
            ("disabled", RepeatInfo::Disable, None),
        ];
        for (name, info, want) in cases {
            assert_eq!(repeat_rate(*info), *want, "{name}");
        }
    }

    #[test]
    fn only_releasing_the_repeating_key_stops_the_repeat() {
        let cases: &[(&str, Option<u32>, u32, bool)] = &[
            ("the repeating key", Some(30), 30, true),
            ("another key", Some(30), 31, false),
            ("nothing repeating", None, 30, false),
        ];
        for (name, repeating, released, want) in cases {
            assert_eq!(stops_repeat(*repeating, *released), *want, "{name}");
        }
    }
}
