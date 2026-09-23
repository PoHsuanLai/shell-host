//! The spike's bar on the real API: a 32 px top bar with a translucent tint over a 2-logical-px
//! checkerboard, reserving its height, blurred behind when `DEMO_BLUR=1`. Driven by the
//! `debug-inject` commands on stdin (`bar hover X Y`, `bar stats`, `bar capture PATH`, `quit`).
//! Re-passes spike items (a) bar, (b) blur, (f) fractional scale, and with `--with-dock`, (g)
//! two surfaces on one device (`scripts/spike-accept.sh`, pointed at this example).
//!
//! H2 core fills the bodies.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, KeyboardMode, Layer, LayerConfig, LayerSize, Margin,
    OutputTarget, RegionSpec,
};

/// The bar's layer surface: top, full width, 32 px, reserving 32 px, never focused.
fn bar_config(blur: BlurSpec) -> LayerConfig {
    LayerConfig {
        namespace: "bar".to_owned(),
        layer: Layer::Top,
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        size: LayerSize {
            width: Extent::Fill,
            height: Extent::Px(32),
        },
        exclusive: ExclusiveZone::Reserve(32),
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur,
        input: RegionSpec::Whole,
    }
}

/// The bar document: a tinted strip with a label and the checkerboard probe.
fn bar() -> Element {
    todo!()
}

/// `Host::connect`, `debug-inject` on stdin, spawn the bar (and the dock with `--with-dock`) on
/// `HostEvent::Started`, run until `quit`.
fn main() {
    todo!()
}
