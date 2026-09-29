//! The decisions behind the keyboard handler: key repeat's timing and when a release stops it.

use std::time::Duration;

use smithay_client_toolkit::seat::keyboard::RepeatInfo;

/// What the compositor's repeat settings ask of the repeat timer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RepeatTiming {
    /// Repeat after `delay`, then every `interval`.
    Every { delay: Duration, interval: Duration },
    /// The compositor turned repeat off.
    Off,
}

/// The timing `info` asks for.
pub(super) fn repeat_timing(info: RepeatInfo) -> RepeatTiming {
    match info {
        RepeatInfo::Repeat { rate, delay } => RepeatTiming::Every {
            delay: Duration::from_millis(u64::from(delay)),
            interval: Duration::from_secs_f64(1.0 / f64::from(rate.get())),
        },
        RepeatInfo::Disable => RepeatTiming::Off,
    }
}

/// What a key release does to the repeat in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepeatAfter {
    /// The repeating key was released: stop.
    Stop,
    /// Another key (or none was repeating): the repeat continues as it was.
    Keep,
}

/// The effect of releasing `released` (a raw key code) while `repeating` is the raw code of the
/// key that repeats, if any.
pub(super) fn repeat_after_release(repeating: Option<u32>, released: u32) -> RepeatAfter {
    match repeating {
        Some(held) if held == released => RepeatAfter::Stop,
        Some(_) | None => RepeatAfter::Keep,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn repeat_settings_become_a_delay_and_an_interval_or_off() {
        let rate = |hz: u32| NonZeroU32::new(hz).expect("non-zero test rate");
        let cases: &[(&str, RepeatInfo, RepeatTiming)] = &[
            (
                "25 per second after 600 ms",
                RepeatInfo::Repeat {
                    rate: rate(25),
                    delay: 600,
                },
                RepeatTiming::Every {
                    delay: Duration::from_millis(600),
                    interval: Duration::from_millis(40),
                },
            ),
            (
                "one per second",
                RepeatInfo::Repeat {
                    rate: rate(1),
                    delay: 0,
                },
                RepeatTiming::Every {
                    delay: Duration::ZERO,
                    interval: Duration::from_secs(1),
                },
            ),
            ("disabled", RepeatInfo::Disable, RepeatTiming::Off),
        ];
        for (name, info, want) in cases {
            assert_eq!(repeat_timing(*info), *want, "{name}");
        }
    }

    #[test]
    fn only_releasing_the_repeating_key_stops_the_repeat() {
        let cases: &[(&str, Option<u32>, u32, RepeatAfter)] = &[
            ("the repeating key", Some(30), 30, RepeatAfter::Stop),
            ("another key", Some(30), 31, RepeatAfter::Keep),
            ("nothing repeating", None, 30, RepeatAfter::Keep),
        ];
        for (name, repeating, released, want) in cases {
            assert_eq!(repeat_after_release(*repeating, *released), *want, "{name}");
        }
    }
}
