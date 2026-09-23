//! The `debug-inject` feature: debug commands on stdin (`debug::parse_command`), one JSON line
//! of `FrameStats` per surface for `stats`, and `capture` through the presenter's offscreen
//! render. The real-API successor of the spike's command source; examples/demo_*.rs use it.
//!
//! Output is one JSON object per line on stdout: a `FrameStats` per surface for `stats` (the
//! global `stats` first prints one `{"host": ..}` line with the compositor facts), and
//! `{"capture": path, "ok": .., "err": ..}` for `capture`.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use blitz_traits::events::{MouseEventButton, MouseEventButtons};
use calloop::generic::Generic;
use calloop::{Interest, Mode, PostAction};
use keyboard_types::Modifiers;
use serde_json::json;

use crate::ctx::HostCtx;
use crate::debug::{DebugCommand, Inject, SurfaceName, parse_command};
use crate::dom::document::Polled;
use crate::error::{HostError, Result};
use crate::frame::FrameInput;
use crate::geometry::Point;
use crate::handlers::background_effect::Support;
use crate::id::SurfaceId;
use crate::input::pointer::{PointerPos, leave_event, move_event, press_events, release_event};
use crate::state::HostState;

/// Add a stdin source to the loop that parses each line and applies it. EOF on stdin exits.
pub fn install<M: 'static>(ctx: &mut HostCtx<M>) -> Result<()> {
    let stdin: OwnedFd = std::io::stdin().as_fd().try_clone_to_owned()?;
    let mut pending = String::new();
    ctx.handle
        .insert_source(
            Generic::new(std::fs::File::from(stdin), Interest::READ, Mode::Level),
            move |_, file, ctx| {
                let mut buf = [0u8; 4096];
                let read = (&**file).read(&mut buf).unwrap_or(0);
                if read == 0 {
                    ctx.exit();
                    return Ok(PostAction::Remove);
                }
                pending.push_str(&String::from_utf8_lossy(&buf[..read]));
                for line in complete_lines(&mut pending) {
                    run_line(ctx, &line);
                }
                Ok(PostAction::Continue)
            },
        )
        .map(drop)
        .map_err(|e| HostError::EventLoop(e.to_string()))
}

/// Apply one command: synthesize the pointer events on the named surface (`input::pointer`),
/// print stats, write a capture, or exit.
pub fn apply<M: 'static>(ctx: &mut HostCtx<M>, command: DebugCommand) -> Result<()> {
    match command {
        DebugCommand::Quit => ctx.exit(),
        DebugCommand::StatsAll => {
            print_line(&host_facts(&ctx.state))?;
            let ids: Vec<SurfaceId> = ctx.state.surfaces.keys().copied().collect();
            for id in ids {
                print_stats(&mut ctx.state, id)?;
            }
        }
        DebugCommand::On { surface, action } => {
            let id = find(&ctx.state, &surface).ok_or_else(|| {
                HostError::Io(std::io::Error::other(format!("no surface {:?}", surface.0)))
            })?;
            on_surface(&mut ctx.state, id, action)?;
        }
    }
    Ok(())
}

fn run_line<M: 'static>(ctx: &mut HostCtx<M>, line: &str) {
    let applied = parse_command(line)
        .map_err(|e| HostError::Io(std::io::Error::other(e.to_string())))
        .and_then(|command| apply(ctx, command));
    if let Err(error) = applied {
        eprintln!("debug-inject: {line:?}: {error}");
    }
}

fn on_surface(state: &mut HostState, id: SurfaceId, action: Inject) -> Result<()> {
    match action {
        Inject::Hover(point) => feed(state, id, vec![move_event(pos(point), none(), mods())]),
        Inject::Click(point) => {
            let [moved, down] = press_events(pos(point), MouseEventButton::Main, none(), mods());
            let held = MouseEventButtons::Primary;
            let up = release_event(pos(point), MouseEventButton::Main, held, mods());
            feed(state, id, vec![moved, down, up]);
        }
        Inject::Leave => {
            let cleared = state
                .surfaces
                .get_mut(&id)
                .map(|surface| surface.document.clear_hover());
            if cleared == Some(Polled::Changed) {
                state.frame_input(id, FrameInput::Invalidate);
            }
            feed(state, id, vec![leave_event(mods())]);
        }
        Inject::Stats => print_stats(state, id)?,
        Inject::Capture(path) => {
            let written = crate::ctx::capture(state, id).and_then(|image| {
                image
                    .save(&path)
                    .map_err(|e| HostError::Io(std::io::Error::other(e.to_string())))
            });
            let err = written.as_ref().err().map(ToString::to_string);
            print_line(&json!({"capture": path, "ok": written.is_ok(), "err": err}))?;
        }
    }
    Ok(())
}

/// Feed `events` to surface `id`'s document. A changed hover asks for a redraw through the
/// shell provider; a Dioxus handler that ran wakes the document's waker; so neither needs an
/// invalidation here, and an event that changes nothing renders nothing.
fn feed(state: &mut HostState, id: SurfaceId, events: Vec<blitz_traits::events::UiEvent>) {
    if let Some(surface) = state.surfaces.get_mut(&id) {
        for event in events {
            surface.document.handle_event(event);
        }
    }
}

fn print_stats(state: &mut HostState, id: SurfaceId) -> Result<()> {
    let adapter = state.adapter_label();
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return Ok(());
    };
    let stats = surface.report(&adapter);
    // Each report covers the time since the previous one (FINDINGS F15).
    surface.stats.drain();
    print_line(&stats)
}

/// What the compositor offers, for the acceptance script's blur and scale checks.
fn host_facts(state: &HostState) -> serde_json::Value {
    let bound = |present: bool| if present { "bound" } else { "unbound" };
    let blur = match (&state.background_effect, state.blur_supported()) {
        (None, _) => "unbound",
        (Some(_), Support::Supported) => "supported",
        (Some(_), Support::Unsupported) => "unsupported",
    };
    json!({"host": {
        "blur": blur,
        "fractional_scale": bound(state.fractional.is_some()),
        "layer_shell": bound(state.layer_shell.is_some()),
        "adapter": state.adapter_label(),
    }})
}

fn find(state: &HostState, name: &SurfaceName) -> Option<SurfaceId> {
    state
        .surfaces
        .values()
        .find(|surface| surface.name == *name)
        .map(|surface| surface.id)
}

fn print_line(value: &impl serde::Serialize) -> Result<()> {
    let line = serde_json::to_string(value).map_err(std::io::Error::other)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()?;
    Ok(())
}

/// Take every complete line out of `pending`, leaving a trailing partial line in place.
fn complete_lines(pending: &mut String) -> Vec<String> {
    let Some(end) = pending.rfind('\n') else {
        return Vec::new();
    };
    let rest = pending.split_off(end + 1);
    let lines = std::mem::replace(pending, rest);
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

fn pos(point: Point) -> PointerPos {
    PointerPos {
        x: f64::from(point.x),
        y: f64::from(point.y),
    }
}

fn none() -> MouseEventButtons {
    MouseEventButtons::None
}

fn mods() -> Modifiers {
    Modifiers::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_lines_are_taken() {
        let cases: &[(&str, &[&str], &str)] = &[
            ("", &[], ""),
            ("dock ho", &[], "dock ho"),
            ("stats\n", &["stats"], ""),
            ("stats\n\ndock leave\ndock hov", &["stats", "dock leave"], "dock hov"),
        ];
        for (input, want, left) in cases {
            let mut pending = (*input).to_owned();
            assert_eq!(complete_lines(&mut pending), *want, "{input:?}");
            assert_eq!(pending, *left, "{input:?}");
        }
    }
}
