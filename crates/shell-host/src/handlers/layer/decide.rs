//! The decisions behind the layer-surface configure handler, as values a table can assert on.

use crate::layer::{LayerExtent, LayerSize};
use crate::region::Px;
use crate::scale::Size;

/// The logical size to record from a `zwlr_layer_surface_v1.configure`: on each axis, the
/// compositor's value when it sent one, or what `requested` asked for on that axis when it
/// sent 0 — the wire's "you decide" for a fixed-size axis (`Extent::Px`). A stretching axis
/// (`Extent::Fill`) has no independent size to fall back to; a 0 there means the compositor has
/// not resolved the stretch yet (a compositor should not normally send this for an anchored,
/// opposite-edges-stretching axis, but nothing about the wire forbids it).
pub fn configured_size(requested: LayerSize, wire: (u32, u32)) -> Size {
    let axis = |extent: LayerExtent, got: u32| -> Px {
        if got != 0 {
            return got as Px;
        }
        match extent {
            LayerExtent::Px(px) => px as Px,
            LayerExtent::Fill => 0,
        }
    };
    Size {
        width: axis(requested.width, wire.0),
        height: axis(requested.height, wire.1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_size_matches_the_table() {
        let bar = LayerSize {
            width: LayerExtent::Fill,
            height: LayerExtent::Px(32),
        };
        let dock = LayerSize {
            width: LayerExtent::Px(740),
            height: LayerExtent::Px(96),
        };
        let cases: &[(&str, LayerSize, (u32, u32), Size)] = &[
            (
                "compositor echoes the requested size",
                dock,
                (740, 96),
                Size {
                    width: 740,
                    height: 96,
                },
            ),
            (
                "fixed axis falls back to what was requested when the compositor sends 0",
                dock,
                (0, 0),
                Size {
                    width: 740,
                    height: 96,
                },
            ),
            (
                "compositor resolves the stretching axis",
                bar,
                (1280, 0),
                Size {
                    width: 1280,
                    height: 32,
                },
            ),
            (
                "a fixed axis is still honoured next to a stretching one",
                bar,
                (1280, 40),
                Size {
                    width: 1280,
                    height: 40,
                },
            ),
            (
                "an unresolved stretch reports 0, not the other axis's fixed size",
                bar,
                (0, 0),
                Size {
                    width: 0,
                    height: 32,
                },
            ),
        ];
        for (name, requested, wire, want) in cases {
            assert_eq!(configured_size(*requested, *wire), *want, "{name}");
        }
    }
}
