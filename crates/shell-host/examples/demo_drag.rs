//! A draggable thumbnail for real-input drag acceptance and the modifier probe
//! (`scripts/dev/probe-modifiers.sh`): a 64 x 64 layer surface at the output's bottom-right corner
//! (margin 40, so on a 1920 x 1080 output it covers 1816..1880 x 976..1040, clear of where a
//! compositor places a first window). A press on it followed by a move of more than 8 px starts a
//! `DragPayload::Uris` drag of a temporary file (`SurfaceHandle::start_drag`); the outcome is
//! printed as `drag: outcome {json}` when the drag ends, and `stats` on stdin prints `drag: stats
//! {"started":N,"outcome":{json}|null}`.
//!
//! Every mouse enter prints `drag: mods enter [..]` and every change of the modifiers a move
//! carries prints `drag: mods move [..]`: the modifier state the host attaches to pointer
//! events on a surface that may never have keyboard focus.
//!
//! Flags: `--icon=none|root` (default `none`: no icon; `root`: a 48 x 48 document follows the
//! pointer), `--keyboard=none|on-demand` (the layer's `KeyboardMode`, default `none`).
//! stdin, one command per line: `stats`, `quit`.

#[path = "shared/harness.rs"]
mod harness;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, DragActions, DragIcon, DragOutcome, DragPayload, DragSpec, ExclusiveZone,
    Host, HostEvent, HostRoot, KeyboardMode, LayerConfig, LayerExtent, LayerLevel, LayerSize,
    Margin, OutputTarget, RegionSpec, Size, use_surface,
};

const CSS: &str = "html,body{margin:0;padding:0}\
.thumb{width:64px;height:64px;background:rgb(60,120,200)}\
.icon{width:48px;height:48px;background:rgb(230,160,40);border-radius:8px}";

/// How far (logical px) a held press must move before it becomes a drag.
const THRESHOLD: f64 = 8.0;

/// Whether the drag carries an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconChoice {
    None,
    Root,
}

/// Where the gesture is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Gesture {
    Idle,
    Pressed { x: f64, y: f64 },
    Dragging,
}

/// The file being dragged, and the command-line choices, fixed at startup.
#[derive(Debug, Clone, PartialEq)]
struct Props {
    file: PathBuf,
    icon: IconChoice,
}

static STARTED: AtomicU32 = AtomicU32::new(0);
static OUTCOME: Mutex<Option<String>> = Mutex::new(None);

/// Flushed at once: the acceptance reads these lines while the demo runs.
fn say(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "drag: {line}");
    let _ = err.flush();
}

fn icon() -> Element {
    rsx! {
        style { "{CSS}" }
        div { class: "icon" }
    }
}

fn spec(props: &Props) -> DragSpec {
    let icon = match props.icon {
        IconChoice::None => DragIcon::None,
        IconChoice::Root => DragIcon::Root {
            root: HostRoot::new(icon),
            size: Size {
                width: 48,
                height: 48,
            },
        },
    };
    DragSpec {
        payload: DragPayload::Uris(vec![format!("file://{}", props.file.display())]),
        actions: DragActions::Copy,
        icon,
    }
}

fn thumb(props: Props) -> Element {
    let surface = use_surface();
    let mut gesture = use_signal(|| Gesture::Idle);
    let mut mods_seen = use_signal(String::new);
    let props_for_drag = props.clone();
    let onmove = move |event: MouseEvent| {
        let mods = format!("{:?}", event.modifiers());
        if *mods_seen.peek() != mods {
            say(&format!("mods move {mods}"));
            mods_seen.set(mods);
        }
        let Gesture::Pressed { x, y } = *gesture.peek() else {
            return;
        };
        let at = event.client_coordinates();
        if (at.x - x).hypot(at.y - y) <= THRESHOLD {
            return;
        }
        gesture.set(Gesture::Dragging);
        STARTED.fetch_add(1, Ordering::SeqCst);
        say("started");
        let surface = surface.clone();
        let spec = spec(&props_for_drag);
        spawn(async move {
            let outcome = surface.start_drag(spec).await;
            let json = json(&outcome);
            say(&format!("outcome {json}"));
            if let Ok(mut slot) = OUTCOME.lock() {
                *slot = Some(json);
            }
            gesture.set(Gesture::Idle);
        });
    };
    rsx! {
        style { "{CSS}" }
        div {
            class: "thumb",
            onmouseenter: move |event: MouseEvent| {
                say(&format!("mods enter {:?}", event.modifiers()));
            },
            onmousedown: move |event: MouseEvent| {
                let at = event.client_coordinates();
                gesture.set(Gesture::Pressed { x: at.x, y: at.y });
            },
            onmousemove: onmove,
            onmouseup: move |_| {
                if *gesture.peek() != Gesture::Dragging {
                    gesture.set(Gesture::Idle);
                }
            },
        }
    }
}

fn json(outcome: &DragOutcome) -> String {
    serde_json::to_string(outcome).unwrap_or_else(|error| format!("\"{error}\""))
}

fn config(keyboard: KeyboardMode) -> LayerConfig {
    LayerConfig {
        namespace: "drag-thumb".to_string(),
        layer: LayerLevel::Overlay,
        anchor: Anchor::BOTTOM.union(Anchor::RIGHT),
        size: LayerSize {
            width: LayerExtent::Px(64),
            height: LayerExtent::Px(64),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin {
            top: 0,
            right: 40,
            bottom: 40,
            left: 0,
        },
        keyboard,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug, Clone, Copy)]
enum Command {
    Stats,
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    match line.trim() {
        "stats" => Some(Command::Stats),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

/// `--icon=` and `--keyboard=`.
fn options() -> Result<(IconChoice, KeyboardMode), String> {
    let mut icon = IconChoice::None;
    let mut keyboard = KeyboardMode::None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--icon=none" => icon = IconChoice::None,
            "--icon=root" => icon = IconChoice::Root,
            "--keyboard=none" => keyboard = KeyboardMode::None,
            "--keyboard=on-demand" => keyboard = KeyboardMode::OnDemand,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok((icon, keyboard))
}

/// A small file to drag, in the temporary directory.
fn temp_file() -> Result<PathBuf, String> {
    let path =
        std::env::temp_dir().join(format!("shell-host-demo-drag-{}.txt", std::process::id()));
    std::fs::write(&path, b"dragged out of shell-host\n").map_err(|e| e.to_string())?;
    Ok(path)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_drag: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (icon, keyboard) = options()?;
    let file = temp_file()?;
    say(&format!("file {}", file.display()));
    let host = Host::<Command>::connect(harness::host_config()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match parse(&line) {
                Some(command) => {
                    let _ = handle.send(command);
                }
                None => eprintln!("demo_drag: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    let props = Props {
        file: file.clone(),
        icon,
    };
    let result = host.run(move |ctx, event| match event {
        HostEvent::Started => {
            if let Err(error) =
                ctx.spawn_layer(config(keyboard), HostRoot::with_props(thumb, props.clone()))
            {
                eprintln!("demo_drag: {error}");
                ctx.exit();
            }
        }
        HostEvent::App(Command::Stats) => {
            let outcome = OUTCOME
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .unwrap_or_else(|| "null".to_owned());
            say(&format!(
                "stats {{\"started\":{},\"outcome\":{outcome}}}",
                STARTED.load(Ordering::SeqCst)
            ));
        }
        HostEvent::App(Command::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    });
    let _ = std::fs::remove_file(&file);
    result.map_err(|e| e.to_string())
}
