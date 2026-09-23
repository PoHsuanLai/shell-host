//! The spike's dock on the real API: twelve 48 px tiles magnified on hover by a CSS transition
//! (`--mode=transform`, the default, or `--mode=width`), anchored bottom, with its input and
//! blur regions following the pill (`RegionSpec::Element`). `--layer=bottom` puts it under
//! windows for the occlusion check. Driven by `dock hover X Y`, `dock leave`, `dock stats`.
//! Re-passes spike item (c): a hover sweep renders once per callback with p95 work under the
//! refresh period, and 5 s idle has zero frames, commits and main-thread wakeups.
//!
//! `demo_bar --with-dock` includes this file as a module to run the bar and the dock in one
//! process (item g), so everything but `main` is shared.

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ElementId, ExclusiveZone, Extent, Host, HostConfig, HostCtx, HostEvent, KeyboardMode,
    Layer, LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root,
};

/// Tiles, gaps and the room the magnified end tiles need.
pub const DOCK_WIDTH: u32 = 12 * 48 + 11 * 8 + 96;
pub const DOCK_HEIGHT: u32 = 96;

/// How the tiles magnify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Magnify {
    /// `transform: scale()` transitions (no relayout).
    Transform,
    /// `width`/`height` transitions (relayout every frame).
    Width,
}

/// The margin and padding reset quire's `css/reset.css` would bring; the host's UA sheet sizes
/// the root but leaves the reset to the design system (FINDINGS F12).
const RESET_CSS: &str = "html,body{margin:0;padding:0}";

// In-flow layout on purpose: with the row `position:absolute` Blitz hit-testing never reached
// the icons (SPIKE.md S1 finding 2).
const DOCK_CSS: &str = ".dock{height:96px;display:flex;align-items:flex-end;\
justify-content:center;padding-bottom:8px;box-sizing:border-box}\
.pill{display:flex;align-items:flex-end;gap:8px;padding:8px;border-radius:12px;\
background:rgba(30,30,40,0.35)}\
.icon{width:48px;height:48px;border-radius:12px;flex:none}";

const TRANSFORM_CSS: &str = ".icon{transition:transform 170ms;transform-origin:50% 100%}\
.icon:hover{transform:scale(1.5)}";

const WIDTH_CSS: &str =
    ".icon{transition:width 170ms,height 170ms}.icon:hover{width:72px;height:72px}";

const COLOURS: [&str; 12] = [
    "#e6194b", "#3cb44b", "#ffe119", "#4363d8", "#f58231", "#911eb4", "#46f0f0", "#f032e6",
    "#bcf60c", "#fabebe", "#008080", "#e6beff",
];

/// The dock's layer surface: bottom-centred, never focused, input and blur on the pill.
pub fn dock_config(layer: Layer) -> LayerConfig {
    let pill = RegionSpec::Element {
        id: ElementId::new("dock-pill"),
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
pub fn dock(mode: Magnify) -> Element {
    let magnify = match mode {
        Magnify::Transform => TRANSFORM_CSS,
        Magnify::Width => WIDTH_CSS,
    };
    rsx! {
        style { "{RESET_CSS}{DOCK_CSS}{magnify}" }
        div { class: "dock",
            div { id: "dock-pill", class: "pill",
                for (i, colour) in COLOURS.iter().enumerate() {
                    div { key: "{i}", class: "icon", style: "background:{colour}" }
                }
            }
        }
    }
}

/// `--mode=transform|width` and `--layer=top|overlay|bottom` from `args`.
pub fn dock_options(args: &[String]) -> Result<(Magnify, Layer), String> {
    let mode = match value(args, "--mode").as_deref() {
        None | Some("transform") => Magnify::Transform,
        Some("width") => Magnify::Width,
        Some(other) => return Err(format!("unknown --mode={other}")),
    };
    Ok((mode, layer_option(args)?))
}

/// `--layer=top|overlay|bottom`, top by default.
pub fn layer_option(args: &[String]) -> Result<Layer, String> {
    match value(args, "--layer").as_deref() {
        None | Some("top") => Ok(Layer::Top),
        Some("overlay") => Ok(Layer::Overlay),
        Some("bottom") => Ok(Layer::Bottom),
        Some(other) => Err(format!("unknown --layer={other}")),
    }
}

/// The value of `--name=value` in `args`.
pub fn value(args: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    args.iter()
        .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_owned))
}

/// Spawn the dock; a failure ends the demo.
pub fn spawn_dock<M: 'static>(ctx: &mut HostCtx<M>, mode: Magnify, layer: Layer) {
    if let Err(error) = ctx.spawn_layer(dock_config(layer), Root::with_props(dock, mode)) {
        eprintln!("dock: {error}");
        ctx.exit();
    }
}

/// `debug-inject` on stdin; a failure ends the demo, since nothing could drive it.
pub fn install_commands<M: 'static>(ctx: &mut HostCtx<M>) {
    if let Err(error) = shell_host::inject::install(ctx) {
        eprintln!("debug-inject: {error}");
        ctx.exit();
    }
}

/// Parse `--mode` and `--layer`, `Host::connect`, `debug-inject`, spawn the dock on
/// `HostEvent::Started`, run until `quit`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("demo_dock: {error}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (mode, layer) = dock_options(args)?;
    let host = Host::<()>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            install_commands(ctx);
            spawn_dock(ctx, mode, layer);
        }
        // The compositor closed the dock (its output went away): nothing is left to drive.
        HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
