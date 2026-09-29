//! One surface's scrolling: the engine, the thumb and the source trackers, applied to the
//! surface's Blitz document through `dom::scroll`. The host calls these from its input handlers
//! and its render; everything they decide is the pure modules'.

mod key_input;
mod pointer_input;
mod thumb;

use blitz_dom::BaseDocument;

use crate::dom::scroll as dom;
use crate::frame::Animation;
use crate::scroll::engine::{self, Engine, Motion, ScrollIn, ScrollOut};
use crate::scroll::geom::{Latch, ScrollAxis};
use crate::scroll::indicator::{Indicator, Wake};
use crate::scroll::latch::WheelBurst;
use crate::scroll::source::Pad;
use crate::scroll::suppress::Driven;
use crate::scroll::tuning::{Env, Tuning};
use crate::scroll::velocity::SampleTime;
use crate::scroll::watch::ScrollBoard;
use crate::tick::HostInstant;

/// What a step did to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Moved {
    Moved,
    Still,
}

impl Moved {
    fn or(self, other: Moved) -> Moved {
        match (self, other) {
            (Moved::Still, Moved::Still) => Moved::Still,
            _ => Moved::Moved,
        }
    }
}

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
    engine: Engine,
    indicator: Indicator,
    pad: Pad,
    burst: WheelBurst,
    palm: PalmGesture,
    driver: Driver,
    /// The scroller the last gesture latched: the keyboard's target while under the pointer.
    last_gesture: Option<Latch>,
    board: ScrollBoard,
    tuning: Tuning,
}

impl SurfaceScroll {
    /// A surface's scrolling, publishing to `board` (shared with its `SurfaceHandle`).
    pub(crate) fn new(board: ScrollBoard, tuning: Tuning) -> SurfaceScroll {
        SurfaceScroll {
            engine: Engine::Idle,
            indicator: Indicator::default(),
            pad: Pad::Idle,
            burst: WheelBurst::default(),
            palm: PalmGesture::Idle,
            driver: Driver::Other,
            last_gesture: None,
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
        &self.engine
    }

    /// Step the engine with `input` and write what it says.
    fn run(&mut self, doc: &mut BaseDocument, input: ScrollIn, env: Env<'_>) -> Moved {
        let target = engine::target(&self.engine, &input);
        let geom = target.and_then(|latch| dom::geom(doc, latch.scroller, latch.axis));
        let current = std::mem::take(&mut self.engine);
        let (next, outs) = engine::step(current, input, env.now, geom, env.physics);
        self.engine = next;
        outs.into_iter().fold(Moved::Still, |moved, out| match out {
            ScrollOut::SetOffset { latch, raw } => {
                dom::write(doc, latch.scroller, latch.axis, raw.0);
                self.indicator = self.indicator.changed(latch, env.now);
                Moved::Moved
            }
            ScrollOut::Notify { .. } => moved,
        })
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
        let fading =
            self.indicator
                .wake(&env.config.scrollbar, env.config.scroll.scrollbars, env.now);
        match (engine::motion(&self.engine), fading) {
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
            .wake(&env.config.scrollbar, env.config.scroll.scrollbars, env.now)
    }

    /// Whether palmrest drove what the engine is doing now (for the double-scroll rule).
    pub(crate) fn driven(&self) -> Driven {
        let busy = !matches!(self.engine, Engine::Idle);
        match (self.driver, busy) {
            (Driver::Palmrest, true) => Driven::Running,
            _ => Driven::Settled,
        }
    }
}
