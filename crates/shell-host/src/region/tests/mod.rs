mod strips;

use super::*;

#[test]
fn a_zero_radius_a_degenerate_rect_and_an_oversized_radius_are_edge_cases() {
    let rect = Rect {
        x: 0,
        y: 0,
        width: 10,
        height: 5,
    };
    let zero_radius = RoundedRect { rect, radius: 0 };
    assert_eq!(
        rounded_strips(zero_radius),
        vec![rect],
        "a zero radius is the rect itself"
    );

    let degenerate = RoundedRect {
        rect: Rect::ZERO,
        radius: 4,
    };
    assert!(
        rounded_strips(degenerate).is_empty(),
        "a degenerate rect has no strips"
    );

    // 10x6 rect: half the shorter side is 3, so a requested radius of 100 clamps to 3.
    let oversized = RoundedRect {
        rect: Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
        },
        radius: 100,
    };
    assert_eq!(
        oversized.effective_radius(),
        3,
        "clamped to half the shorter side"
    );
}
