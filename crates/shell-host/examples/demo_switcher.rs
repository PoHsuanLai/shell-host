//! An Exclusive-keyboard layer surface for the modifiers acceptance: a small centred panel, spawned
//! only on the first `map` stdin command (so a script can hold a modifier or toggle a lock key
//! before the surface, and its `wl_keyboard.enter`, ever exists). A later `unmap`/`map` pair hides
//! and re-shows the *same* surface (`Visibility::Hidden` / `Shown`, not a close and respawn):
//! `unmap` rebuilds its `wl_surface` (the unmap that must withdraw the keyboard) while keeping its
//! `SurfaceId` and document, exactly the shape a real keyboard-exclusive surface (an app switcher,
//! a lock screen, a power menu) maps and unmaps in. Its document reads `use_modifiers()` and prints
//! `{"event":"modifiers",..}` on its first render and every change: the seat's already-current
//! state right after `wl_keyboard.enter`, then whatever `wl_keyboard.modifiers` reports next (a
//! held key released, a lock key toggled), and nothing at all if none of that changes. The host
//! loop also prints `{"event":"focus","state":"Gained"|"Lost"}` for the surface's own
//! `HostEvent::KeyboardFocus`: a host that withdraws the keyboard at unmap prints `Lost` there,
//! before the next `map`'s `Gained`; one that does not goes straight from `Gained` to `Gained`
//! again, with no `Lost` between them.
//!
//! stdin, one command per line: `map`, `unmap`, `quit`.

#[path = "shared/harness.rs"]
mod harness;

use std::io::{BufRead, Write};

use dioxus::prelude::*;
use serde_json::json;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Host, HostEvent, HostRoot, KeyboardMode, LayerConfig,
    LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget, RegionSpec, SurfaceId, Visibility,
    use_modifiers,
};

const CSS: &str =
    "html,body{margin:0;padding:0}.panel{width:160px;height:100px;background:rgb(90,60,200)}";

fn print(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// `{:?}` of every field: `Modifier::{Held,Released}` and `LockKey::{Locked,Unlocked}` already
/// read as values (CONVENTIONS §13), so this is JSON-shaped without a manual mapping.
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
        layer: LayerLevel::Overlay,
        anchor: Anchor::NONE,
        size: LayerSize {
            width: LayerExtent::Px(160),
            height: LayerExtent::Px(100),
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
    let host = Host::<Command>::connect(harness::host_config()).map_err(|e| e.to_string())?;
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
    // `None` until the first `map`; after that the surface stays alive (hidden or shown) for the
    // rest of the run, so `unmap`/`map` exercise the true unmap-and-rebuild path rather than a
    // close and a fresh spawn.
    let mut surface: Option<SurfaceId> = None;
    host.run(move |ctx, event| match event {
        HostEvent::App(Command::Map) => match surface {
            Some(id) => ctx.set_visibility(id, Visibility::Shown),
            None => match ctx.spawn_layer(config(), HostRoot::new(panel)) {
                Ok(id) => surface = Some(id),
                Err(error) => eprintln!("demo_switcher: {error}"),
            },
        },
        HostEvent::App(Command::Unmap) => {
            if let Some(id) = surface {
                ctx.set_visibility(id, Visibility::Hidden);
            }
        }
        HostEvent::SurfaceClosed { id, .. } if Some(id) == surface => {
            surface = None;
        }
        HostEvent::KeyboardFocus { id, focus } if Some(id) == surface => {
            print(json!({ "event": "focus", "state": format!("{focus:?}") }));
        }
        HostEvent::App(Command::Quit) | HostEvent::QuitAsked => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
