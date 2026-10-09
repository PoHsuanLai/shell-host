//! Table test for `Scale` and its conversions, against the public API only.

use crate::scale::{Scale, Size, buffer_size};

const CASES: &[(&str, u32, f64)] = &[
    ("1.0x", 120, 1.0),
    ("1.25x", 150, 1.25),
    ("1.5x", 180, 1.5),
    ("2.0x", 240, 2.0),
];

#[test]
fn preferred_scale_wire_values_decode_to_the_expected_multiplier() {
    for &(name, wire, expected) in CASES {
        let scale = Scale::from_wire(wire);
        assert!(
            (scale.as_f64() - expected).abs() < 1e-9,
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
        let got = buffer_size(logical, Scale::from_wire(wire));
        assert_eq!(got, expected, "wire={wire}");
    }
}
