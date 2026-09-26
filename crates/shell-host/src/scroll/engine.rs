//! The scroll state machine (design/11 §11.4): pure. The host reads the latched scroller's
//! geometry from the laid-out document, calls [`step`] with an input and the time, and performs
//! the returned effects (writes the raw offsets, notifies). Every animation is a closed form of
//! the time since its phase began (`scroll::animate`), so `now` can be advanced by hand.

use crate::scroll::animate;
use crate::scroll::config::{Momentum, ScrollConfig};
use crate::scroll::geom::{Dir, Geom, Latch, Px};
use crate::scroll::latch::Latched;
use crate::scroll::momentum::Glide;
use crate::scroll::rubber::{AtEdge, Band, SnapBack};
use crate::scroll::velocity::{Release, SampleTime, Samples};
use crate::tick::HostInstant;

/// The engine's constants, from a [`ScrollConfig`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Physics {
    pub glide: Glide,
    pub band: Band,
    pub snap: SnapBack,
    pub release: Release,
    pub momentum: Momentum,
    /// A release slower than this (px/s) does not glide.
    pub start: f64,
    /// A release is capped at this (px/s).
    pub cap: f64,
    /// A same-direction flick during a glide at least this fast (px/s) adds its speed.
    pub boost_min: f64,
}

impl Physics {
    /// The physics `config` tunes.
    pub fn from_config(config: &ScrollConfig) -> Physics {
        let s = &config.scroll;
        Physics {
            glide: Glide::from_settings(s),
            band: Band::from_settings(s),
            snap: SnapBack::from_settings(s),
            release: Release {
                window: s.velocity_window_ms.duration(),
                held_still: s.velocity_held_still_ms.duration(),
            },
            momentum: s.momentum,
            start: s.momentum_start_px_s.get(),
            cap: s.momentum_cap_px_s.get(),
            boost_min: s.momentum_boost_threshold_px_s.get(),
        }
    }
}

/// Whether a gesture may glide and stretch (a finger) or only track (a trackpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kinetic {
    Momentum,
    Plain,
}

/// Whether a programmatic jump animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animate {
    Smooth,
    Instant,
}

/// One input to the engine. Deltas are in offset space (natural direction already applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollIn {
    /// Fingers touched: stop any glide, rebound or animation where it is (touch to stop).
    MayBegin,
    /// A gesture's axis is known and its scroller latched, moving toward `dir`.
    Began {
        latched: Latched,
        dir: Dir,
        kinetic: Kinetic,
    },
    /// The fingers moved by `delta` at `at` (the source's own timeline).
    Changed { delta: Px, at: SampleTime },
    /// The fingers lifted at `at`.
    Ended { at: SampleTime },
    /// A click or a palm ended the gesture: no glide.
    Cancelled,
    /// A discrete step by `by` (wheel detent, arrow, page), smoothed; steps accumulate.
    Step { latch: Latch, by: Px },
    /// A jump to `to` (Home/End, a command).
    Jump {
        latch: Latch,
        to: Px,
        animate: Animate,
    },
    /// An arrow key repeats: ramp up by `step` per press.
    Hold { latch: Latch, step: Px },
    /// The held arrow was released.
    Release,
    /// A frame is being rendered at `now`.
    Frame,
}

/// What the host must do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollOut {
    /// Write `raw` as `latch`'s offset (no clamping: a stretched scroller is out of range).
    SetOffset { latch: Latch, raw: Px },
    /// A phase boundary, for `use_scroll`.
    Notify { latch: Latch, phase: NotifyPhase },
}

/// The phase boundaries the engine reports (design/11 R1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyPhase {
    Began,
    Ended,
    MomentumBegan,
    MomentumEnded,
}

/// Whether the engine needs frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Animating,
    Still,
}

/// The fingers are down on a latched scroller.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub latch: Latch,
    pub kinetic: Kinetic,
    pub began: AtEdge,
    /// Where the fingers have taken the offset, unclamped: past an edge by the overscroll `O`.
    pub pos: f64,
    pub samples: Samples,
    /// Signed px/s added to the release (a flick boosting a glide).
    pub boost: f64,
}

/// A glide after release: signed `v0` from `x0` at `t0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub latch: Latch,
    pub v0: f64,
    pub x0: f64,
    pub t0: HostInstant,
}

/// A spring back to the `edge` from stretch `s0` moving outward at `v0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rebound {
    pub latch: Latch,
    pub edge: Dir,
    pub s0: f64,
    pub v0: f64,
    pub t0: HostInstant,
}

/// A smooth step from `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smooth {
    pub latch: Latch,
    pub from: f64,
    pub to: f64,
    pub t0: HostInstant,
    pub dur: f64,
}

/// A held arrow ramping from `x0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    pub latch: Latch,
    pub step: f64,
    pub x0: f64,
    pub t0: HostInstant,
}

/// The released arrow's spring toward `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub latch: Latch,
    pub x0: f64,
    pub v0: f64,
    pub target: f64,
    pub t0: HostInstant,
}

/// Fingers are down but no axis yet; the glide they stopped is remembered for a boost.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pending {
    pub last: Option<Latch>,
    /// The stopped glide's signed speed, px/s.
    pub carry: f64,
}

/// The engine (design/11 §11.4).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Engine {
    #[default]
    Idle,
    Pending(Pending),
    Tracking(Track),
    Momentum(Flight),
    Rebound(Rebound),
    Smooth(Smooth),
    Held(Held),
    Spring(Spring),
}

/// The scroller whose geometry `step` needs for `input`: the one the input names, else the one
/// the engine is on.
pub fn target(engine: &Engine, input: &ScrollIn) -> Option<Latch> {
    match input {
        ScrollIn::Began { latched, .. } => Some(latched.latch),
        ScrollIn::Step { latch, .. }
        | ScrollIn::Jump { latch, .. }
        | ScrollIn::Hold { latch, .. } => Some(*latch),
        _ => latch_of(engine),
    }
}

/// The scroller the engine is on.
pub fn latch_of(engine: &Engine) -> Option<Latch> {
    match engine {
        Engine::Idle => None,
        Engine::Pending(p) => p.last,
        Engine::Tracking(t) => Some(t.latch),
        Engine::Momentum(f) => Some(f.latch),
        Engine::Rebound(r) => Some(r.latch),
        Engine::Smooth(s) => Some(s.latch),
        Engine::Held(h) => Some(h.latch),
        Engine::Spring(s) => Some(s.latch),
    }
}

/// Whether the engine animates (and so needs a frame per callback).
pub fn motion(engine: &Engine) -> Motion {
    match engine {
        Engine::Idle | Engine::Pending(_) | Engine::Tracking(_) => Motion::Still,
        Engine::Momentum(_)
        | Engine::Rebound(_)
        | Engine::Smooth(_)
        | Engine::Held(_)
        | Engine::Spring(_) => Motion::Animating,
    }
}

/// Advance the engine by `input` at `now`. `geom` is [`target`]'s geometry, `None` when it is
/// gone (the node was removed): the engine then stops.
pub fn step(
    engine: Engine,
    input: ScrollIn,
    now: HostInstant,
    geom: Option<Geom>,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    let Some(g) = geom else {
        return match input {
            ScrollIn::MayBegin => (Engine::Pending(Pending::default()), Vec::new()),
            _ => (Engine::Idle, Vec::new()),
        };
    };
    match input {
        ScrollIn::MayBegin => (may_begin(engine, now, physics), Vec::new()),
        ScrollIn::Began {
            latched,
            dir,
            kinetic,
        } => began(engine, latched, dir, kinetic, now, &g, physics),
        ScrollIn::Changed { delta, at } => changed(engine, delta.0, at, &g, physics),
        ScrollIn::Ended { at } => lifted(engine, Some(at), now, &g, physics),
        ScrollIn::Cancelled => lifted(engine, None, now, &g, physics),
        ScrollIn::Step { latch, by } => {
            let (from, base) = match &engine {
                Engine::Smooth(s) if s.latch == latch => (g.offset.0, s.to),
                _ => (g.clamp(g.offset.0), g.clamp(g.offset.0)),
            };
            animate::smooth_to(latch, from, g.clamp(base + by.0), now)
        }
        ScrollIn::Jump { latch, to, animate } => match animate {
            Animate::Smooth => animate::smooth_to(latch, g.offset.0, g.clamp(to.0), now),
            Animate::Instant => (Engine::Idle, vec![set(latch, g.clamp(to.0))]),
        },
        ScrollIn::Hold { latch, step } => match engine {
            Engine::Held(h) if h.latch == latch => (Engine::Held(h), Vec::new()),
            _ => (
                Engine::Held(Held {
                    latch,
                    step: step.0,
                    x0: g.clamp(g.offset.0),
                    t0: now,
                }),
                Vec::new(),
            ),
        },
        ScrollIn::Release => match engine {
            Engine::Held(h) => (animate::release(h, now, &g), Vec::new()),
            other => (other, Vec::new()),
        },
        ScrollIn::Frame => animate::frame(engine, now, &g, physics),
    }
}

/// `SetOffset` for `latch`.
pub(crate) fn set(latch: Latch, raw: f64) -> ScrollOut {
    ScrollOut::SetOffset {
        latch,
        raw: Px(raw),
    }
}

/// `Notify` for `latch`.
pub(crate) fn notify(latch: Latch, phase: NotifyPhase) -> ScrollOut {
    ScrollOut::Notify { latch, phase }
}

/// Touch to stop: every animation halts where it is; a glide's speed is kept for a boost.
fn may_begin(engine: Engine, now: HostInstant, physics: &Physics) -> Engine {
    match engine {
        Engine::Tracking(t) => Engine::Tracking(t),
        Engine::Momentum(f) => Engine::Pending(Pending {
            last: Some(f.latch),
            carry: animate::flight_velocity(&f, now, physics),
        }),
        other => Engine::Pending(Pending {
            last: latch_of(&other),
            carry: 0.0,
        }),
    }
}

fn began(
    engine: Engine,
    latched: Latched,
    dir: Dir,
    kinetic: Kinetic,
    now: HostInstant,
    g: &Geom,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    let carried = match &engine {
        Engine::Momentum(f) => Some((f.latch, animate::flight_velocity(f, now, physics))),
        Engine::Pending(p) => p.last.map(|latch| (latch, p.carry)),
        _ => None,
    };
    let boost = carried
        .filter(|(latch, v)| {
            *latch == latched.latch && v.abs() >= physics.boost_min && v * dir.sign() > 0.0
        })
        .map_or(0.0, |(_, v)| v);
    let track = Track {
        latch: latched.latch,
        kinetic,
        began: latched.began,
        pos: animate::pos_for(g.offset.0, g, latched.began, physics),
        samples: Samples::default(),
        boost,
    };
    (
        Engine::Tracking(track),
        vec![notify(latched.latch, NotifyPhase::Began)],
    )
}

fn changed(
    engine: Engine,
    delta: f64,
    at: SampleTime,
    g: &Geom,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    let Engine::Tracking(mut track) = engine else {
        return (engine, Vec::new());
    };
    track.pos = match animate::stretches(&track, g, physics) {
        true => track.pos + delta,
        false => g.clamp(track.pos + delta),
    };
    track.samples.push(at, track.pos);
    let raw = animate::raw_for(&track, g, physics);
    let out = if (raw - g.offset.0).abs() > f64::EPSILON {
        vec![set(track.latch, raw)]
    } else {
        Vec::new()
    };
    (Engine::Tracking(track), out)
}

/// The fingers lifted (`Some(at)`) or the gesture was cancelled (`None`).
fn lifted(
    engine: Engine,
    at: Option<SampleTime>,
    now: HostInstant,
    g: &Geom,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    match engine {
        Engine::Tracking(track) => animate::release_track(track, at, now, g, physics),
        Engine::Pending(Pending {
            last: Some(latch), ..
        }) => (animate::settle_stretch(latch, now, g), Vec::new()),
        Engine::Pending(_) => (Engine::Idle, Vec::new()),
        other => (other, Vec::new()),
    }
}
