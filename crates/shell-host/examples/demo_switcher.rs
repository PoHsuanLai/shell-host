//! An Exclusive-keyboard layer surface for the modifiers acceptance (FINDINGS F66): a small
//! centred panel, spawned only on the `map` stdin command (so a script can hold a modifier or
//! toggle a lock key before the surface, and its `wl_keyboard.enter`, ever exists) and closed on
//! `unmap`. Its document reads `use_modifiers()` and prints
//! `{"event":"modifiers",..}` on its first render and every change: the seat's already-current
//! state right after `wl_keyboard.enter`, then whatever `wl_keyboard.modifiers` reports next
//! (a held key released, a lock key toggled), and nothing at all if none of that changes.
//!
//! stdin, one command per line: `map`, `unmap`, `quit`.

use std::io::{BufRead, Write};

use dioxus::prelude::*;
use serde_json::json;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, Host, HostConfig, HostEvent, KeyboardMode, Layer,
    LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root, SurfaceId, use_modifiers,
};

const CSS: &str =
    "html,body{margin:0;padding:0}.panel{width:160px;height:100px;background:rgb(90,60,200)}";

fn print(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// `{:?}` of every field: `Modifier::{Held,Released}` and `LockKey::{Locked,Unlocked}` already
/// read as values (CONVENTIONS §7), so this is JSON-shaped without a manual mapping.
fn panel() -> Element {
    let modifiers = use_modifiers();
    use_effect(move || {
        let m = modifiers();
        print(json!({
            "event": "modifiers",
            "shift": format!("{:?}", m.shift),
            "ctrl": format!("{:?}", m.ctrl),
            "alt": format!("{:?}", m.alt),
            "logo": format!("{:?}", m.logo),
            "caps_lock": format!("{:?}", m.caps_lock),
            "num_lock": format!("{:?}", m.num_lock),
        }));
    });
    rsx! {
        style { "{CSS}" }
        div { class: "panel" }
    }
}

fn config() -> LayerConfig {
    LayerConfig {
        namespace: "switcher".to_string(),
        layer: Layer::Overlay,
        anchor: Anchor::NONE,
        size: LayerSize {
            width: Extent::Px(160),
            height: Extent::Px(100),
        },
        exclusive: ExclusiveZone::Ignore,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::Exclusive,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

#[derive(Debug, Clone, Copy)]
enum Command {
    Map,
    Unmap,
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    match line.trim() {
        "map" => Some(Command::Map),
        "unmap" => Some(Command::Unmap),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_switcher: {error}");
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
                None => eprintln!("demo_switcher: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    let mut mapped: Option<SurfaceId> = None;
    host.run(move |ctx, event| match event {
        HostEvent::App(Command::Map) => {
            if mapped.is_some() {
                return;
            }
            match ctx.spawn_layer(config(), Root::new(panel)) {
                Ok(id) => mapped = Some(id),
                Err(error) => eprintln!("demo_switcher: {error}"),
            }
        }
        HostEvent::App(Command::Unmap) => {
            if let Some(id) = mapped.take() {
                ctx.close(id);
            }
        }
        HostEvent::SurfaceClosed { id, .. } if Some(id) == mapped => {
            mapped = None;
        }
        HostEvent::App(Command::Quit) => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
