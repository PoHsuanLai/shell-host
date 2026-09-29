//! The samples and counters behind one surface's stats, accumulated between reports.

use std::time::{Duration, Instant};

use super::frame_stats::FrameCounters;
use super::percentile::{Percentiles, percentiles};

/// The samples and counters behind one surface's [`FrameStats`], accumulated between reports.
#[derive(Debug, Clone, Default)]
pub struct StatsWindow {
    counters: FrameCounters,
    work_us: Vec<u32>,
    interval_us: Vec<u32>,
    last_present: Option<Instant>,
}

impl StatsWindow {
    /// Record a presented frame whose work took `work`, presented at `now`.
    pub fn presented(&mut self, work: Duration, now: Instant) {
        self.counters.frames += 1;
        self.counters.commits += 1;
        self.work_us.push(micros(work));
        if let Some(previous) = self.last_present {
            self.interval_us
                .push(micros(now.saturating_duration_since(previous)));
        }
        self.last_present = Some(now);
    }

    /// Record a frame callback.
    pub fn callback(&mut self) {
        self.counters.callbacks += 1;
    }

    /// Record a frame that could not be presented.
    pub fn skipped(&mut self) {
        self.counters.skipped += 1;
    }

    /// Record a frame tick.
    pub fn ticked(&mut self) {
        self.counters.ticks += 1;
    }

    /// Record a render that ran the pre-paint hook.
    pub fn prepainted(&mut self) {
        self.counters.prepaints += 1;
    }

    /// Record a watchdog expiry.
    pub fn watchdog_fired(&mut self) {
        self.counters.watchdog_fired += 1;
    }

    /// The counters so far.
    pub fn counters(&self) -> FrameCounters {
        self.counters
    }

    /// The work and interval percentiles and sample count since the last drain.
    pub fn peek(&self) -> (Percentiles, Percentiles, u32) {
        let samples = u32::try_from(self.work_us.len()).unwrap_or(u32::MAX);
        (
            percentiles(&self.work_us),
            percentiles(&self.interval_us),
            samples,
        )
    }

    /// [`StatsWindow::peek`], then clear the samples (counters stay cumulative). The debug
    /// `stats` command drains, so each report covers the time since the previous one.
    pub fn drain(&mut self) -> (Percentiles, Percentiles, u32) {
        let report = self.peek();
        self.work_us.clear();
        self.interval_us.clear();
        report
    }
}

fn micros(d: Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_separates_work_from_interval() {
        let t0 = Instant::now();
        let mut w = StatsWindow::default();
        w.presented(Duration::from_micros(900), t0);
        w.callback();
        w.presented(
            Duration::from_micros(1100),
            t0 + Duration::from_micros(16_700),
        );
        w.callback();
        w.skipped();
        assert_eq!(w.peek(), w.clone().drain());
        let (work, interval, samples) = w.drain();
        assert_eq!(samples, 2);
        assert_eq!(
            work,
            Percentiles {
                p50_us: 900,
                p95_us: 1100,
                max_us: 1100
            }
        );
        assert_eq!(
            interval,
            Percentiles {
                p50_us: 16_700,
                p95_us: 16_700,
                max_us: 16_700
            }
        );
        assert_eq!(
            w.counters(),
            FrameCounters {
                frames: 2,
                callbacks: 2,
                commits: 2,
                skipped: 1,
                watchdog_fired: 0,
                ticks: 0,
                prepaints: 0
            }
        );
        // Draining clears the samples, not the counters.
        assert_eq!(
            w.drain(),
            (Percentiles::default(), Percentiles::default(), 0)
        );
        assert_eq!(w.counters().frames, 2);
    }

    #[test]
    fn a_stats_window_keeps_work_and_interval_apart_and_drains_only_samples() {
        let t0 = Instant::now();
        let mut window = StatsWindow::default();
        // Three frames, 1 ms of work each, presented a refresh apart.
        for n in 0..3u64 {
            window.presented(
                Duration::from_millis(1),
                t0 + Duration::from_micros(16_667 * n),
            );
            window.callback();
        }
        window.watchdog_fired();
        let (work, interval, samples) = window.peek();
        assert_eq!(samples, 3);
        assert_eq!(work.p95_us, 1000, "work is trigger to present");
        assert_eq!(interval.p50_us, 16_667, "interval is present to present");
        assert_eq!(
            window.drain(),
            (work, interval, samples),
            "drain reports what peek did"
        );
        assert_eq!(
            window.peek(),
            (Percentiles::default(), Percentiles::default(), 0),
            "then starts over"
        );
        assert_eq!(
            window.counters(),
            FrameCounters {
                frames: 3,
                callbacks: 3,
                commits: 3,
                skipped: 0,
                watchdog_fired: 1,
                ticks: 0,
                prepaints: 0,
            },
            "counters are cumulative"
        );
    }
}
