//! A scrolling surface for real-input acceptance of the host's scroll engine (FINDINGS F63):
//! one 400 x 600 layer surface centred on the output, holding
//!
//! - `#list`, a 300 x 600 scroll container (`data-overscroll="band"`) of 60 rows of 50 px,
//!   watched with `use_scroll`, and
//! - `#capture`, a 100 x 100 `data-wheel="capture"` element whose `onwheel` handler
//!   `prevent_default`s and counts the raw wheel events it receives.
//!
//! Every line goes to stderr as `scroll: ...`. stdin: `stats` prints
//! `scroll: stats {"frames":N,"commits":N,"offset":Y,"captured":K}` (the surface's cumulative
//! frame counters, the list's offset as `use_scroll` last reported it, and the capture count),
//! `quit` exits.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ElementId, ExclusiveZone, Extent, Host, HostConfig, HostEvent, KeyboardMode,
    Layer, LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root, SurfaceId, use_scroll,
};

const CSS: &str = "html,body{margin:0;padding:0;background:rgb(240,240,240)}\
#list{position:absolute;left:0;top:0;width:300px;height:600px;overflow:auto;\
scrollbar-width:none;background:white}\
.row{height:50px;box-sizing:border-box;border-bottom:1px solid #ddd;padding:12px;\
font:16px sans-serif}\
#capture{position:absolute;left:300px;top:0;width:100px;height:100px;background:rgb(40,90,200)}";

/// The list's offset as `use_scroll` last reported it, as f64 bits.
static OFFSET: AtomicU64 = AtomicU64::new(0);
/// Raw wheel events `#capture` received.
static CAPTURED: AtomicU32 = AtomicU32::new(0);

/// Flushed at once: the acceptance reads these lines while the demo runs.
fn say(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "scroll: {line}");
    let _ = err.flush();
}

fn app() -> Element {
    let state = use_scroll(ElementId::new("list"));
    let offset = state.read().offset.y;
    OFFSET.store(offset.to_bits(), Ordering::SeqCst);
    rsx! {
        style { "{CSS}" }
        div { id: "list", "data-overscroll": "band",
            for i in 0..60 {
                div { class: "row", "row {i}" }
            }
        }
        div {
            id: "capture",
            "data-wheel": "capture",
            onwheel: move |event: WheelEvent| {
                event.prevent_default();
                CAPTURED.fetch_add(1, Ordering::SeqCst);
                say(&format!("capture wheel {}", event.delta().strip_units().y));
            },
        }
    }
}

fn config() -> LayerConfig {
    LayerConfig {
        namespace: "scroll-demo".to_string(),
        layer: Layer::Overlay,
        anchor: Anchor::NONE,
        size: LayerSize {
            width: Extent::Px(400),
            height: Extent::Px(600),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

/// A stdin line.
#[derive(Debug, Clone, Copy)]
enum Command {
    Stats,
    Quit,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_scroll: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = Host::<Command>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let command = match line.trim() {
                "stats" => Command::Stats,
                "quit" => Command::Quit,
                other => {
                    eprintln!("demo_scroll: unknown command {other:?}");
                    continue;
                }
            };
            let _ = handle.send(command);
        }
        let _ = handle.send(Command::Quit);
    });
    let mut surface: Option<SurfaceId> = None;
    host.run(move |ctx, event| match event {
        HostEvent::Started => match ctx.spawn_layer(config(), Root::new(app)) {
            Ok(id) => surface = Some(id),
            Err(error) => {
                eprintln!("demo_scroll: {error}");
                ctx.exit();
            }
        },
        HostEvent::App(Command::Stats) => {
            let counters = surface
                .and_then(|id| ctx.stats(id))
                .map(|stats| stats.counters);
            let (frames, commits) = counters.map_or((0, 0), |c| (c.frames, c.commits));
            say(&format!(
                "stats {{\"frames\":{frames},\"commits\":{commits},\"offset\":{},\"captured\":{}}}",
                f64::from_bits(OFFSET.load(Ordering::SeqCst)),
                CAPTURED.load(Ordering::SeqCst)
            ));
        }
        HostEvent::App(Command::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
