mod strips;

use super::*;

#[test]
fn element_ids_compare_by_the_name_they_were_built_from() {
    assert_eq!(ElementId::new("dock-pill"), ElementId::new("dock-pill"));
    assert_eq!(ElementId::new("dock-pill").as_str(), "dock-pill");
    assert_ne!(ElementId::new("dock-pill"), ElementId::new("dock-hit"));
    // `impl Into<String>` accepts both a borrowed and an owned string.
    assert_eq!(
        ElementId::new("dock-pill"),
        ElementId::new("dock-pill".to_owned())
    );
}

#[test]
fn zero_radius_is_the_rect_itself() {
    let rr = RoundedRect {
        rect: Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        },
        radius: 0,
    };
    assert_eq!(rounded_strips(rr), vec![rr.rect]);
}

#[test]
fn degenerate_rect_has_no_strips() {
    let rr = RoundedRect {
        rect: Rect::ZERO,
        radius: 4,
    };
    assert!(rounded_strips(rr).is_empty());
}

#[test]
fn radius_is_clamped_to_half_the_shorter_side() {
    // 10x6 rect: half-height is 3, so a requested radius of 100 clamps to 3.
    let rr = RoundedRect {
        rect: Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
        },
        radius: 100,
    };
    assert_eq!(rr.effective_radius(), 3);
}

#[test]
fn every_corner_inset_is_within_the_radius() {
    for r in 1..40 {
        for row in 0..r {
            let inset = corner_inset(r, row);
            assert!((0..=r).contains(&inset), "r={r} row={row} inset={inset}");
        }
    }
}
