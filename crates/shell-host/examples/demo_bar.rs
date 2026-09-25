//! The spike's bar on the real API: a 32 px top bar with a translucent tint over a 2-logical-px
//! checkerboard, reserving its height, blurred behind when `DEMO_BLUR=1`. Driven by the
//! `debug-inject` commands on stdin (`bar hover X Y`, `bar stats`, `bar capture PATH`, `quit`).
//! Re-passes spike items (a) bar, (b) blur, (f) fractional scale, and with `--with-dock`, (g)
//! two surfaces on one device (`scripts/spike-accept.sh demo <item>`).
//!
//! Flags: `--with-dock` (bar and dock in one process; the dock takes `--mode` and `--layer`
//! as in `demo_dock`), `--exclusive=N` (reserve N px instead of 32; item b uses 0 so a window
//! can sit behind the bar).

// Only the dock's document and config are used here; its `main` is demo_dock's own.
#[allow(dead_code)]
#[path = "demo_dock.rs"]
mod dock;

use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, Host, HostConfig, HostEvent, KeyboardMode, Layer,
    LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root,
};

/// The margin and padding reset quire's `css/reset.css` would bring (FINDINGS F12).
const RESET_CSS: &str = "html,body{margin:0;padding:0}";

/// A magenta tint (item a probes it at (10, 10)) and a 2-logical-px checkerboard at x 200..280,
/// y 12..20 (item f counts its physical runs).
const BAR_CSS: &str = ".bar{height:32px;box-sizing:border-box;padding:0 12px;display:flex;\
align-items:center;justify-content:space-between;background:rgba(255,0,128,0.55);\
color:white;font-size:14px;font-family:sans-serif}\
.checker{position:absolute;left:200px;top:12px;display:flex;height:8px}\
.checker div{width:2px;height:8px}\
.checker div:nth-child(odd){background:black}.checker div:nth-child(even){background:white}";

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

/// Presses the bar's document has received, printed as `bar: press N` on each one so a real-input
/// run can count them against the presses it sent (FINDINGS F53: one press must arrive once).
static PRESSES: AtomicU32 = AtomicU32::new(0);

/// The bar document: a tinted strip with a label and the checkerboard probe.
fn bar() -> Element {
    rsx! {
        style { "{RESET_CSS}{BAR_CSS}" }
        div {
            class: "bar",
            onmousedown: move |_| {
                let count = PRESSES.fetch_add(1, Ordering::Relaxed) + 1;
                eprintln!("bar: press {count}");
            },
            span { "shell-host demo bar" }
            div { class: "checker",
                for i in 0..40 {
                    div { key: "{i}" }
                }
            }
        }
    }
}

/// What the command line and the environment ask for.
#[derive(Debug, Clone)]
struct Options {
    blur: BlurSpec,
    exclusive: ExclusiveZone,
    dock: Option<(dock::Magnify, Layer)>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let blur = match std::env::var("DEMO_BLUR").as_deref() {
        Ok("1") => BlurSpec::Region(RegionSpec::Whole),
        _ => BlurSpec::None,
    };
    let exclusive = match dock::value(args, "--exclusive") {
        None => ExclusiveZone::Reserve(32),
        Some(px) => ExclusiveZone::Reserve(px.parse().map_err(|e| format!("--exclusive: {e}"))?),
    };
    let dock = args
        .iter()
        .any(|arg| arg == "--with-dock")
        .then(|| dock::dock_options(args))
        .transpose()?;
    Ok(Options {
        blur,
        exclusive,
        dock,
    })
}

/// `Host::connect`, `debug-inject` on stdin, spawn the bar (and the dock with `--with-dock`) on
/// `HostEvent::Started`, run until `quit`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("demo_bar: {error}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let options = options(args)?;
    let host = Host::<()>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            dock::install_commands(ctx);
            let config = LayerConfig {
                exclusive: options.exclusive,
                ..bar_config(options.blur.clone())
            };
            if let Err(error) = ctx.spawn_layer(config, Root::new(bar)) {
                eprintln!("bar: {error}");
                ctx.exit();
            }
            if let Some((mode, layer)) = options.dock {
                dock::spawn_dock(ctx, mode, layer);
            }
        }
        // The compositor closed a surface (its output went away): nothing is left to drive.
        HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
