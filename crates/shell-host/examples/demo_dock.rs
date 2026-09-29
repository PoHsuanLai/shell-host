//! A dock: twelve 48 px tiles magnified on hover by a CSS transition
//! (`--mode=transform`, the default, or `--mode=width`), or by the component itself once per
//! compositor frame (`--mode=tick`: `use_frame_tick` with a `FrameDrive` held from the first
//! pointer motion until the tiles settle after the pointer leaves), anchored bottom, with its
//! input and blur regions following the pill (`RegionSpec::Element`). `--cards=two` adds a
//! second, independently region-tracked element above the pill (`RegionSpec::Elements`),
//! for verifying on a headless KWin (FINDINGS F52/F55) that fake input reaches each element's
//! own strips and nothing in the gap between them. `--layer=bottom` puts it under windows for
//! the occlusion check. Driven by `dock hover X Y`, `dock leave`, `dock stats`. Render acceptance
//! item (c): a hover sweep renders once per callback with p95 work under the refresh period,
//! and 5 s idle has zero frames, commits and main-thread wakeups.
//!
//! `demo_bar --with-dock` runs the same dock (`shared/dock.rs`) next to the bar in one process
//! (item g).

#[path = "shared/dock.rs"]
mod dock;

use dock::{dock_options, install_commands, spawn_dock};
use shell_host::{Host, HostConfig, HostEvent};

/// Parse `--mode`, `--cards` and `--layer`, `Host::connect`, `debug-inject`, spawn the dock on
/// `HostEvent::Started`, run until `quit`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("demo_dock: {error}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (props, layer) = dock_options(args)?;
    let host = Host::<()>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            install_commands(ctx);
            spawn_dock(ctx, props, layer);
        }
        // The compositor closed the dock (its output went away): nothing is left to drive.
        HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
