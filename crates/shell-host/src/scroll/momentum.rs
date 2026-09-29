//! The glide after a flick (design/11 §11.3.6, §11.5.1): speed and travel as closed forms of
//! the time since release, so a frame at any `now` lands exactly on the curve.
//!
//! All speeds here are magnitudes (px/s, >= 0); the engine carries the direction.

use crate::scroll::config::{ScrollMomentumModel, ScrollSettings};

/// The iOS decay rate per millisecond (design/11 R5).
const IOS_RATE_PER_MS: f64 = 0.998;

/// A deceleration curve with its stop speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    curve: Curve,
    /// Below this speed the glide is over.
    stop: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Curve {
    /// `v' = -a v^b`.
    Drag { a: f64, b: f64 },
    /// `v(t) = v0 e^(k t)`, `k < 0`.
    Decay { k: f64 },
}

impl Glide {
    /// The curve `settings` choose.
    pub fn from_settings(settings: &ScrollSettings) -> Glide {
        let curve = match settings.momentum_model {
            ScrollMomentumModel::MacMouseFix => Curve::Drag {
                a: settings.momentum_a.get(),
                b: settings.momentum_b.get(),
            },
            ScrollMomentumModel::Ios => Curve::Decay {
                k: 1000.0 * IOS_RATE_PER_MS.ln(),
            },
        };
        Glide {
            curve,
            stop: settings.momentum_stop_px_s.get(),
        }
    }

    /// How long a glide from `v0` lasts, seconds.
    pub fn duration(&self, v0: f64) -> f64 {
        if v0 <= self.stop {
            return 0.0;
        }
        match self.curve {
            Curve::Drag { a, b } => (v0.powf(1.0 - b) - self.stop.powf(1.0 - b)) / ((1.0 - b) * a),
            Curve::Decay { k } => (self.stop / v0).ln() / k,
        }
    }

    /// The speed `t` seconds into a glide from `v0` (0 once it is over).
    pub fn speed(&self, v0: f64, t: f64) -> f64 {
        if t >= self.duration(v0) {
            return 0.0;
        }
        match self.curve {
            Curve::Drag { a, b } => {
                let base = v0.powf(1.0 - b) - (1.0 - b) * a * t;
                base.max(0.0).powf(1.0 / (1.0 - b))
            }
            Curve::Decay { k } => v0 * (k * t).exp(),
        }
    }

    /// The distance covered `t` seconds into a glide from `v0`; constant once it is over.
    pub fn travel(&self, v0: f64, t: f64) -> f64 {
        let t = t.clamp(0.0, self.duration(v0));
        match self.curve {
            Curve::Drag { a, b } => {
                let base = (v0.powf(1.0 - b) - (1.0 - b) * a * t).max(0.0);
                (v0.powf(2.0 - b) - base.powf((2.0 - b) / (1.0 - b))) / ((2.0 - b) * a)
            }
            Curve::Decay { k } => v0 * ((k * t).exp() - 1.0) / k,
        }
    }

    /// The whole distance of a glide from `v0`.
    pub fn total(&self, v0: f64) -> f64 {
        self.travel(v0, self.duration(v0))
    }

    /// When a glide from `v0` has covered `d` px, and its speed then; `None` if it stops first.
    pub fn reach(&self, v0: f64, d: f64) -> Option<(f64, f64)> {
        if d <= 0.0 {
            return Some((0.0, v0));
        }
        if d >= self.total(v0) {
            return None;
        }
        match self.curve {
            Curve::Drag { a, b } => {
                let u = v0.powf(2.0 - b) - (2.0 - b) * a * d;
                let w = u.powf((1.0 - b) / (2.0 - b));
                Some((
                    (v0.powf(1.0 - b) - w) / ((1.0 - b) * a),
                    u.powf(1.0 / (2.0 - b)),
                ))
            }
            Curve::Decay { k } => {
                let v = v0 + d * k;
                Some(((v / v0).ln() / k, v))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mmf() -> Glide {
        Glide::from_settings(&ScrollSettings::default())
    }

    #[test]
    fn the_worked_numbers_of_11_3_6() {
        // (v0, total px, duration s) from design/11 §11.3.6.
        let cases = [
            (2000.0, 501.5, 0.976),
            (1000.0, 203.7, 0.772),
            (5000.0, 1650.0, 1.319),
            (12_000.0, 5151.0, 1.749),
        ];
        let glide = mmf();
        for (v0, total, duration) in cases {
            let got = glide.total(v0);
            assert!((got - total).abs() / total < 0.005, "{v0}: travel {got}");
            let got = glide.duration(v0);
            assert!((got - duration).abs() < 0.002, "{v0}: duration {got}");
        }
    }

    #[test]
    fn speed_and_travel_agree_and_the_glide_stops() {
        let glide = mmf();
        let (v0, t, dt) = (2000.0, 0.3, 1e-4);
        let slope = (glide.travel(v0, t + dt) - glide.travel(v0, t - dt)) / (2.0 * dt);
        assert!((slope - glide.speed(v0, t)).abs() < 0.5, "{slope}");
        assert_eq!(glide.speed(v0, 2.0), 0.0);
        assert_eq!(glide.travel(v0, 2.0), glide.total(v0));
        assert_eq!(glide.duration(0.5), 0.0);
    }

    #[test]
    fn reach_inverts_travel() {
        for glide in [
            mmf(),
            Glide::from_settings(&ScrollSettings {
                momentum_model: ScrollMomentumModel::Ios,
                ..ScrollSettings::default()
            }),
        ] {
            let v0 = 3000.0;
            let t = 0.2;
            let d = glide.travel(v0, t);
            let (at, v) = glide.reach(v0, d).expect("reached before stopping");
            assert!((at - t).abs() < 1e-6, "{glide:?}: {at}");
            assert!((v - glide.speed(v0, t)).abs() < 1e-3, "{glide:?}: {v}");
            assert_eq!(glide.reach(v0, glide.total(v0) + 1.0), None);
        }
    }

    #[test]
    fn the_ios_curve_glides_about_twice_as_far() {
        let ios = Glide::from_settings(&ScrollSettings {
            momentum_model: ScrollMomentumModel::Ios,
            ..ScrollSettings::default()
        });
        // ~0.5 s x v0 (design/11 R5).
        let total = ios.total(2000.0);
        assert!((total - 998.5).abs() < 2.0, "{total}");
    }
}
