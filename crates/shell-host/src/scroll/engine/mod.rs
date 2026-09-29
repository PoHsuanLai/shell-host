//! The scroll state machine (design/11 §11.4): pure. The host reads the latched scroller's
//! geometry from the laid-out document, calls [`step`] with an input and the time, and performs
//! the returned effects (writes the raw offsets, notifies). Every animation is a closed form of
//! the time since its phase began (`scroll::animate`), so `now` can be advanced by hand.

mod phases;
mod types;

pub use types::ScrollAnimate;
pub(crate) use types::{
    Flight, Held, Kinetic, Motion, NotifyPhase, Pending, Physics, Rebound, ScrollIn, ScrollOut,
    Smooth, Spring, Track,
};

use crate::scroll::animate;
use crate::scroll::geom::{Geom, Latch, Px};
use crate::tick::HostInstant;

use phases::{began, changed, lifted, may_begin};

/// The engine (design/11 §11.4).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum Engine {
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
pub(crate) fn target(engine: &Engine, input: &ScrollIn) -> Option<Latch> {
    match input {
        ScrollIn::Began { latched, .. } => Some(latched.latch),
        ScrollIn::Step { latch, .. }
        | ScrollIn::Jump { latch, .. }
        | ScrollIn::Hold { latch, .. } => Some(*latch),
        _ => latch_of(engine),
    }
}

/// The scroller the engine is on.
pub(crate) fn latch_of(engine: &Engine) -> Option<Latch> {
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
pub(crate) fn motion(engine: &Engine) -> Motion {
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
pub(crate) fn step(
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
            ScrollAnimate::Smooth => animate::smooth_to(latch, g.offset.0, g.clamp(to.0), now),
            ScrollAnimate::Instant => (Engine::Idle, vec![set(latch, g.clamp(to.0))]),
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
