//! Layer-shell surfaces: what a shell asks for when it spawns a bar, a dock or an overlay, and
//! the `zwlr_layer_surface_v1` wire values each choice becomes.

use smithay_client_toolkit::shell::wlr_layer::{
    Anchor as WireAnchor, KeyboardInteractivity, Layer as WireLayer,
};

use crate::blur::BlurSpec;
use crate::geometry::Margin;
use crate::id::OutputId;
use crate::region::RegionSpec;

/// Everything `HostCtx::spawn_layer` needs besides the root component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerConfig {
    /// The layer-shell namespace (`sill-bar`, `sill-dock`); compositors key rules on it, and the
    /// debug commands address the surface by it.
    pub namespace: String,
    pub layer: Layer,
    pub anchor: Anchor,
    pub size: LayerSize,
    pub exclusive: ExclusiveZone,
    pub margin: Margin,
    pub keyboard: KeyboardMode,
    pub output: OutputTarget,
    pub blur: BlurSpec,
    pub input: RegionSpec,
}

/// Which stacking layer a surface lives in, bottom to top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Background,
    /// Below normal windows: a dock that windows may cover (the occlusion check of render
    /// acceptance item c).
    Bottom,
    Top,
    Overlay,
}

/// The edges a layer surface is anchored to: a set of up to four edges. Hand-written rather than
/// `bitflags` because `bitflags` is not in the pinned block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Anchor(u8);

impl Anchor {
    /// Anchored to nothing: centred on the output.
    pub const NONE: Anchor = Anchor(0);
    pub const TOP: Anchor = Anchor(1);
    pub const BOTTOM: Anchor = Anchor(2);
    pub const LEFT: Anchor = Anchor(4);
    pub const RIGHT: Anchor = Anchor(8);
    /// All four edges.
    pub const ALL: Anchor = Anchor(15);

    /// Both sets of edges.
    pub const fn union(self, other: Anchor) -> Anchor {
        Anchor(self.0 | other.0)
    }

    /// Whether every edge of `other` is in `self`.
    pub const fn contains(self, other: Anchor) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Anchor {
    type Output = Anchor;

    fn bitor(self, other: Anchor) -> Anchor {
        self.union(other)
    }
}

/// One dimension of a layer surface's requested size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extent {
    /// Stretch between the two opposite anchors (wire value 0; the protocol requires both
    /// opposite edges to be anchored, or the compositor raises an error).
    Fill,
    /// A fixed size in logical pixels.
    Px(u32),
}

/// The size a layer surface asks for, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerSize {
    pub width: Extent,
    pub height: Extent,
}

/// How a layer surface interacts with other surfaces' exclusive zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExclusiveZone {
    /// Extend under other panels (wire -1).
    Ignore,
    /// Keep clear of other panels but reserve nothing (wire 0).
    Auto,
    /// Reserve this many logical pixels along the anchored edge (a bar's 32).
    Reserve(u32),
}

/// When a layer surface receives keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardMode {
    /// Never (bar, dock). A grabbing popup opened from such a surface switches it to `OnDemand`
    /// until the popup closes (plan: shell-host input rule; design/13 §13.3.8), except on KWin,
    /// which focuses the popup itself (`popup_keyboard`).
    None,
    /// Takes all keyboard input while mapped (launcher, switcher).
    Exclusive,
    /// Focusable like a window.
    OnDemand,
}

/// Which output a layer surface goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputTarget {
    /// Let the compositor choose (usually the focused output).
    CompositorPicks,
    /// This output.
    Output(OutputId),
}

/// The sctk value for `layer`.
pub fn to_wire_layer(layer: Layer) -> WireLayer {
    match layer {
        Layer::Background => WireLayer::Background,
        Layer::Bottom => WireLayer::Bottom,
        Layer::Top => WireLayer::Top,
        Layer::Overlay => WireLayer::Overlay,
    }
}

/// The sctk value for `anchor`.
pub fn to_wire_anchor(anchor: Anchor) -> WireAnchor {
    [
        (Anchor::TOP, WireAnchor::TOP),
        (Anchor::BOTTOM, WireAnchor::BOTTOM),
        (Anchor::LEFT, WireAnchor::LEFT),
        (Anchor::RIGHT, WireAnchor::RIGHT),
    ]
    .into_iter()
    .filter(|&(ours, _)| anchor.contains(ours))
    .fold(WireAnchor::empty(), |wire, (_, bit)| wire | bit)
}

/// The `set_size` arguments for `size`.
pub fn to_wire_size(size: LayerSize) -> (u32, u32) {
    let wire = |extent| match extent {
        Extent::Fill => 0,
        Extent::Px(px) => px,
    };
    (wire(size.width), wire(size.height))
}

/// The `set_exclusive_zone` argument for `zone`.
pub fn to_wire_exclusive(zone: ExclusiveZone) -> i32 {
    match zone {
        ExclusiveZone::Ignore => -1,
        ExclusiveZone::Auto => 0,
        ExclusiveZone::Reserve(px) => i32::try_from(px).unwrap_or(i32::MAX),
    }
}

/// The sctk value for `mode`.
pub fn to_wire_keyboard(mode: KeyboardMode) -> KeyboardInteractivity {
    match mode {
        KeyboardMode::None => KeyboardInteractivity::None,
        KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
        KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_maps_to_its_wire_layer() {
        let cases = [
            (Layer::Background, WireLayer::Background),
            (Layer::Bottom, WireLayer::Bottom),
            (Layer::Top, WireLayer::Top),
            (Layer::Overlay, WireLayer::Overlay),
        ];
        for (ours, wire) in cases {
            assert_eq!(to_wire_layer(ours), wire, "{ours:?}");
        }
    }

    #[test]
    fn anchors_map_edge_by_edge() {
        let cases = [
            ("none", Anchor::NONE, WireAnchor::empty()),
            (
                "bar",
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                WireAnchor::TOP | WireAnchor::LEFT | WireAnchor::RIGHT,
            ),
            ("dock", Anchor::BOTTOM, WireAnchor::BOTTOM),
            ("all", Anchor::ALL, WireAnchor::all()),
        ];
        for (name, ours, wire) in cases {
            assert_eq!(to_wire_anchor(ours), wire, "{name}");
        }
    }

    #[test]
    fn anchor_set_operations() {
        let bar = Anchor::TOP | Anchor::LEFT | Anchor::RIGHT;
        assert!(bar.contains(Anchor::TOP));
        assert!(bar.contains(Anchor::LEFT | Anchor::RIGHT));
        assert!(!bar.contains(Anchor::BOTTOM));
        assert!(Anchor::ALL.contains(bar));
        assert_eq!(Anchor::default(), Anchor::NONE);
    }

    #[test]
    fn sizes_zones_and_keyboard_modes_map_to_wire_values() {
        let sizes = [
            (
                LayerSize {
                    width: Extent::Fill,
                    height: Extent::Px(32),
                },
                (0, 32),
            ),
            (
                LayerSize {
                    width: Extent::Px(740),
                    height: Extent::Px(96),
                },
                (740, 96),
            ),
        ];
        for (size, wire) in sizes {
            assert_eq!(to_wire_size(size), wire, "{size:?}");
        }
        let zones = [
            (ExclusiveZone::Ignore, -1),
            (ExclusiveZone::Auto, 0),
            (ExclusiveZone::Reserve(32), 32),
            (ExclusiveZone::Reserve(u32::MAX), i32::MAX),
        ];
        for (zone, wire) in zones {
            assert_eq!(to_wire_exclusive(zone), wire, "{zone:?}");
        }
        let modes = [
            (KeyboardMode::None, KeyboardInteractivity::None),
            (KeyboardMode::Exclusive, KeyboardInteractivity::Exclusive),
            (KeyboardMode::OnDemand, KeyboardInteractivity::OnDemand),
        ];
        for (mode, wire) in modes {
            assert_eq!(to_wire_keyboard(mode), wire, "{mode:?}");
        }
    }
}
