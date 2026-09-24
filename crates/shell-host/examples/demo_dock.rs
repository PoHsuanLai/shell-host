//! The spike's dock on the real API: twelve 48 px tiles magnified on hover by a CSS transition
//! (`--mode=transform`, the default, or `--mode=width`), or by the component itself once per
//! compositor frame (`--mode=tick`: `use_frame_tick` with a `FrameDrive` held from the first
//! pointer motion until the tiles settle after the pointer leaves), anchored bottom, with its
//! input and blur regions following the pill (`RegionSpec::Element`). `--layer=bottom` puts it
//! under windows for the occlusion check. Driven by `dock hover X Y`, `dock leave`, `dock
//! stats`. Re-passes spike item (c): a hover sweep renders once per callback with p95 work
//! under the refresh period, and 5 s idle has zero frames, commits and main-thread wakeups.
//!
//! `demo_bar --with-dock` includes this file as a module to run the bar and the dock in one
//! process (item g), so everything but `main` is shared.

use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ElementId, ExclusiveZone, Extent, FrameDrive, FrameTick, Host, HostConfig,
    HostCtx, HostEvent, KeyboardMode, Layer, LayerConfig, LayerSize, Margin, OutputTarget,
    RegionSpec, Root, use_frame_tick, use_surface,
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
    /// Scales computed by the component on every frame tick (`use_frame_tick`), no CSS motion.
    Tick,
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
        Magnify::Tick => return rsx! { TickDock {} },
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

/// Tile scales in thousandths.
type Scales = [u32; 12];

const REST: Scales = [1000; 12];

/// The magnified scale at the tile centre under the pointer, and the reach of the effect.
const PEAK: f64 = 1500.0;
const REACH_PX: f64 = 112.0;

/// How fast a tile closes on its target: the gap shrinks by `dt / TIME_CONSTANT` per tick.
const TIME_CONSTANT: Duration = Duration::from_millis(60);

/// Close enough to the target to stop moving (thousandths).
const SETTLED: u32 = 4;

/// The centre of tile `i`, in surface-local logical px: the pill is centred in the dock with
/// 8 px padding, 48 px tiles and 8 px gaps.
fn tile_centre(i: usize) -> f64 {
    let pill = 12.0 * 48.0 + 11.0 * 8.0 + 16.0;
    let left = (f64::from(DOCK_WIDTH) - pill) / 2.0;
    left + 8.0 + i as f64 * 56.0 + 24.0
}

/// The scales the tiles move towards with the pointer at `x` (none: at rest).
fn targets(x: Option<f64>) -> Scales {
    let Some(x) = x else { return REST };
    std::array::from_fn(|i| {
        let d = ((x - tile_centre(i)) / REACH_PX).abs();
        let lift = (1.0 - d * d).max(0.0);
        (1000.0 + (PEAK - 1000.0) * lift).round() as u32
    })
}

/// One tick of motion: each tile closes `dt / TIME_CONSTANT` of its gap to `target`, and
/// snaps once within `SETTLED`.
fn step(now: Scales, target: Scales, dt: Duration) -> Scales {
    let k = (dt.as_secs_f64() / TIME_CONSTANT.as_secs_f64()).min(1.0);
    std::array::from_fn(|i| {
        let (s, t) = (f64::from(now[i]), f64::from(target[i]));
        let next = (s + (t - s) * k).round() as u32;
        if next.abs_diff(target[i]) <= SETTLED {
            target[i]
        } else {
            next
        }
    })
}

/// The dock with magnification run by the component at the compositor's frame rate: pointer
/// motion takes a `FrameDrive`, every `FrameTick` steps the scales, and the drive is dropped
/// once the tiles reach their targets (at rest after the pointer leaves, or still under a
/// resting pointer), so idle has zero frames.
#[component]
fn TickDock() -> Element {
    let surface = use_surface();
    let tick = use_frame_tick();
    let mut pointer = use_signal(|| None::<f64>);
    let mut scales = use_signal(|| REST);
    let mut drive = use_signal(|| None::<FrameDrive>);
    let leaving = surface.clone();
    use_effect(move || {
        let FrameTick { dt, .. } = tick();
        let target = targets(*pointer.peek());
        let next = step(*scales.peek(), target, dt);
        if next != *scales.peek() {
            scales.set(next);
        }
        if next == target && drive.peek().is_some() {
            drive.set(None);
        }
    });
    rsx! {
        style { "{RESET_CSS}{DOCK_CSS}.icon{{transform-origin:50% 100%}}" }
        div {
            class: "dock",
            onmousemove: move |event| {
                pointer.set(Some(event.client_coordinates().x));
                hold(drive, &surface);
            },
            onmouseleave: move |_| {
                pointer.set(None);
                hold(drive, &leaving);
            },
            div { id: "dock-pill", class: "pill",
                for (i, colour) in COLOURS.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: "icon",
                        style: "background:{colour};transform:scale({f64::from(scales()[i]) / 1000.0})",
                    }
                }
            }
        }
    }
}

/// Take a drive for `surface` unless one is held already.
fn hold(mut drive: Signal<Option<FrameDrive>>, surface: &shell_host::SurfaceHandle) {
    if drive.peek().is_none() {
        drive.set(Some(surface.drive_frames()));
    }
}

/// `--mode=transform|width|tick` and `--layer=top|overlay|bottom` from `args`.
pub fn dock_options(args: &[String]) -> Result<(Magnify, Layer), String> {
    let mode = match value(args, "--mode").as_deref() {
        None | Some("transform") => Magnify::Transform,
        Some("width") => Magnify::Width,
        Some("tick") => Magnify::Tick,
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
    if let Err(error) = shell_host::inject::stdin::install(ctx) {
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
