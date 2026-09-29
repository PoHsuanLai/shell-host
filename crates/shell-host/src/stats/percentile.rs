//! Nearest-rank percentiles of duration samples.

use serde::{Deserialize, Serialize};

/// Nearest-rank percentiles of a set of durations, in microseconds (integers, so the stats
/// stay `Eq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50_us: u32,
    pub p95_us: u32,
    pub max_us: u32,
}

/// Nearest-rank p50, p95 and max of `samples`; all zero when there are none.
pub fn percentiles(samples: &[u32]) -> Percentiles {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = |q_percent: usize| -> u32 {
        match sorted.len() {
            0 => 0,
            n => sorted[(q_percent * n).div_ceil(100).clamp(1, n) - 1],
        }
    };
    Percentiles {
        p50_us: rank(50),
        p95_us: rank(95),
        max_us: rank(100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let cases: &[(&[u32], Percentiles)] = &[
            (&[], Percentiles::default()),
            (
                &[3],
                Percentiles {
                    p50_us: 3,
                    p95_us: 3,
                    max_us: 3,
                },
            ),
            (
                &[4, 1, 3, 2],
                Percentiles {
                    p50_us: 2,
                    p95_us: 4,
                    max_us: 4,
                },
            ),
            (
                &[
                    10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170,
                    180, 190, 200,
                ],
                Percentiles {
                    p50_us: 100,
                    p95_us: 190,
                    max_us: 200,
                },
            ),
        ];
        for (samples, want) in cases {
            assert_eq!(percentiles(samples), *want, "{samples:?}");
        }
    }
}
