//! A client-decorated toplevel (FINDINGS F46-F49): a coloured titlebar and a body, drawn by the
//! document, with the host's invisible resize band around them.
//!
//! - Pressing the titlebar starts a move (`SurfaceHandle::begin_move`); double-clicking it
//!   toggles maximize. Each attempt prints `{"event":"begin_move","ok":..,"err":..}`.
//! - Every configure state change prints `{"event":"state",..}` from `use_toplevel_state`.
//! - stdin, one command per line: `move`, `resize <edge>` (top, bottom, left, right, top-left,
//!   top-right, bottom-left, bottom-right), `maximize`, `unmaximize`, `fullscreen`,
//!   `unfullscreen`, `minimize`, `stats` (the window's `FrameStats` line), `quit`. `move` and
//!   `resize` go through the same `SurfaceHandle` calls, so they need a button held on the
//!   window (a real or synthetic press), and print the refusal otherwise.

use std::cell::RefCell;
use std::io::{BufRead, Write};

use dioxus::prelude::*;
use serde_json::json;
use shell_host::{
    Decorations, Fullscreen, GrabRefused, Host, HostConfig, HostEvent, Maximize, ResizeEdge, Root,
    Size, SurfaceHandle, SurfaceId, ToplevelConfig, use_surface, use_toplevel_state,
};

/// The titlebar's colour, which the acceptance finds on screen to locate the frame.
const CSS: &str = "html,body,#main{margin:0;padding:0;height:100%}\
.win{display:flex;flex-direction:column;height:100%;user-select:none}\
.title{height:32px;flex:none;background:rgb(230,60,40);color:white;font:14px sans-serif;\
display:flex;align-items:center;padding-left:12px}\
.body{flex:1;background:rgb(40,90,200)}";

thread_local! {
    /// The window's handle, for stdin commands: the host closure and the document share the
    /// main thread.
    static WINDOW: RefCell<Option<SurfaceHandle>> = const { RefCell::new(None) };
}

fn print(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn grab_line(what: &str, result: Result<(), GrabRefused>) -> serde_json::Value {
    let err = result.err().map(|e| format!("{e:?}"));
    json!({"event": what, "ok": err.is_none(), "err": err})
}

fn window() -> Element {
    let surface = use_surface();
    let state = use_toplevel_state();
    use_hook({
        let surface = surface.clone();
        move || WINDOW.with(|slot| *slot.borrow_mut() = Some(surface))
    });
    use_effect(move || {
        let s = state();
        print(json!({"event": "state",
            "maximized": format!("{:?}", s.maximized),
            "fullscreen": format!("{:?}", s.fullscreen),
            "activated": format!("{:?}", s.activated),
            "tiled": format!("{:?}", s.tiled.any())}));
    });
    let drag = surface.clone();
    let zoom = surface.clone();
    rsx! {
        style { "{CSS}" }
        div { class: "win",
            div {
                class: "title",
                onmousedown: move |_| print(grab_line("begin_move", drag.begin_move())),
                ondoubleclick: move |_| {
                    let next = match zoom.current_toplevel_state().maximized {
                        Maximize::Set => Maximize::Unset,
                        Maximize::Unset => Maximize::Set,
                    };
                    zoom.set_maximized(next);
                },
                "demo_window"
            }
            div { class: "body" }
        }
    }
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug)]
enum Command {
    Move,
    Resize(ResizeEdge),
    Maximize(Maximize),
    Fullscreen(Fullscreen),
    Minimize,
    Stats,
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    let edge = |name: &str| match name {
        "top" => Some(ResizeEdge::Top),
        "bottom" => Some(ResizeEdge::Bottom),
        "left" => Some(ResizeEdge::Left),
        "right" => Some(ResizeEdge::Right),
        "top-left" => Some(ResizeEdge::TopLeft),
        "top-right" => Some(ResizeEdge::TopRight),
        "bottom-left" => Some(ResizeEdge::BottomLeft),
        "bottom-right" => Some(ResizeEdge::BottomRight),
        _ => None,
    };
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["move"] => Some(Command::Move),
        ["resize", name] => edge(name).map(Command::Resize),
        ["maximize"] => Some(Command::Maximize(Maximize::Set)),
        ["unmaximize"] => Some(Command::Maximize(Maximize::Unset)),
        ["fullscreen"] => Some(Command::Fullscreen(Fullscreen::Set)),
        ["unfullscreen"] => Some(Command::Fullscreen(Fullscreen::Unset)),
        ["minimize"] => Some(Command::Minimize),
        ["stats"] => Some(Command::Stats),
        ["quit"] => Some(Command::Quit),
        _ => None,
    }
}

fn apply(command: Command) {
    WINDOW.with(|slot| {
        let Some(window) = slot.borrow().clone() else {
            return;
        };
        match command {
            Command::Move => print(grab_line("move", window.begin_move())),
            Command::Resize(edge) => print(grab_line("resize", window.begin_resize(edge))),
            Command::Maximize(maximize) => window.set_maximized(maximize),
            Command::Fullscreen(fullscreen) => window.set_fullscreen(fullscreen),
            Command::Minimize => window.set_minimized(),
            Command::Stats | Command::Quit => {}
        }
    });
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_window: {error}");
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
                None => eprintln!("demo_window: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    let mut id: Option<SurfaceId> = None;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            let config = ToplevelConfig {
                app_id: "demo_window".to_owned(),
                title: "demo_window".to_owned(),
                size: Size {
                    width: 480,
                    height: 320,
                },
                min_size: Some(Size {
                    width: 240,
                    height: 160,
                }),
                max_size: None,
                decorations: Decorations::CLIENT,
            };
            match ctx.spawn_toplevel(config, Root::new(window)) {
                Ok(spawned) => id = Some(spawned),
                Err(error) => {
                    eprintln!("window: {error}");
                    ctx.exit();
                }
            }
        }
        HostEvent::App(Command::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        HostEvent::App(Command::Stats) => {
            if let Some(stats) = id.and_then(|id| ctx.take_stats(id)) {
                print(serde_json::to_value(&stats).unwrap_or_default());
            }
        }
        HostEvent::App(command) => apply(command),
        _ => {}
    })
    .map_err(|e| e.to_string())
}
