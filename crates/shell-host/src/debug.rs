//! Frame statistics and the debug command language.
//!
//! `FrameStats` is what `HostCtx::stats` returns and what the `debug-inject` feature prints as
//! one JSON line per surface: the spike's `stats` set (SPIKE.md S1, "API changes"), with the
//! work time (trigger to present) kept apart from the present-to-present interval, because the
//! interval's p95 is only the refresh period and says nothing about cost.
//!
//! Debug commands address one surface by name (`dock hover 60 40`), so a process with a bar and
//! a dock can be driven per surface; `capture` renders offscreen at the buffer size, the only
//! exact test of fractional scale.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

/// The name a debug command addresses a surface by: the layer namespace, the toplevel app id,
/// or `<parent>/popup` for a parent's open popup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceName(pub String);

/// Nearest-rank percentiles of a set of durations, in microseconds (integers, so the stats
/// stay `Eq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50_us: u32,
    pub p95_us: u32,
    pub max_us: u32,
}

/// A width and height in pixels, logical or physical by field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// How frames reach the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentMode {
    Mailbox,
    Fifo,
    Immediate,
    /// The CPU backend's `wl_shm` buffers.
    Shm,
}

/// How the compositor blends the surface's alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    PreMultiplied,
    PostMultiplied,
    /// No usable alpha mode: the surface is opaque and its blur is turned off.
    Opaque,
}

/// Cumulative counters for one surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FrameCounters {
    /// Frames rendered and presented.
    pub frames: u64,
    /// Frame callbacks received.
    pub callbacks: u64,
    /// `wl_surface.commit`s (every present is exactly one commit, SPIKE.md S1 finding 3).
    pub commits: u64,
    /// Frames that were due but could not be presented.
    pub skipped: u64,
    /// Watchdog expiries (a compositor that stopped calling back).
    pub watchdog_fired: u64,
    /// Frame ticks delivered to the document (`tick`): frames due while it was animating or
    /// a `FrameDrive` was held.
    #[serde(default)]
    pub ticks: u64,
    /// Renders that ran `HostConfig::prepaint`'s hook after resolving: equal to the frames
    /// resolved with a hook installed (presented or not), and zero without one. A frame that
    /// ran it twice, or an idle period that moves it, is a bug.
    #[serde(default)]
    pub prepaints: u64,
}

/// Everything `stats` reports about one surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameStats {
    pub surface: SurfaceName,
    pub counters: FrameCounters,
    /// Trigger to present, over the window since the previous report.
    pub work: Percentiles,
    /// Present to present, over the same window.
    pub interval: Percentiles,
    /// How many frames the percentiles cover.
    pub samples: u32,
    /// The surface's scale in 120ths.
    pub scale120: u32,
    pub logical: Dimensions,
    pub buffer: Dimensions,
    pub alpha_mode: AlphaMode,
    pub present_mode: PresentMode,
    /// `name (backend, driver)` of the one adapter.
    pub adapter: String,
    /// GPU devices created by this process; stays 1 however many surfaces there are.
    pub devices_created: u32,
    /// Resident set size of the process, KiB.
    pub vm_rss_kib: u64,
}

/// The samples and counters behind one surface's [`FrameStats`], accumulated between reports.
#[derive(Debug, Clone, Default)]
pub struct StatsWindow {
    counters: FrameCounters,
    work_us: Vec<u32>,
    interval_us: Vec<u32>,
    last_present: Option<Instant>,
}

impl StatsWindow {
    /// Record a presented frame whose work took `work`, presented at `now`.
    pub fn presented(&mut self, work: Duration, now: Instant) {
        self.counters.frames += 1;
        self.counters.commits += 1;
        self.work_us.push(micros(work));
        if let Some(previous) = self.last_present {
            self.interval_us
                .push(micros(now.saturating_duration_since(previous)));
        }
        self.last_present = Some(now);
    }

    /// Record a frame callback.
    pub fn callback(&mut self) {
        self.counters.callbacks += 1;
    }

    /// Record a frame that could not be presented.
    pub fn skipped(&mut self) {
        self.counters.skipped += 1;
    }

    /// Record a frame tick.
    pub fn ticked(&mut self) {
        self.counters.ticks += 1;
    }

    /// Record a render that ran the pre-paint hook.
    pub fn prepainted(&mut self) {
        self.counters.prepaints += 1;
    }

    /// Record a watchdog expiry.
    pub fn watchdog_fired(&mut self) {
        self.counters.watchdog_fired += 1;
    }

    /// The counters so far.
    pub fn counters(&self) -> FrameCounters {
        self.counters
    }

    /// The work and interval percentiles and sample count since the last drain.
    pub fn peek(&self) -> (Percentiles, Percentiles, u32) {
        let samples = u32::try_from(self.work_us.len()).unwrap_or(u32::MAX);
        (
            percentiles(&self.work_us),
            percentiles(&self.interval_us),
            samples,
        )
    }

    /// [`StatsWindow::peek`], then clear the samples (counters stay cumulative). The debug
    /// `stats` command drains, so each report covers the time since the previous one.
    pub fn drain(&mut self) -> (Percentiles, Percentiles, u32) {
        let report = self.peek();
        self.work_us.clear();
        self.interval_us.clear();
        report
    }
}

/// Nearest-rank p50, p95 and max of `samples`; all zero when there are none.
pub fn percentiles(samples: &[u32]) -> Percentiles {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = |q_percent: usize| -> u32 {
        match sorted.len() {
            0 => 0,
            n => sorted[(q_percent * n).div_ceil(100).clamp(1, n) - 1],
        }
    };
    Percentiles {
        p50_us: rank(50),
        p95_us: rank(95),
        max_us: rank(100),
    }
}

/// The `VmRSS` line of a `/proc/<pid>/status` text, in KiB.
pub fn vm_rss_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.trim().strip_suffix("kB"))
        .and_then(|value| value.trim().parse().ok())
}

fn micros(d: Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
}

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

/// A pointer button, as `debug click` and `Inject::Click` name it: the Linux evdev codes
/// `input::pointer::button_from_evdev` maps to a DOM button. `Primary` is `BTN_LEFT` (272),
/// `Secondary` is `BTN_RIGHT` (273), `Middle` is `BTN_MIDDLE` (274).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerButton {
    #[default]
    Primary,
    Secondary,
    Middle,
}

/// What a debug command does to its surface. Coordinates are surface-local logical pixels.
///
/// `Press`, `Hover` (as a held move) and `Release` script a drag: `Press` at the start point,
/// `Hover` at each point in between (still with the button down, so it moves like one), then
/// `Release` at the end point. `Click` is `Press` immediately followed by `Release` at the same
/// point, with no move between (FINDINGS F70; a real drag cannot be scripted through `Click`,
/// sill's G330).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugAction {
    /// A pointer motion to the point (a plain hover with nothing pressed, or a held move
    /// between a `Press` and a `Release`).
    Hover(Point),
    /// `button` presses at the point.
    Press(Point, PointerButton),
    /// `button` releases at the point.
    Release(Point, PointerButton),
    /// Move, press and release `button` at the point, with no move in between.
    Click(Point, PointerButton),
    /// The pointer leaves the surface.
    Leave,
    /// Print this surface's stats.
    Stats,
    /// Render offscreen at the buffer size and write a PNG.
    Capture(PathBuf),
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
    #[error("{0:?} is not a pointer button (primary, secondary or middle)")]
    NotAButton(String),
}

/// Parse one line: `stats`, `quit`, or `<surface> hover X Y`, `<surface> press X Y [button]`,
/// `<surface> release X Y [button]`, `<surface> click X Y [button]` (`button` is `primary`,
/// `secondary` or `middle`; primary when omitted — a press, one or more `hover`s and a release
/// script a drag, FINDINGS F70), `<surface> leave`, `<surface> stats`, `<surface> capture PATH`.
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
    match (verb, args) {
        ("hover", _) => point("hover", args).map(DebugAction::Hover),
        ("press", _) => {
            button_at("press", args).map(|(point, button)| DebugAction::Press(point, button))
        }
        ("release", _) => {
            button_at("release", args).map(|(point, button)| DebugAction::Release(point, button))
        }
        ("click", _) => {
            button_at("click", args).map(|(point, button)| DebugAction::Click(point, button))
        }
        ("leave", []) => Ok(DebugAction::Leave),
        ("stats", []) => Ok(DebugAction::Stats),
        ("capture", [path]) => Ok(DebugAction::Capture(PathBuf::from(path))),
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
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let cases: &[(&[u32], Percentiles)] = &[
            (&[], Percentiles::default()),
            (
                &[3],
                Percentiles {
                    p50_us: 3,
                    p95_us: 3,
                    max_us: 3,
                },
            ),
            (
                &[4, 1, 3, 2],
                Percentiles {
                    p50_us: 2,
                    p95_us: 4,
                    max_us: 4,
                },
            ),
            (
                &[
                    10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170,
                    180, 190, 200,
                ],
                Percentiles {
                    p50_us: 100,
                    p95_us: 190,
                    max_us: 200,
                },
            ),
        ];
        for (samples, want) in cases {
            assert_eq!(percentiles(samples), *want, "{samples:?}");
        }
    }

    #[test]
    fn the_window_separates_work_from_interval() {
        let t0 = Instant::now();
        let mut w = StatsWindow::default();
        w.presented(Duration::from_micros(900), t0);
        w.callback();
        w.presented(
            Duration::from_micros(1100),
            t0 + Duration::from_micros(16_700),
        );
        w.callback();
        w.skipped();
        assert_eq!(w.peek(), w.clone().drain());
        let (work, interval, samples) = w.drain();
        assert_eq!(samples, 2);
        assert_eq!(
            work,
            Percentiles {
                p50_us: 900,
                p95_us: 1100,
                max_us: 1100
            }
        );
        assert_eq!(
            interval,
            Percentiles {
                p50_us: 16_700,
                p95_us: 16_700,
                max_us: 16_700
            }
        );
        assert_eq!(
            w.counters(),
            FrameCounters {
                frames: 2,
                callbacks: 2,
                commits: 2,
                skipped: 1,
                watchdog_fired: 0,
                ticks: 0,
                prepaints: 0
            }
        );
        // Draining clears the samples, not the counters.
        assert_eq!(
            w.drain(),
            (Percentiles::default(), Percentiles::default(), 0)
        );
        assert_eq!(w.counters().frames, 2);
    }

    #[test]
    fn vm_rss_is_read_from_proc_status() {
        let status = "Name:\tspike_bar\nVmPeak:\t  999 kB\nVmRSS:\t  172800 kB\nThreads:\t9\n";
        assert_eq!(vm_rss_kib(status), Some(172_800));
        assert_eq!(vm_rss_kib("Name:\tx\n"), None);
    }

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
                Ok(on("dock", DebugAction::Hover(Point { x: 60, y: 40 }))),
            ),
            (
                "sill-bar click 5 -1",
                Ok(on(
                    "sill-bar",
                    DebugAction::Click(Point { x: 5, y: -1 }, PointerButton::Primary),
                )),
            ),
            (
                "dock click 60 64 secondary",
                Ok(on(
                    "dock",
                    DebugAction::Click(Point { x: 60, y: 64 }, PointerButton::Secondary),
                )),
            ),
            (
                "dock click 60 64 middle",
                Ok(on(
                    "dock",
                    DebugAction::Click(Point { x: 60, y: 64 }, PointerButton::Middle),
                )),
            ),
            (
                "dock press 60 40",
                Ok(on(
                    "dock",
                    DebugAction::Press(Point { x: 60, y: 40 }, PointerButton::Primary),
                )),
            ),
            (
                "dock press 60 40 secondary",
                Ok(on(
                    "dock",
                    DebugAction::Press(Point { x: 60, y: 40 }, PointerButton::Secondary),
                )),
            ),
            (
                "dock release 70 50",
                Ok(on(
                    "dock",
                    DebugAction::Release(Point { x: 70, y: 50 }, PointerButton::Primary),
                )),
            ),
            (
                "dock release 70 50 middle",
                Ok(on(
                    "dock",
                    DebugAction::Release(Point { x: 70, y: 50 }, PointerButton::Middle),
                )),
            ),
            ("dock leave", Ok(on("dock", DebugAction::Leave))),
            ("bar stats", Ok(on("bar", DebugAction::Stats))),
            (
                "bar capture /tmp/a.png",
                Ok(on("bar", DebugAction::Capture("/tmp/a.png".into()))),
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
