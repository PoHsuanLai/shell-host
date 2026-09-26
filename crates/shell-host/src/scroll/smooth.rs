//! The smooth step (design/11 §11.5.4): a discrete jump (a key, a wheel detent, a command)
//! animated at 1000 px/s, capped at 200 ms, on `--e-out` = `cubic-bezier(.22, .9, .30, 1)`.
//! Settled values (R18), so constants rather than settings keys.

/// The speed of a smooth step, px/s.
const SPEED: f64 = 1000.0;
/// The longest a smooth step takes, seconds.
const MAX_S: f64 = 0.2;
/// `--e-out`'s control points.
const E_OUT: (f64, f64, f64, f64) = (0.22, 0.9, 0.30, 1.0);

/// How long a step of `distance` px animates, seconds.
pub fn duration(distance: f64) -> f64 {
    (distance.abs() / SPEED).min(MAX_S)
}

/// The eased progress at linear progress `p` (clamped to `0..=1`).
pub fn e_out(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    let (x1, y1, x2, y2) = E_OUT;
    let u = solve_u(p, x1, x2);
    bezier(u, y1, y2)
}

/// One coordinate of a unit cubic Bezier with control values `c1`, `c2` at parameter `u`.
fn bezier(u: f64, c1: f64, c2: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * v * v * u * c1 + 3.0 * v * u * u * c2 + u * u * u
}

/// The parameter where the curve's x is `x` (monotonic in `u` for x controls in `0..=1`):
/// bisection, exact to well under a pixel of a 200 ms step.
fn solve_u(x: f64, x1: f64, x2: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if bezier(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_of_11_8() {
        let cases = [
            ("line", 40.0, 0.04),
            ("detent", 60.0, 0.06),
            ("page", 860.0, 0.2),
            ("upward line", -40.0, 0.04),
            ("nothing", 0.0, 0.0),
        ];
        for (name, distance, want) in cases {
            assert!((duration(distance) - want).abs() < 1e-9, "{name}");
        }
    }

    #[test]
    fn e_out_runs_from_0_to_1_monotonically_and_front_loaded() {
        assert!(e_out(0.0).abs() < 1e-9);
        assert!((e_out(1.0) - 1.0).abs() < 1e-9);
        let mut last = 0.0;
        for i in 1..=100 {
            let y = e_out(f64::from(i) / 100.0);
            assert!(y >= last, "not monotonic at {i}");
            last = y;
        }
        assert!(e_out(0.25) > 0.5, "an ease-out is past half at a quarter");
    }
}
