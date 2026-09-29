//! The gesture phases: touch, begin, change and lift.

use super::types::{Kinetic, NotifyPhase, Pending, Physics, Track};
use super::{Engine, ScrollOut, latch_of, notify, set};
use crate::scroll::animate;
use crate::scroll::geom::{Dir, Geom};
use crate::scroll::latch::Latched;
use crate::scroll::velocity::{SampleTime, Samples};
use crate::tick::HostInstant;

/// Touch to stop: every animation halts where it is; a glide's speed is kept for a boost.
pub(super) fn may_begin(engine: Engine, now: HostInstant, physics: &Physics) -> Engine {
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

pub(super) fn began(
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

pub(super) fn changed(
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
pub(super) fn lifted(
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
