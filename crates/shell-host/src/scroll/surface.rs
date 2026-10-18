//! One surface's scrolling: the engine's driver (`blitz_kit::scroll::driver`), the thumb and the
//! palmrest tracker, applied to the surface's Blitz document. The host calls these from its
//! input handlers and its render; everything they decide is the pure modules'.

mod key_input;
mod pointer_input;
mod thumb;

use blitz_dom::BaseDocument;

use crate::frame::Animation;
use crate::scroll::indicator::{Indicator, Wake};
use crate::scroll::suppress::Driven;
use crate::scroll::tuning::{Env, Tuning};
use crate::scroll::watch::ScrollBoard;
use crate::tick::HostInstant;
pub(crate) use blitz_kit::scroll::driver::Moved;
use blitz_kit::scroll::driver::{Claim, Moves, ScrollDriver};
use blitz_kit::scroll::engine::{Engine, Motion, ScrollIn};
use blitz_kit::scroll::geom::ScrollAxis;
use blitz_kit::scroll::velocity::SampleTime;

/// Whether a key reached the engine or belongs to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyUse {
    Scrolled(Moved),
    Passed,
}

/// Who started what the engine is doing, for the double-scroll rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Driver {
    Palmrest,
    Other,
}

/// A palmrest gesture whose axis is known but whose direction is not yet (palmrest drops the
/// first delta at Began, design/12 §12.3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PalmGesture {
    Idle,
    Waiting(ScrollAxis),
    Latched,
}

/// A palmrest scroll event, already on the host's terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PalmScroll {
    MayBegin,
    Began {
        axis: ScrollAxis,
    },
    /// Finger-direction px on each axis at `at` (palmrest's `t_ns`).
    Changed {
        dx: f64,
        dy: f64,
        at: SampleTime,
    },
    Ended {
        at: SampleTime,
    },
    Cancelled,
}

/// One surface's scroll state.
#[derive(Debug, Clone)]
pub(crate) struct SurfaceScroll {
    scroll: ScrollDriver,
    indicator: Indicator,
    palm: PalmGesture,
    driver: Driver,
    board: ScrollBoard,
    tuning: Tuning,
}

impl SurfaceScroll {
    /// A surface's scrolling, publishing to `board` (shared with its `SurfaceHandle`).
    pub(crate) fn new(board: ScrollBoard, tuning: Tuning) -> SurfaceScroll {
        SurfaceScroll {
            scroll: ScrollDriver::default(),
            indicator: Indicator::default(),
            palm: PalmGesture::Idle,
            driver: Driver::Other,
            board,
            tuning,
        }
    }

    /// Use `tuning` from now on (a settings change; nothing animates from it).
    pub(crate) fn set_tuning(&mut self, tuning: Tuning) {
        self.tuning = tuning;
    }

    /// The watched scrollers this surface publishes to.
    #[cfg(test)]
    pub(crate) fn board(&self) -> ScrollBoard {
        self.board.clone()
    }

    /// The engine, for tests.
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &Engine {
        self.scroll.engine()
    }

    /// Step the engine with `input` and write what it says.
    fn run(&mut self, doc: &mut BaseDocument, input: ScrollIn, env: Env<'_>) -> Moved {
        let moves = self.scroll.run(doc, input, env.kit);
        self.applied(moves, env.now)
    }

    /// Show the thumb for what `moves` wrote, and remember who drove the engine when it took the
    /// input.
    fn applied(&mut self, moves: Moves, now: HostInstant) -> Moved {
        for latch in moves.written() {
            self.indicator = self.indicator.changed(*latch, now);
        }
        if moves.claim() == Claim::Took {
            self.driver = Driver::Other;
        }
        moves.moved()
    }

    /// A frame is rendered at `env.now`: advance the animation. Called before the resolve, so
    /// the layout (and Blitz's hoisting of positioned descendants) sees this frame's offsets.
    pub(crate) fn frame(&mut self, doc: &mut BaseDocument, now: HostInstant) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        self.run(doc, ScrollIn::Frame, env)
    }

    /// Whether the surface needs a frame per callback for scrolling.
    pub(crate) fn motion(&self, now: HostInstant) -> Motion {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let fading = self
            .indicator
            .wake(env.scrollbar, env.kit.settings.scrollbars, env.now);
        match (self.scroll.motion(), fading) {
            (Motion::Animating, _) | (_, Wake::Frames) => Motion::Animating,
            _ => Motion::Still,
        }
    }

    /// [`SurfaceScroll::motion`] as the frame machine reads it.
    pub(crate) fn animation(&self, now: HostInstant) -> Animation {
        match self.motion(now) {
            Motion::Animating => Animation::Running,
            Motion::Still => Animation::Stopped,
        }
    }

    /// When the thumb next needs a frame while nothing animates (the end of its hold).
    pub(crate) fn wake(&self, now: HostInstant) -> Wake {
        let tuning = self.tuning;
        let env = tuning.at(now);
        self.indicator
            .wake(env.scrollbar, env.kit.settings.scrollbars, env.now)
    }

    /// Whether palmrest drove what the engine is doing now (for the double-scroll rule).
    pub(crate) fn driven(&self) -> Driven {
        let busy = !matches!(self.scroll.engine(), Engine::Idle);
        match (self.driver, busy) {
            (Driver::Palmrest, true) => Driven::Running,
            _ => Driven::Settled,
        }
    }
}
