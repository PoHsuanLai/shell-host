//! Release velocity (design/11 §11.3.5, §11.5.2): the least-squares slope of the cumulative
//! offset over the samples of the last 80 ms before the fingers lift.
//!
//! Sample times are on the source's own timeline ([`SampleTime`]): palmrest's `t_ns`, or the
//! host's arrival time for compositor axis events. Only differences are ever taken, so the two
//! never need to agree with each other or with the host clock.

use std::collections::VecDeque;
use std::time::Duration;

/// How long a sample is kept.
const KEEP: Duration = Duration::from_millis(100);

/// Fewer samples than this in the window: no momentum (design/11 §11.3.6).
const MIN_SAMPLES: usize = 3;

/// A point on a source's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct SampleTime(pub Duration);

impl SampleTime {
    /// Seconds from `earlier` to `self` (negative if `earlier` is later).
    pub fn secs_since(self, earlier: SampleTime) -> f64 {
        self.0.as_secs_f64() - earlier.0.as_secs_f64()
    }
}

/// The window and held-still rule of a release.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Release {
    pub window: Duration,
    pub held_still: Duration,
}

/// The recent `(time, cumulative offset)` samples of one gesture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Samples(VecDeque<(SampleTime, f64)>);

impl Samples {
    /// Record the cumulative offset `at`, dropping samples older than 100 ms.
    pub fn push(&mut self, at: SampleTime, cumulative: f64) {
        self.0.push_back((at, cumulative));
        while let Some(&(oldest, _)) = self.0.front() {
            if at.secs_since(oldest) > KEEP.as_secs_f64() {
                self.0.pop_front();
            } else {
                break;
            }
        }
    }

    /// The release velocity (px/s, signed) for fingers lifted at `end`: 0 with fewer than three
    /// samples in the window, or when the newest is older than the held-still limit.
    pub fn velocity(&self, end: SampleTime, rule: Release) -> f64 {
        let Some(&(newest, _)) = self.0.back() else {
            return 0.0;
        };
        if end.secs_since(newest) > rule.held_still.as_secs_f64() {
            return 0.0;
        }
        let window = rule.window.as_secs_f64();
        let recent: Vec<(f64, f64)> = self
            .0
            .iter()
            .filter(|(t, _)| end.secs_since(*t) <= window + 1e-9)
            .map(|(t, x)| (t.secs_since(end), *x))
            .collect();
        slope(&recent)
    }
}

/// The least-squares slope of `points`; 0 with fewer than [`MIN_SAMPLES`] or no time spread.
fn slope(points: &[(f64, f64)]) -> f64 {
    if points.len() < MIN_SAMPLES {
        return 0.0;
    }
    let n = points.len() as f64;
    let t_mean = points.iter().map(|(t, _)| t).sum::<f64>() / n;
    let x_mean = points.iter().map(|(_, x)| x).sum::<f64>() / n;
    let (num, den) = points.iter().fold((0.0, 0.0), |(num, den), (t, x)| {
        (
            num + (t - t_mean) * (x - x_mean),
            den + (t - t_mean) * (t - t_mean),
        )
    });
    if den <= f64::EPSILON { 0.0 } else { num / den }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: Release = Release {
        window: Duration::from_millis(80),
        held_still: Duration::from_millis(50),
    };

    fn at(ms: f64) -> SampleTime {
        SampleTime(Duration::from_secs_f64(ms / 1000.0))
    }

    /// A steady `speed` px/s stroke sampled at 120 Hz for `ms`.
    fn stroke(speed: f64, ms: f64) -> (Samples, SampleTime) {
        let mut samples = Samples::default();
        let frames = (ms / 1000.0 * 120.0).round() as u32;
        for i in 0..=frames {
            let t = f64::from(i) / 120.0;
            samples.push(at(t * 1000.0), speed * t);
        }
        (samples, at(ms))
    }

    #[test]
    fn a_steady_stroke_releases_at_its_speed() {
        for speed in [2000.0, -750.0, 12_000.0] {
            let (samples, end) = stroke(speed, 100.0);
            let v = samples.velocity(end, RULE);
            assert!((v - speed).abs() < 1e-6 * speed.abs(), "{speed}: {v}");
        }
    }

    #[test]
    fn a_pause_before_the_lift_releases_nothing() {
        let (samples, end) = stroke(2000.0, 100.0);
        let lifted = SampleTime(end.0 + Duration::from_millis(60));
        assert_eq!(samples.velocity(lifted, RULE), 0.0);
    }

    #[test]
    fn too_few_samples_release_nothing() {
        let mut samples = Samples::default();
        samples.push(at(0.0), 0.0);
        samples.push(at(8.0), 16.0);
        assert_eq!(samples.velocity(at(8.0), RULE), 0.0);
    }

    #[test]
    fn only_the_last_80_ms_count() {
        let mut samples = Samples::default();
        // 50 ms slow, then 80 ms fast: the slope is the fast part's.
        for i in 0..=6 {
            samples.push(at(f64::from(i) * 8.0), f64::from(i));
        }
        let base = 6.0;
        for i in 1..=10 {
            let t = 48.0 + f64::from(i) * 8.0;
            samples.push(at(t), base + 2.0 * f64::from(i) * 8.0);
        }
        let v = samples.velocity(at(128.0), RULE);
        assert!((v - 2000.0).abs() < 1.0, "{v}");
    }
}
