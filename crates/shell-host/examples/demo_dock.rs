//! The spike's dock on the real API: twelve 48 px tiles magnified on hover by a CSS transition
//! (`--mode=transform`, the default, or `--mode=width`), anchored bottom, with its input and
//! blur regions following the pill (`RegionSpec::Element`). `--layer=bottom` puts it under
//! windows for the occlusion check. Driven by `dock hover X Y`, `dock leave`, `dock stats`.
//! Re-passes spike item (c): a hover sweep renders once per callback with p95 work under the
//! refresh period, and 5 s idle has zero frames, commits and main-thread wakeups.
//!
//! H2 core fills the bodies.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, KeyboardMode, Layer, LayerConfig, LayerSize, Margin,
    OutputTarget, RegionSpec, element_id,
};

/// Tiles, gaps and the room the magnified end tiles need.
const DOCK_WIDTH: u32 = 12 * 48 + 11 * 8 + 96;
const DOCK_HEIGHT: u32 = 96;

/// How the tiles magnify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Magnify {
    /// `transform: scale()` transitions (no relayout).
    Transform,
    /// `width`/`height` transitions (relayout every frame).
    Width,
}

/// The dock's layer surface: bottom-centred, never focused, input and blur on the pill.
fn dock_config(layer: Layer) -> LayerConfig {
    let pill = RegionSpec::Element {
        id: element_id("dock-pill"),
        radius: 12,
        inflate: 0,
    };
    LayerConfig {
        namespace: "dock".to_owned(),
        layer,
        anchor: Anchor::BOTTOM,
        size: LayerSize {
            width: Extent::Px(DOCK_WIDTH),
            height: Extent::Px(DOCK_HEIGHT),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::Region(pill.clone()),
        input: pill,
    }
}

/// The dock document for `mode`.
fn dock(mode: Magnify) -> Element {
    todo!()
}

/// Parse `--mode` and `--layer`, `Host::connect`, `debug-inject`, spawn the dock on
/// `HostEvent::Started`, run until `quit`.
fn main() {
    todo!()
}
