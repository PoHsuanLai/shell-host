//! What a design system's pixel snapping gets from the host: the surface's scale as live state
//! (`use_surface_scale`) and a pre-paint hook (`HostConfig::prepaint`), here a counter standing
//! in for quire's `snap_to_device` (FINDINGS F43).
//!
//! A 28 px top layer with 1 px rules. Every render of its root prints the scale it read; the
//! hook counts its calls and records the viewport scale it saw. After 2 s the demo prints a
//! report (the stats counters and the hook's), idles 5 s with no input, prints another, and
//! exits. The hook count must equal the frame count in both, and not move across the idle.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, Host, HostConfig, HostCtx, HostEvent, KeyboardMode,
    Layer, LayerConfig, LayerSize, Margin, OutputTarget, Prepaint, RegionSpec, Root, SurfaceId,
    use_surface_scale,
};

/// How long the surface gets to settle before the first report, and how long it then idles.
const SETTLE: Duration = Duration::from_secs(2);
const IDLE: Duration = Duration::from_secs(5);

/// Calls of the pre-paint hook, and the viewport scale (in 120ths) the last one saw.
static HOOK_CALLS: AtomicU64 = AtomicU64::new(0);
static HOOK_SCALE120: AtomicU32 = AtomicU32::new(0);

const CSS: &str = "html,body{margin:0;padding:0}\
.strip{height:28px;box-sizing:border-box;border-bottom:1px solid black;display:flex;\
align-items:center;gap:8px;padding:0 8px;background:rgba(240,240,240,0.9);font:13px sans-serif}\
.rule{width:1px;height:14px;background:black}";

/// Messages from the timer thread.
#[derive(Debug, Clone, Copy)]
enum Step {
    Report(Moment),
    Quit,
}

/// Which report this is.
#[derive(Debug, Clone, Copy)]
enum Moment {
    Settled,
    AfterIdle,
}

fn strip() -> Element {
    let scale = use_surface_scale();
    let wire = scale().0;
    println!("demo_scale: render reads scale120={wire}");
    rsx! {
        style { "{CSS}" }
        div { class: "strip",
            span { "scale {wire}/120" }
            div { class: "rule" }
            span { "1 px rules" }
            div { class: "rule" }
        }
    }
}

fn strip_config() -> LayerConfig {
    LayerConfig {
        namespace: "scale-demo".to_owned(),
        layer: Layer::Top,
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        size: LayerSize {
            width: Extent::Fill,
            height: Extent::Px(28),
        },
        exclusive: ExclusiveZone::Ignore,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Empty,
    }
}

/// The counting hook: cheap, and it never touches the document's dirty state.
fn counting_hook() -> Prepaint {
    Prepaint::hook(|doc| {
        HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
        let scale120 = (doc.viewport().scale_f64() * 120.0).round() as u32;
        HOOK_SCALE120.store(scale120, Ordering::Relaxed);
    })
}

fn report(ctx: &HostCtx<Step>, id: SurfaceId, moment: Moment) {
    let Some(stats) = ctx.stats(id) else {
        println!("demo_scale: {moment:?}: the surface is gone");
        return;
    };
    let c = stats.counters;
    println!(
        "demo_scale: {moment:?}: frames={} commits={} callbacks={} watchdog_fired={} \
         prepaints={} hook_calls={} hook_scale120={} surface_scale120={} logical={}x{} \
         buffer={}x{}",
        c.frames,
        c.commits,
        c.callbacks,
        c.watchdog_fired,
        c.prepaints,
        HOOK_CALLS.load(Ordering::Relaxed),
        HOOK_SCALE120.load(Ordering::Relaxed),
        stats.scale120,
        stats.logical.width,
        stats.logical.height,
        stats.buffer.width,
        stats.buffer.height,
    );
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_scale: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = HostConfig {
        prepaint: counting_hook(),
        ..HostConfig::default()
    };
    let host = Host::<Step>::connect(config).map_err(|e| e.to_string())?;
    let handle = host.handle();
    let mut strip_id = None;
    host.run(move |ctx, event| match event {
        HostEvent::Started => match ctx.spawn_layer(strip_config(), Root::new(strip)) {
            Ok(id) => {
                strip_id = Some(id);
                let handle = handle.clone();
                thread::spawn(move || {
                    thread::sleep(SETTLE);
                    let _ = handle.send(Step::Report(Moment::Settled));
                    thread::sleep(IDLE);
                    let _ = handle.send(Step::Report(Moment::AfterIdle));
                    let _ = handle.send(Step::Quit);
                });
            }
            Err(error) => {
                eprintln!("demo_scale: {error}");
                ctx.exit();
            }
        },
        HostEvent::App(Step::Report(moment)) => {
            if let Some(id) = strip_id {
                report(ctx, id, moment);
            }
        }
        HostEvent::App(Step::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
