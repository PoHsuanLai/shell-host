//! The engine's continuous phases, each a closed form of the time since it began: the glide,
//! the rebound, the smooth step, the held arrow and its spring (design/11 §11.5). Called from
//! `scroll::engine::step`; pure.

use crate::scroll::config::Momentum;
use crate::scroll::engine::{
    Engine, Flight, Held, Kinetic, NotifyPhase, Physics, Rebound, ScrollOut, Smooth, Spring, Track,
    notify, set,
};
use crate::scroll::geom::{Dir, Elastic, Geom, Latch};
use crate::scroll::keys;
use crate::scroll::rubber::{AtEdge, Settled};
use crate::scroll::smooth;
use crate::scroll::velocity::SampleTime;
use crate::tick::HostInstant;

/// A spring is settled within this many px of its target ...
const SPRING_DONE_PX: f64 = 0.5;
/// ... moving slower than this, px/s (design/11 §11.4).
const SPRING_DONE_PX_S: f64 = 5.0;

/// Seconds from `t0` to `now`.
fn since(t0: HostInstant, now: HostInstant) -> f64 {
    now.0.as_secs_f64() - t0.0.as_secs_f64()
}

/// Whether a gesture on `g` may stretch past the edges.
pub(crate) fn stretches(track: &Track, g: &Geom, physics: &Physics) -> bool {
    track.kinetic == Kinetic::Momentum && g.elastic == Elastic::Elastic && physics.band.enabled()
}

/// The raw offset a tracking gesture shows: its position, or the edge plus the stretch.
pub(crate) fn raw_for(track: &Track, g: &Geom, physics: &Physics) -> f64 {
    let max = g.edge(Dir::Pos);
    let d = g.viewport.0;
    if !stretches(track, g, physics) {
        return g.clamp(track.pos);
    }
    if track.pos < 0.0 {
        -physics.band.stretch(-track.pos, d, track.began)
    } else if track.pos > max {
        max + physics.band.stretch(track.pos - max, d, track.began)
    } else {
        track.pos
    }
}

/// The unclamped position a gesture starting on raw offset `raw` begins from (a stretched
/// scroller caught by the fingers keeps its stretch).
pub(crate) fn pos_for(raw: f64, g: &Geom, began: AtEdge, physics: &Physics) -> f64 {
    let max = g.edge(Dir::Pos);
    let d = g.viewport.0;
    if raw < 0.0 {
        -physics.band.overscroll(-raw, d, began)
    } else if raw > max {
        max + physics.band.overscroll(raw - max, d, began)
    } else {
        raw
    }
}

/// The side and size of `raw`'s stretch past `g`'s range, if any.
fn stretch_of(raw: f64, g: &Geom) -> Option<(Dir, f64)> {
    let max = g.edge(Dir::Pos);
    if raw < -f64::EPSILON {
        Some((Dir::Neg, -raw))
    } else if raw > max + f64::EPSILON {
        Some((Dir::Pos, raw - max))
    } else {
        None
    }
}

/// A glide's signed speed at `now`.
pub(crate) fn flight_velocity(f: &Flight, now: HostInstant, physics: &Physics) -> f64 {
    f.v0.signum() * physics.glide.speed(f.v0.abs(), since(f.t0, now))
}

/// The fingers lifted from a tracking gesture (`at`), or it was cancelled (`None`).
pub(crate) fn release_track(
    track: Track,
    at: Option<SampleTime>,
    now: HostInstant,
    g: &Geom,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    let released = match (at, track.kinetic, physics.momentum) {
        (Some(at), Kinetic::Momentum, Momentum::On) => {
            track.samples.velocity(at, physics.release) + track.boost
        }
        _ => 0.0,
    };
    let v0 = released.clamp(-physics.cap, physics.cap);
    let latch = track.latch;
    let raw = raw_for(&track, g, physics);
    if let Some((edge, s0)) = stretch_of(raw, g) {
        let rebound = Rebound {
            latch,
            edge,
            s0,
            v0: v0 * edge.sign(),
            t0: now,
        };
        return (
            Engine::Rebound(rebound),
            vec![notify(latch, NotifyPhase::Ended)],
        );
    }
    if v0.abs() >= physics.start && v0.abs() > 0.0 {
        let flight = Flight {
            latch,
            v0,
            x0: g.clamp(track.pos),
            t0: now,
        };
        return (
            Engine::Momentum(flight),
            vec![notify(latch, NotifyPhase::MomentumBegan)],
        );
    }
    (Engine::Idle, vec![notify(latch, NotifyPhase::Ended)])
}

/// Fingers lifted without a gesture: a scroller they caught mid-stretch springs back.
pub(crate) fn settle_stretch(latch: Latch, now: HostInstant, g: &Geom) -> Engine {
    match stretch_of(g.offset.0, g) {
        Some((edge, s0)) => Engine::Rebound(Rebound {
            latch,
            edge,
            s0,
            v0: 0.0,
            t0: now,
        }),
        None => Engine::Idle,
    }
}

/// Animate `latch` from `from` to `to` by the smooth rule; a zero-length step does nothing.
pub(crate) fn smooth_to(
    latch: Latch,
    from: f64,
    to: f64,
    now: HostInstant,
) -> (Engine, Vec<ScrollOut>) {
    let dur = smooth::duration(to - from);
    if dur <= 0.0 {
        return (Engine::Idle, Vec::new());
    }
    let smooth = Smooth {
        latch,
        from,
        to,
        t0: now,
        dur,
    };
    (Engine::Smooth(smooth), Vec::new())
}

/// The held arrow was released at `now`: spring toward 100 ms ahead of it.
pub(crate) fn release(held: Held, now: HostInstant, g: &Geom) -> Engine {
    let v = keys::held_speed(held.step, since(held.t0, now));
    let x = g.offset.0;
    Engine::Spring(Spring {
        latch: held.latch,
        x0: x,
        v0: v,
        target: g.clamp(keys::release_target(x, v)),
        t0: now,
    })
}

/// A frame at `now`: advance whatever animates.
pub(crate) fn frame(
    engine: Engine,
    now: HostInstant,
    g: &Geom,
    physics: &Physics,
) -> (Engine, Vec<ScrollOut>) {
    match engine {
        Engine::Momentum(f) => glide(f, now, g, physics),
        Engine::Rebound(r) => rebound(r, now, g, physics),
        Engine::Smooth(s) => smooth_frame(s, now),
        Engine::Held(h) => {
            let x = g.clamp(h.x0 + keys::held_travel(h.step, since(h.t0, now)));
            (Engine::Held(h), vec![set(h.latch, x)])
        }
        Engine::Spring(s) => spring_frame(s, now, g),
        other => (other, Vec::new()),
    }
}

fn glide(f: Flight, now: HostInstant, g: &Geom, physics: &Physics) -> (Engine, Vec<ScrollOut>) {
    let t = since(f.t0, now);
    let speed = f.v0.abs();
    let dir = if f.v0 > 0.0 { Dir::Pos } else { Dir::Neg };
    let room = (g.edge(dir) - f.x0) * dir.sign();
    let travel = physics.glide.travel(speed, t);
    let elastic = g.elastic == Elastic::Elastic && physics.band.enabled();
    if travel >= room {
        let ended = || {
            (
                Engine::Idle,
                vec![
                    set(f.latch, g.edge(dir)),
                    notify(f.latch, NotifyPhase::MomentumEnded),
                ],
            )
        };
        if !elastic {
            return ended();
        }
        let Some((t_hit, v_hit)) = physics.glide.reach(speed, room.max(0.0)) else {
            return ended();
        };
        let impact = Rebound {
            latch: f.latch,
            edge: dir,
            s0: 0.0,
            v0: v_hit,
            t0: HostInstant(f.t0.0 + std::time::Duration::from_secs_f64(t_hit)),
        };
        return rebound(impact, now, g, physics);
    }
    if t >= physics.glide.duration(speed) {
        return (
            Engine::Idle,
            vec![
                set(f.latch, f.x0 + dir.sign() * physics.glide.total(speed)),
                notify(f.latch, NotifyPhase::MomentumEnded),
            ],
        );
    }
    (
        Engine::Momentum(f),
        vec![set(f.latch, f.x0 + dir.sign() * travel)],
    )
}

fn rebound(r: Rebound, now: HostInstant, g: &Geom, physics: &Physics) -> (Engine, Vec<ScrollOut>) {
    let t = since(r.t0, now).max(0.0);
    let s = physics.snap.stretch(r.s0, r.v0, t);
    let edge = g.edge(r.edge);
    match physics.snap.settled(s, t) {
        Settled::Settled => (
            Engine::Idle,
            vec![
                set(r.latch, edge),
                notify(r.latch, NotifyPhase::MomentumEnded),
            ],
        ),
        Settled::Moving => (
            Engine::Rebound(r),
            vec![set(r.latch, edge + r.edge.sign() * s)],
        ),
    }
}

fn smooth_frame(s: Smooth, now: HostInstant) -> (Engine, Vec<ScrollOut>) {
    let p = since(s.t0, now) / s.dur;
    if p >= 1.0 {
        return (Engine::Idle, vec![set(s.latch, s.to)]);
    }
    let x = s.from + (s.to - s.from) * smooth::e_out(p);
    (Engine::Smooth(s), vec![set(s.latch, x)])
}

fn spring_frame(s: Spring, now: HostInstant, g: &Geom) -> (Engine, Vec<ScrollOut>) {
    let (e, v) = keys::spring(s.x0 - s.target, s.v0, since(s.t0, now));
    if e.abs() < SPRING_DONE_PX && v.abs() < SPRING_DONE_PX_S {
        return (Engine::Idle, vec![set(s.latch, s.target)]);
    }
    (Engine::Spring(s), vec![set(s.latch, g.clamp(s.target + e))])
}
