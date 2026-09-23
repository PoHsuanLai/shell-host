//! Per-surface frame counters and the JSON line `stats` prints.

use std::time::Instant;

/// Counters are cumulative; the timing samples cover the window since the previous `stats`.
#[derive(Default)]
pub struct Stats {
    pub frames: u64,
    pub callbacks: u64,
    pub commits: u64,
    pub skipped: u64,
    pub frame_ms: Vec<f64>,
    pub interval_ms: Vec<f64>,
    pub last_present: Option<Instant>,
}

/// The static facts about a surface that `stats` reports next to the counters.
pub struct Facts<'a> {
    pub surface: &'a str,
    pub scale120: u32,
    pub logical: (u32, u32),
    pub buffer: (u32, u32),
    pub alpha_mode: String,
    pub present_mode: String,
    pub adapter: &'a str,
    pub devices_created: usize,
    pub blur: &'a str,
}

/// The `q`-quantile (0..=1) of `samples`, nearest-rank; 0 when empty.
pub fn percentile(samples: &[f64], q: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut v = samples.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = ((q * v.len() as f64).ceil() as usize).clamp(1, v.len());
    v[rank - 1]
}

/// Resident set size of this process in KiB, from /proc/self/status.
pub fn vm_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmRSS:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        })
        .unwrap_or(0)
}

impl Stats {
    /// Records one presented frame that took `work_ms` from trigger to present.
    pub fn presented(&mut self, work_ms: f64, now: Instant) {
        self.frames += 1;
        self.commits += 1;
        self.frame_ms.push(work_ms);
        if let Some(prev) = self.last_present {
            self.interval_ms.push((now - prev).as_secs_f64() * 1000.0);
        }
        self.last_present = Some(now);
    }

    /// The JSON line for `stats`; drains the timing window.
    pub fn report(&mut self, facts: &Facts<'_>) -> serde_json::Value {
        let v = serde_json::json!({
            "surface": facts.surface,
            "frames": self.frames,
            "callbacks": self.callbacks,
            "commits": self.commits,
            "skipped": self.skipped,
            "frame_ms_p50": percentile(&self.frame_ms, 0.5),
            "frame_ms_p95": percentile(&self.frame_ms, 0.95),
            "frame_ms_max": percentile(&self.frame_ms, 1.0),
            "interval_ms_p50": percentile(&self.interval_ms, 0.5),
            "interval_ms_p95": percentile(&self.interval_ms, 0.95),
            "samples": self.frame_ms.len(),
            "scale": facts.scale120 as f64 / 120.0,
            "scale120": facts.scale120,
            "logical": [facts.logical.0, facts.logical.1],
            "buffer": [facts.buffer.0, facts.buffer.1],
            "alpha_mode": facts.alpha_mode,
            "present_mode": facts.present_mode,
            "adapter": facts.adapter,
            "devices_created": facts.devices_created,
            "blur": facts.blur,
            "vm_rss_kb": vm_rss_kb(),
        });
        self.frame_ms.clear();
        self.interval_ms.clear();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_is_nearest_rank() {
        const CASES: &[(&[f64], f64, f64)] = &[
            (&[], 0.5, 0.0),
            (&[3.0], 0.95, 3.0),
            (&[4.0, 1.0, 3.0, 2.0], 0.5, 2.0),
            (&[4.0, 1.0, 3.0, 2.0], 0.95, 4.0),
        ];
        for (i, (s, q, want)) in CASES.iter().enumerate() {
            assert_eq!(percentile(s, *q), *want, "case {i}");
        }
    }
}
