//! A plain text field for the IME acceptance (FINDINGS F71, `scripts/dev/accept-ime.sh`): one
//! 480 x 120 Overlay layer surface centred on the output with exclusive keyboard focus, as sill's
//! launcher panel has, holding a real `<input>` that focuses itself when it mounts (so the field
//! asks for the IME before the keyboard arrives, as the launcher's does).
//!
//! Every line goes to stderr as `ime: ...`: `value <text>` from the field's `oninput`, and
//! `key <key>` from its `onkeydown` (what reached the document as a raw key rather than through
//! the input method). stdin: `value` prints the field's current text as `ime: value <text>`,
//! `quit` exits.

use std::io::{BufRead, Write};
use std::sync::Mutex;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, Host, HostConfig, HostEvent, KeyboardMode, Layer,
    LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root,
};

const CSS: &str = "html,body{margin:0;padding:0;background:white}\
input{margin:24px;width:420px;height:48px;box-sizing:border-box;font:24px sans-serif;\
border:2px solid rgb(40,90,200)}";

/// The field's text as its `oninput` last reported it, for the `value` command.
static VALUE: Mutex<String> = Mutex::new(String::new());

/// Flushed at once: the acceptance reads these lines while the demo runs.
fn say(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "ime: {line}");
    let _ = err.flush();
}

fn app() -> Element {
    rsx! {
        style { "{CSS}" }
        input {
            id: "field",
            r#type: "text",
            onmounted: move |event: MountedEvent| async move {
                let _ = event.set_focus(true).await;
                say("focused");
            },
            oninput: move |event: FormEvent| {
                let value = event.value();
                say(&format!("value {value}"));
                *VALUE.lock().unwrap_or_else(|e| e.into_inner()) = value;
            },
            onkeydown: move |event: KeyboardEvent| say(&format!("key {}", event.key())),
        }
    }
}

fn config() -> LayerConfig {
    LayerConfig {
        namespace: "ime-demo".to_string(),
        layer: Layer::Overlay,
        anchor: Anchor::NONE,
        size: LayerSize {
            width: Extent::Px(480),
            height: Extent::Px(120),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::Exclusive,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug, Clone, Copy)]
enum Command {
    Value,
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    match line.trim() {
        "value" => Some(Command::Value),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_ime: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = Host::<Command>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match parse(&line) {
                Some(command) => {
                    let _ = handle.send(command);
                }
                None => eprintln!("demo_ime: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            if let Err(error) = ctx.spawn_layer(config(), Root::new(app)) {
                eprintln!("demo_ime: {error}");
                ctx.exit();
            }
        }
        HostEvent::App(Command::Value) => {
            let value = VALUE.lock().unwrap_or_else(|e| e.into_inner()).clone();
            say(&format!("value {value}"));
        }
        HostEvent::App(Command::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
