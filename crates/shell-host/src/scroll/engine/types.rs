//! What the engine takes, gives and remembers: its inputs, outputs and one struct per phase.

use crate::scroll::config::{Momentum, ScrollConfig};
use crate::scroll::geom::{Dir, Latch, Px};
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
