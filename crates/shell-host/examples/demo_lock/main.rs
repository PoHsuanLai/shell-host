//! The session lock's acceptance demo: on `lock` it locks the session
//! (`HostCtx::lock_session`) with one document per output, a plain password field
//! (`screen.rs`); typing "letmein" and Enter unlocks through the document's `LockHandle`.
//!
//! It locks nothing but the throwaway compositor `scripts/dev/accept-lock.sh` starts: the rules
//! in `guard.rs` are checked before it connects, and `lock` is refused unless they all hold.
//!
//! Every line goes to stderr as `lock: ...`, the first `lock: pid N`. stdin, one command per
//! line:
//! - `lock`: lock; prints `lock: requested {"id":N}`, `lock: refused <why>` or `lock: error
//!   <why>`, then `lock: event {"id":N,"state":"locked|unlocked|finished"}` as the host
//!   reports;
//! - `unlock`: `HostCtx::unlock_session` (the script's safety net);
//! - `stats`: `lock: stats {"phase":..,"surfaces":[{"output":..,"name":..,"frames":N,
//!   "commits":N,"logical":[w,h]}],"typed":{<output>:{"inputs":N,"length":N,"rejected":N,
//!   "accepted":N}},"focus":<surface id or null>}`;
//! - `quit`: unlock first when locked, then exit once the host reports the end.

mod guard;
#[path = "../shared/harness.rs"]
mod harness;
mod screen;

use std::io::{BufRead, Write};

use shell_host::{
    Host, HostCtx, HostEvent, HostRoot, KeyboardFocus, LockConfig, LockId, LockState, OutputInfo,
    SurfaceId,
};

use guard::Permit;
use screen::{Screen, TYPED, lock_screen};

/// Flushed at once: the acceptance reads these lines while the demo runs.
pub fn say(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "lock: {line}");
    let _ = err.flush();
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug, Clone, Copy)]
enum Command {
    Lock,
    Unlock,
    Stats,
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    match line.trim() {
        "lock" => Some(Command::Lock),
        "unlock" => Some(Command::Unlock),
        "stats" => Some(Command::Stats),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

/// The demo's own record of the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    None,
    Asked(LockId),
    Locked(LockId),
}

/// Whether `quit` is waiting for the unlock to land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaving {
    No,
    AfterUnlock,
}

#[derive(Debug)]
struct Demo {
    permit: Permit,
    held: Held,
    leaving: Leaving,
    focus: Option<SurfaceId>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_lock: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    // Before connecting: the connection reads the same variables.
    say(&format!("pid {}", std::process::id()));
    let permit = guard::permit(&guard::from_process());
    match &permit {
        Permit::Granted => say("guard granted"),
        Permit::Refused(why) => say(&format!("guard refused: {why}")),
    }
    let host = Host::<Command>::connect(harness::host_config()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match parse(&line) {
                Some(command) => {
                    let _ = handle.send(command);
                }
                None => eprintln!("demo_lock: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    let mut demo = Demo {
        permit,
        held: Held::None,
        leaving: Leaving::No,
        focus: None,
    };
    host.run(move |ctx, event| on_event(ctx, &mut demo, event))
        .map_err(|e| e.to_string())
}

fn on_event(ctx: &mut HostCtx<Command>, demo: &mut Demo, event: HostEvent<Command>) {
    match event {
        HostEvent::Started => {
            let names: Vec<String> = ctx.outputs().iter().map(output_name).collect();
            say(&format!("ready {}", json(&names)));
        }
        HostEvent::App(Command::Lock) => lock(ctx, demo),
        HostEvent::App(Command::Unlock) => {
            if let Held::Asked(id) | Held::Locked(id) = demo.held {
                ctx.unlock_session(id);
            }
        }
        HostEvent::App(Command::Stats) => stats(ctx, demo),
        HostEvent::App(Command::Quit) | HostEvent::QuitAsked => match demo.held {
            Held::None => ctx.exit(),
            Held::Asked(id) | Held::Locked(id) => {
                demo.leaving = Leaving::AfterUnlock;
                ctx.unlock_session(id);
            }
        },
        HostEvent::Lock { id, state } => {
            say(&format!(
                "event {{\"id\":{},\"state\":{}}}",
                id.0,
                json(&state)
            ));
            demo.held = match state {
                LockState::Locked => Held::Locked(id),
                LockState::Unlocked | LockState::Finished => Held::None,
            };
            if demo.held == Held::None && demo.leaving == Leaving::AfterUnlock {
                ctx.exit();
            }
        }
        HostEvent::KeyboardFocus { id, focus } => {
            demo.focus = match focus {
                KeyboardFocus::Gained => Some(id),
                KeyboardFocus::Lost => demo.focus.filter(|held| *held != id),
            };
            say(&format!("focus {} {focus:?}", id.0));
        }
        _ => {}
    }
}

fn lock(ctx: &mut HostCtx<Command>, demo: &mut Demo) {
    if let Permit::Refused(why) = &demo.permit {
        say(&format!("refused {why}"));
        return;
    }
    let factory = |info: OutputInfo| {
        HostRoot::with_props(
            lock_screen,
            Screen {
                output: output_name(&info),
            },
        )
    };
    match ctx.lock_session(LockConfig::default(), factory) {
        Ok(id) => {
            demo.held = Held::Asked(id);
            say(&format!("requested {{\"id\":{}}}", id.0));
        }
        Err(error) => say(&format!("error {error}")),
    }
}

fn stats(ctx: &mut HostCtx<Command>, demo: &Demo) {
    let (phase, id) = match demo.held {
        Held::None => ("none", None),
        Held::Asked(id) => ("requested", Some(id)),
        Held::Locked(id) => ("locked", Some(id)),
    };
    let surfaces: Vec<serde_json::Value> = id
        .map(|id| ctx.lock_surfaces(id))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(output, surface)| {
            let stats = ctx.stats(surface)?;
            Some(serde_json::json!({
                "output": output.0,
                "surface": surface.0,
                "name": stats.surface.0,
                "frames": stats.counters.frames,
                "commits": stats.counters.commits,
                "logical": [stats.logical.width, stats.logical.height],
                "buffer": [stats.buffer.width, stats.buffer.height],
            }))
        })
        .collect();
    let typed = TYPED.lock().map(|typed| typed.clone()).unwrap_or_default();
    let line = serde_json::json!({
        "phase": phase,
        "surfaces": surfaces,
        "typed": typed,
        "focus": demo.focus.map(|id| id.0),
    });
    say(&format!("stats {line}"));
}

fn output_name(info: &OutputInfo) -> String {
    info.name.clone().unwrap_or_else(|| info.id.0.to_string())
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| format!("\"{error}\""))
}
