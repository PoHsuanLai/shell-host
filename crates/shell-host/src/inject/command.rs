//! The debug command language: one line addresses one surface by name (`dock hover 60 40`), so a
//! process with a bar and a dock can be driven per surface; `capture` renders offscreen at the
//! buffer size, the only exact test of fractional scale.

use std::path::PathBuf;

use super::key_spec::parse_key;
use super::{Inject, KeyAction, PointerButton};
use crate::geometry::Point;
use crate::id::SurfaceName;

/// One line of the debug command language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    /// Act on one surface.
    On {
        surface: SurfaceName,
        action: DebugAction,
    },
    /// Print stats for every surface.
    StatsAll,
    /// Close everything and leave `Host::run`.
    Quit,
}

/// What a debug command does to its surface. Coordinates are surface-local logical pixels.
///
/// Everything but `Stats` and `Focused` is an [`Inject`]; see it for how `PointerDown`, `Pointer`
/// and `PointerUp` script a drag (`press`, `hover`, `release` on the line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugAction {
    /// Do this to the surface as if the user had.
    Inject(Inject),
    /// Print this surface's stats.
    Stats,
    /// Print the focused text field's text, as typed and as shown (with any composition), and
    /// what the IME was last asked for.
    Focused,
}

/// Why a debug line was not understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("empty line")]
    Empty,
    #[error("unknown command {0:?}")]
    Unknown(String),
    #[error("{verb} needs {needs}")]
    Missing {
        verb: &'static str,
        needs: &'static str,
    },
    #[error("{0:?} is not an integer coordinate")]
    NotACoordinate(String),
    #[error("{0:?} is not a key (a W3C key name, or Space, optionally +shift/ctrl/alt/super)")]
    NotAKey(String),
    #[error("{0:?} is not a modifier (shift, ctrl, alt or super)")]
    NotAModifier(String),
    #[error("{0:?} is not a key action (down or up)")]
    NotAKeyAction(String),
    #[error("{0:?} is not a pointer button (primary, secondary or middle)")]
    NotAButton(String),
}

/// Parse one line: `stats`, `quit`, or `<surface> hover X Y`, `<surface> press X Y [button]`,
/// `<surface> release X Y [button]`, `<surface> click X Y [button]` (`button` is `primary`,
/// `secondary` or `middle`; primary when omitted — a press, one or more `hover`s and a release
/// script a drag), `<surface> leave`, `<surface> stats`, `<surface> capture PATH`,
/// `<surface> focused`, `<surface> key <name>[+mods] [down|up]` (`Tab`, `k+ctrl`, `Tab+shift up`;
/// a tap when neither `down` nor `up` is given).
/// Global verbs win, so no surface may be named `stats` or `quit`.
pub fn parse_command(line: &str) -> Result<DebugCommand, CommandError> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        [] => Err(CommandError::Empty),
        ["stats"] => Ok(DebugCommand::StatsAll),
        ["quit"] => Ok(DebugCommand::Quit),
        [surface, verb, rest @ ..] => Ok(DebugCommand::On {
            surface: SurfaceName((*surface).to_owned()),
            action: parse_action(verb, rest)?,
        }),
        [word] => Err(CommandError::Unknown((*word).to_owned())),
    }
}

fn parse_action(verb: &str, args: &[&str]) -> Result<DebugAction, CommandError> {
    let inject = |inject| Ok(DebugAction::Inject(inject));
    match (verb, args) {
        ("hover", _) => point("hover", args).and_then(|at| inject(Inject::Pointer { at })),
        ("press", _) => button_at("press", args)
            .and_then(|(at, button)| inject(Inject::PointerDown { at, button })),
        ("release", _) => button_at("release", args)
            .and_then(|(at, button)| inject(Inject::PointerUp { at, button })),
        ("click", _) => {
            button_at("click", args).and_then(|(at, button)| inject(Inject::Click { at, button }))
        }
        ("key", [spec]) => inject(Inject::Key {
            stroke: parse_key(spec)?,
            action: KeyAction::Tap,
        }),
        ("key", [spec, action]) => inject(Inject::Key {
            stroke: parse_key(spec)?,
            action: key_action(action)?,
        }),
        ("key", _) => Err(CommandError::Missing {
            verb: "key",
            needs: "a key name, optionally down or up",
        }),
        ("leave", []) => inject(Inject::Leave),
        ("stats", []) => Ok(DebugAction::Stats),
        ("focused", []) => Ok(DebugAction::Focused),
        ("capture", [path]) => inject(Inject::Capture {
            path: PathBuf::from(path),
        }),
        ("capture", _) => Err(CommandError::Missing {
            verb: "capture",
            needs: "one path",
        }),
        _ => Err(CommandError::Unknown(verb.to_owned())),
    }
}

fn point(verb: &'static str, args: &[&str]) -> Result<Point, CommandError> {
    let [x, y] = args else {
        return Err(CommandError::Missing {
            verb,
            needs: "X and Y",
        });
    };
    let coord = |s: &str| {
        s.parse()
            .map_err(|_| CommandError::NotACoordinate(s.to_owned()))
    };
    Ok(Point {
        x: coord(x)?,
        y: coord(y)?,
    })
}

/// `<verb> X Y` (primary) or `<verb> X Y <button>`: the grammar `press`, `release` and `click`
/// share.
fn button_at(verb: &'static str, args: &[&str]) -> Result<(Point, PointerButton), CommandError> {
    match args {
        [x, y] => Ok((point(verb, &[x, y])?, PointerButton::Primary)),
        [x, y, button] => Ok((point(verb, &[x, y])?, parse_button(button)?)),
        _ => Err(CommandError::Missing {
            verb,
            needs: "X and Y, optionally a button",
        }),
    }
}

fn key_action(word: &str) -> Result<KeyAction, CommandError> {
    match word {
        "down" => Ok(KeyAction::Press),
        "up" => Ok(KeyAction::Release),
        _ => Err(CommandError::NotAKeyAction(word.to_owned())),
    }
}

fn parse_button(word: &str) -> Result<PointerButton, CommandError> {
    match word {
        "primary" => Ok(PointerButton::Primary),
        "secondary" => Ok(PointerButton::Secondary),
        "middle" => Ok(PointerButton::Middle),
        _ => Err(CommandError::NotAButton(word.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use crate::inject::KeyStroke;
    use keyboard_types::{Code, Key, Modifiers};

    use super::*;

    #[test]
    fn commands_parse_per_surface() {
        let on = |surface: &str, action| DebugCommand::On {
            surface: SurfaceName(surface.to_owned()),
            action,
        };
        let cases: Vec<(&str, Result<DebugCommand, CommandError>)> = vec![
            ("stats", Ok(DebugCommand::StatsAll)),
            ("  quit  ", Ok(DebugCommand::Quit)),
            (
                "dock hover 60 40",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::Pointer {
                        at: Point { x: 60, y: 40 },
                    }),
                )),
            ),
            (
                "sill-bar click 5 -1",
                Ok(on(
                    "sill-bar",
                    DebugAction::Inject(Inject::Click {
                        at: Point { x: 5, y: -1 },
                        button: PointerButton::Primary,
                    }),
                )),
            ),
            (
                "dock click 60 64 secondary",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::Click {
                        at: Point { x: 60, y: 64 },
                        button: PointerButton::Secondary,
                    }),
                )),
            ),
            (
                "dock click 60 64 middle",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::Click {
                        at: Point { x: 60, y: 64 },
                        button: PointerButton::Middle,
                    }),
                )),
            ),
            (
                "dock press 60 40",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::PointerDown {
                        at: Point { x: 60, y: 40 },
                        button: PointerButton::Primary,
                    }),
                )),
            ),
            (
                "dock press 60 40 secondary",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::PointerDown {
                        at: Point { x: 60, y: 40 },
                        button: PointerButton::Secondary,
                    }),
                )),
            ),
            (
                "dock release 70 50",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::PointerUp {
                        at: Point { x: 70, y: 50 },
                        button: PointerButton::Primary,
                    }),
                )),
            ),
            (
                "dock release 70 50 middle",
                Ok(on(
                    "dock",
                    DebugAction::Inject(Inject::PointerUp {
                        at: Point { x: 70, y: 50 },
                        button: PointerButton::Middle,
                    }),
                )),
            ),
            (
                "dock leave",
                Ok(on("dock", DebugAction::Inject(Inject::Leave))),
            ),
            ("bar/popup stats", Ok(on("bar/popup", DebugAction::Stats))),
            (
                "power-menu key Escape",
                Ok(on(
                    "power-menu",
                    DebugAction::Inject(Inject::Key {
                        stroke: KeyStroke {
                            key: Key::Escape,
                            code: Code::Escape,
                            mods: Modifiers::empty(),
                        },
                        action: KeyAction::Tap,
                    }),
                )),
            ),
            (
                "launcher key k+ctrl down",
                Ok(on(
                    "launcher",
                    DebugAction::Inject(Inject::Key {
                        stroke: KeyStroke {
                            key: Key::Character("k".into()),
                            code: Code::KeyK,
                            mods: Modifiers::CONTROL,
                        },
                        action: KeyAction::Press,
                    }),
                )),
            ),
            (
                "launcher key Tab up",
                Ok(on(
                    "launcher",
                    DebugAction::Inject(Inject::Key {
                        stroke: KeyStroke {
                            key: Key::Tab,
                            code: Code::Tab,
                            mods: Modifiers::empty(),
                        },
                        action: KeyAction::Release,
                    }),
                )),
            ),
            (
                "dock key",
                Err(CommandError::Missing {
                    verb: "key",
                    needs: "a key name, optionally down or up",
                }),
            ),
            (
                "dock key Tab sideways",
                Err(CommandError::NotAKeyAction("sideways".into())),
            ),
            ("dock key Tabb", Err(CommandError::NotAKey("Tabb".into()))),
            ("bar stats", Ok(on("bar", DebugAction::Stats))),
            (
                "sill-launcher-Virtual-1 focused",
                Ok(on("sill-launcher-Virtual-1", DebugAction::Focused)),
            ),
            (
                "bar capture /tmp/a.png",
                Ok(on(
                    "bar",
                    DebugAction::Inject(Inject::Capture {
                        path: "/tmp/a.png".into(),
                    }),
                )),
            ),
            ("", Err(CommandError::Empty)),
            ("hover", Err(CommandError::Unknown("hover".into()))),
            (
                "dock hover 1",
                Err(CommandError::Missing {
                    verb: "hover",
                    needs: "X and Y",
                }),
            ),
            (
                "dock hover 1.5 2",
                Err(CommandError::NotACoordinate("1.5".into())),
            ),
            (
                "dock click 1 2 sideways",
                Err(CommandError::NotAButton("sideways".into())),
            ),
            (
                "dock click 1",
                Err(CommandError::Missing {
                    verb: "click",
                    needs: "X and Y, optionally a button",
                }),
            ),
            (
                "dock press 1",
                Err(CommandError::Missing {
                    verb: "press",
                    needs: "X and Y, optionally a button",
                }),
            ),
            (
                "dock press 1 2 sideways",
                Err(CommandError::NotAButton("sideways".into())),
            ),
            (
                "dock release 1",
                Err(CommandError::Missing {
                    verb: "release",
                    needs: "X and Y, optionally a button",
                }),
            ),
            (
                "dock capture",
                Err(CommandError::Missing {
                    verb: "capture",
                    needs: "one path",
                }),
            ),
            ("dock fly", Err(CommandError::Unknown("fly".into()))),
            ("dock leave now", Err(CommandError::Unknown("leave".into()))),
        ];
        for (line, want) in cases {
            assert_eq!(parse_command(line), want, "{line:?}");
        }
    }
}
