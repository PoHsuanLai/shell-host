//! The demos' stdin protocol: each line is a [`DebugCommand`], mapped onto
//! [`HostCtx::inject`], [`HostCtx::take_stats`] and [`HostCtx::exit`], and the result printed
//! as one JSON object per line on stdout. This is the only place the feature prints.
//!
//! Output: a `FrameStats` per surface for `stats` (the global `stats` first prints one
//! `{"host": ..}` line with the compositor facts), and `{"capture": path, "ok": .., "err": ..}`
//! for `capture`, `{"focused": name, "typed": .., "shown": .., "ime": ..}` for `focused` (`typed`
//! and `shown` null without a focused text field). Pointer commands print nothing; a failed line
//! is reported on stderr.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use calloop::generic::Generic;
use calloop::{Interest, Mode, PostAction};
use serde_json::json;

use super::{DebugAction, DebugCommand, Inject, Injected, parse_command};
use crate::ctx::HostCtx;
use crate::dom::shell_provider::ImeRequest;
use crate::error::{HostError, Result};
use crate::handlers::background_effect::Support;
use crate::id::SurfaceId;
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

/// Apply one command and print what it produced.
pub fn apply<M: 'static>(ctx: &mut HostCtx<M>, command: DebugCommand) -> Result<()> {
    match command {
        DebugCommand::Quit => ctx.exit(),
        DebugCommand::StatsAll => {
            print_line(&host_facts(&ctx.state))?;
            let ids: Vec<SurfaceId> = ctx.state.surfaces.keys().copied().collect();
            for id in ids {
                print_stats(ctx, id)?;
            }
        }
        DebugCommand::On { surface, action } => {
            let id = ctx.surface_named(&surface).ok_or_else(|| {
                HostError::Io(std::io::Error::other(format!("no surface {:?}", surface.0)))
            })?;
            on_surface(ctx, id, action)?;
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

/// A per-surface command: `stats` reads the stats, `focused` reads the text field, everything else is an [`Inject`].
fn on_surface<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId, action: DebugAction) -> Result<()> {
    match action {
        DebugAction::Stats => print_stats(ctx, id),
        DebugAction::Focused => print_line(&focused_line(&ctx.state, id)),
        DebugAction::Inject(Inject::Capture { path }) => {
            let written = ctx.inject(id, Inject::Capture { path: path.clone() });
            print_line(&capture_line(&path, &written))
        }
        DebugAction::Inject(inject) => ctx.inject(id, inject).map(drop),
    }
}

/// `{"capture": path, "ok": .., "err": ..}`, the line the acceptance script waits for.
fn capture_line(path: &std::path::Path, written: &Result<Injected>) -> serde_json::Value {
    let err = written.as_ref().err().map(ToString::to_string);
    json!({"capture": path, "ok": written.is_ok(), "err": err})
}

/// `{"focused": name, "typed": .., "shown": .., "ime": ..}`: surface `id`'s focused text field
/// and the IME request its document last made.
fn focused_line(state: &HostState, id: SurfaceId) -> serde_json::Value {
    let Some(surface) = state.surfaces.get(&id) else {
        return json!({"focused": null});
    };
    let text = surface.document.focused_text();
    let ime = match surface.shell.ime() {
        ImeRequest::Disabled => "disabled",
        ImeRequest::Enabled { .. } => "enabled",
    };
    json!({
        "focused": surface.name.0,
        "typed": text.as_ref().map(|t| &t.typed),
        "shown": text.as_ref().map(|t| &t.shown),
        "ime": ime,
    })
}

/// Print surface `id`'s stats and start a new window, so each printed line covers the time
/// since the previous one.
fn print_stats<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) -> Result<()> {
    match ctx.take_stats(id) {
        Some(stats) => print_line(&stats),
        None => Ok(()),
    }
}

/// What the compositor offers, for the acceptance script's blur and scale checks.
fn host_facts(state: &HostState) -> serde_json::Value {
    let advertised = state
        .background_effect
        .as_ref()
        .map(|effect| effect.capabilities().blur);
    let blur = match advertised {
        None => "unbound",
        Some(Support::Supported) => "supported",
        Some(Support::Unsupported) => "unsupported",
    };
    json!({"host": {
        "blur": blur,
        "fractional_scale": bound(state.fractional.as_ref()),
        "layer_shell": bound(state.layer_shell.as_ref()),
        "adapter": state.adapter_label(),
    }})
}

/// Whether the compositor's global was bound, as the acceptance script reads it.
fn bound<T>(global: Option<&T>) -> &'static str {
    global.map_or("unbound", |_| "bound")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_lines_are_taken() {
        let cases: &[(&str, &[&str], &str)] = &[
            ("", &[], ""),
            ("dock ho", &[], "dock ho"),
            ("stats\n", &["stats"], ""),
            (
                "stats\n\ndock leave\ndock hov",
                &["stats", "dock leave"],
                "dock hov",
            ),
        ];
        for (input, want, left) in cases {
            let mut pending = (*input).to_owned();
            assert_eq!(complete_lines(&mut pending), *want, "{input:?}");
            assert_eq!(pending, *left, "{input:?}");
        }
    }

    #[test]
    fn a_capture_line_says_whether_it_was_written() {
        let path = std::path::Path::new("/tmp/x.png");
        let ok: Result<Injected> = Ok(Injected::Captured {
            path: path.to_owned(),
            size: crate::scale::Size {
                width: 2,
                height: 1,
            },
        });
        let failed: Result<Injected> = Err(HostError::NoSurface(SurfaceId(3)));
        assert_eq!(capture_line(path, &ok)["ok"], json!(true));
        assert_eq!(capture_line(path, &ok)["err"], serde_json::Value::Null);
        assert_eq!(capture_line(path, &failed)["ok"], json!(false));
        assert_eq!(capture_line(path, &failed)["capture"], json!("/tmp/x.png"));
    }
}
