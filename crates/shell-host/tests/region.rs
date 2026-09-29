//! `rounded_strips` checked against a pixel-raster oracle: every strip pixel must lie inside
//! the true rounded rectangle, and the strips must cover at least 99% of it. A second check
//! sweeps many radii and confirms no corner strip is ever inset by more than the radius.

use shell_host::region::{Px, Rect, RoundedRect, rounded_strips};

/// Is the pixel with top-left corner `(px, py)` (so centre `(px+0.5, py+0.5)`) inside the
/// rounded rectangle `rr`? This is an independent rasteriser: it tests distance-from-corner-
/// centre directly, rather than reusing `rounded_strips`'s row-inset arithmetic.
fn pixel_inside(rr: RoundedRect, px: Px, py: Px) -> bool {
    let Rect {
        x,
        y,
        width,
        height,
    } = rr.rect;
    if px < x || py < y || px >= x + width || py >= y + height {
        return false;
    }
    let r = rr.effective_radius();
    if r <= 0 {
        return true;
    }
    let (lx, ly) = (px - x, py - y); // local coords, 0-based
    let cx = (lx + lx + 1) as f64 / 2.0; // pixel centre
    let cy = (ly + ly + 1) as f64 / 2.0;

    // Which corner's circle (if any) governs this pixel.
    let (corner_cx, in_corner_x) = if lx < r {
        (r as f64, true)
    } else if lx >= width - r {
        ((width - r) as f64, true)
    } else {
        (0.0, false)
    };
    let (corner_cy, in_corner_y) = if ly < r {
        (r as f64, true)
    } else if ly >= height - r {
        ((height - r) as f64, true)
    } else {
        (0.0, false)
    };
    if !(in_corner_x && in_corner_y) {
        return true; // not in a corner square: the rounded rect is full there
    }
    let dx = cx - corner_cx;
    let dy = cy - corner_cy;
    dx * dx + dy * dy <= (r as f64) * (r as f64)
}

fn strip_pixels(rect: Rect) -> impl Iterator<Item = (Px, Px)> {
    (rect.y..rect.y + rect.height)
        .flat_map(move |py| (rect.x..rect.x + rect.width).map(move |px| (px, py)))
}

const CASES: &[(Px, Px, Px, Px, Px)] = &[
    // (x, y, width, height, radius)
    (0, 0, 20, 20, 4),
    (0, 0, 200, 32, 8),   // roughly a bar
    (0, 0, 33, 17, 6),    // odd dimensions
    (0, 0, 100, 100, 50), // radius clamps to a full circle
    (5, 5, 40, 40, 1),    // radius of 1, the smallest nontrivial corner
    (0, 0, 10, 60, 5),    // tall and narrow
];

#[test]
fn every_strip_pixel_is_inside_the_rounded_rect() {
    for &(x, y, width, height, radius) in CASES {
        let rr = RoundedRect {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            radius,
        };
        for strip in rounded_strips(rr) {
            for (px, py) in strip_pixels(strip) {
                assert!(
                    pixel_inside(rr, px, py),
                    "case {rr:?}: strip pixel ({px},{py}) is outside the rounded rect"
                );
            }
        }
    }
}

#[test]
fn strip_coverage_is_at_least_99_percent() {
    for &(x, y, width, height, radius) in CASES {
        let rr = RoundedRect {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            radius,
        };
        let total_inside: usize = (y..y + height)
            .flat_map(|py| (x..x + width).map(move |px| (px, py)))
            .filter(|&(px, py)| pixel_inside(rr, px, py))
            .count();
        let covered: usize = rounded_strips(rr)
            .into_iter()
            .map(|s| (s.width * s.height) as usize)
            .sum();
        assert!(
            covered <= total_inside,
            "case {rr:?}: strips cover more than the oracle says exists"
        );
        let coverage = covered as f64 / total_inside as f64;
        assert!(
            coverage >= 0.99,
            "case {rr:?}: coverage {coverage:.4} < 0.99 ({covered}/{total_inside})"
        );
    }
}

#[test]
fn corner_strips_never_exceed_the_radius() {
    for width in [4, 9, 20, 33, 64] {
        for height in [4, 9, 20, 33, 64] {
            for radius in 0..=32 {
                let rr = RoundedRect {
                    rect: Rect {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                    radius,
                };
                let r = rr.effective_radius();
                for strip in rounded_strips(rr) {
                    let left_inset = strip.x - rr.rect.x;
                    let right_inset = (rr.rect.x + rr.rect.width) - (strip.x + strip.width);
                    assert!(
                        (0..=r).contains(&left_inset) && (0..=r).contains(&right_inset),
                        "case w={width} h={height} r={radius}: strip {strip:?} insets {left_inset}/{right_inset} exceed radius {r}"
                    );
                }
            }
        }
    }
}

#[test]
fn strips_never_extend_outside_the_rect() {
    for &(x, y, width, height, radius) in CASES {
        let rr = RoundedRect {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            radius,
        };
        for strip in rounded_strips(rr) {
            assert!(strip.x >= x && strip.y >= y);
            assert!(strip.x + strip.width <= x + width);
            assert!(strip.y + strip.height <= y + height);
        }
    }
}
