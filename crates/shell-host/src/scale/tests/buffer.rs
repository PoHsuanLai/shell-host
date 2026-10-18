//! Table test for `Scale120` and its conversions, against the public API only.

use crate::region::Px;
use crate::scale::{MAX_SURFACE_EXTENT, Scale120, Size, buffer_size, wire_extent};

const CASES: &[(&str, u32, f64)] = &[
    ("1.0x", 120, 1.0),
    ("1.25x", 150, 1.25),
    ("1.5x", 180, 1.5),
    ("2.0x", 240, 2.0),
];

#[test]
fn preferred_scale_wire_values_decode_to_the_expected_multiplier() {
    for &(name, wire, expected) in CASES {
        let scale = Scale120::from_wire(wire);
        assert!(
            (scale.factor() - expected).abs() < 1e-9,
            "{name}: wire={wire}"
        );
    }
}

#[test]
fn buffer_size_rounds_half_away_from_zero_at_each_pinned_scale() {
    let logical = Size {
        width: 201,
        height: 33,
    };
    let table: &[(u32, Size)] = &[
        (
            120,
            Size {
                width: 201,
                height: 33,
            },
        ),
        (
            150,
            Size {
                width: 251,
                height: 41,
            },
        ), // 251.25 -> 251, 41.25 -> 41
        (
            180,
            Size {
                width: 302,
                height: 50,
            },
        ), // 301.5 -> 302, 49.5 -> 50 (half away from zero)
        (
            240,
            Size {
                width: 402,
                height: 66,
            },
        ),
    ];
    for &(wire, expected) in table {
        let got = buffer_size(logical, Scale120::from_wire(wire));
        assert_eq!(got, expected, "wire={wire}");
    }
}

#[test]
fn a_scale_from_the_wire_is_kept_between_a_quarter_and_ten_times() {
    let cases: &[(&str, u32, u32)] = &[
        ("zero is the smallest, not a zero-size buffer", 0, 30),
        ("1x", 120, 120),
        ("1.5x", 180, 180),
        ("the largest", 1200, 1200),
        ("a lie", 4_000_000_000, 1200),
    ];
    for &(name, wire, want) in cases {
        assert_eq!(Scale120::from_wire(wire), Scale120(want), "{name}");
    }
}

#[test]
fn a_whole_scale_factor_is_kept_between_one_and_ten() {
    let cases: &[(&str, i32, u32)] = &[
        ("negative", -4, 120),
        ("zero", 0, 120),
        ("1x", 1, 120),
        ("2x", 2, 240),
        ("the largest", 10, 1200),
        ("a factor whose 120ths overflow", i32::MAX, 1200),
    ];
    for &(name, factor, want) in cases {
        assert_eq!(
            Scale120::from_integer_factor(factor),
            Scale120(want),
            "{name}"
        );
    }
}

#[test]
fn a_size_from_the_wire_is_kept_within_what_a_device_can_hold() {
    let cases: &[(&str, u32, Px)] = &[
        ("unset", 0, 0),
        ("an ordinary output", 1920, 1920),
        ("the largest", 8192, 8192),
        ("one past it", 8193, 8192),
        ("a lie", u32::MAX, 8192),
    ];
    for &(name, wire, want) in cases {
        assert_eq!(wire_extent(wire), want, "{name}");
    }
}

#[test]
fn a_buffer_never_exceeds_the_largest_size_whatever_the_scale() {
    let logical = Size {
        width: 8192,
        height: 600,
    };
    let got = buffer_size(logical, Scale120::from_wire(1200));
    assert_eq!(
        got,
        Size {
            width: MAX_SURFACE_EXTENT,
            height: 6000,
        },
        "only the side past the limit is cut"
    );
}

#[test]
fn only_a_size_with_both_sides_has_an_area() {
    let size = |width, height| Size { width, height };
    let cases = [
        ("both", size(1280, 32), true),
        ("an unresolved stretch", size(0, 32), false),
        ("no height", size(1280, 0), false),
        ("nothing", size(0, 0), false),
        ("negative", size(-5, 32), false),
    ];
    for (name, size, want) in cases {
        assert_eq!(size.has_area(), want, "{name}");
    }
}
