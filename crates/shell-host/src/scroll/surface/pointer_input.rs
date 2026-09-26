//! Pointer-driven scrolling: a `wl_pointer` axis frame (wheel detents, a touchpad's phased pan)
//! and palmrest's gestures, latched from the scrollers under the pointer.

use blitz_dom::BaseDocument;

use super::{Driver, Moved, PalmGesture, PalmScroll, SurfaceScroll};
use crate::dom::scroll as dom;
use crate::input::pointer::PointerPos;
use crate::scroll::engine::{self, Engine, Kinetic, ScrollIn};
use crate::scroll::geom::{Axis, Dir, Px};
use crate::scroll::latch::{self, Latched};
use crate::scroll::rubber::AtEdge;
use crate::scroll::source::{self, AxisFrame, AxisInput, PadEvent};
use crate::scroll::tuning::Env;
use crate::scroll::velocity::SampleTime;
use crate::tick::HostInstant;

impl SurfaceScroll {
    /// A `wl_pointer` axis frame at surface-local `pos`.
    pub fn axis(
        &mut self,
        doc: &mut BaseDocument,
        frame: &AxisFrame,
        pos: PointerPos,
        now: HostInstant,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        match source::classify(frame) {
            AxisInput::Detents { axis, steps } => self.detents(doc, axis, steps, pos, env),
            input @ (AxisInput::Pan { .. } | AxisInput::Stop) => {
                let (pad, event) = self.pad.feed(input);
                self.pad = pad;
                match event {
                    Some(event) => self.pad_event(doc, event, pos, env),
                    None => Moved::Still,
                }
            }
            AxisInput::Nothing => Moved::Still,
        }
    }

    fn detents(
        &mut self,
        doc: &mut BaseDocument,
        axis: Axis,
        steps: f64,
        pos: PointerPos,
        env: Env<'_>,
    ) -> Moved {
        let px = steps * env.config.scroll.wheel_detent_px.get();
        let Some(dir) = Dir::of(px) else {
            return Moved::Still;
        };
        let window = env.config.scroll.wheel_burst_window_ms.duration();
        let latch = self
            .burst
            .current(axis, env.now, window)
            .or_else(|| latch::latch_rigid(&dom::chain_at(doc, pos.x, pos.y, axis), axis, dir));
        let Some(latch) = latch else {
            return Moved::Still;
        };
        self.burst = self.burst.record(latch, env.now);
        self.driver = Driver::Other;
        self.run(doc, ScrollIn::Step { latch, by: Px(px) }, env)
    }

    fn pad_event(
        &mut self,
        doc: &mut BaseDocument,
        event: PadEvent,
        pos: PointerPos,
        env: Env<'_>,
    ) -> Moved {
        let at = SampleTime(env.now.0);
        match event {
            PadEvent::Began {
                axis,
                dir,
                delta,
                kinetic,
            } => {
                self.driver = Driver::Other;
                self.begin(doc, axis, dir, kinetic, pos, env).or(self.run(
                    doc,
                    ScrollIn::Changed {
                        delta: Px(delta),
                        at,
                    },
                    env,
                ))
            }
            PadEvent::Changed { delta } => self.run(
                doc,
                ScrollIn::Changed {
                    delta: Px(delta),
                    at,
                },
                env,
            ),
            PadEvent::Ended => self.run(doc, ScrollIn::Ended { at }, env),
        }
    }

    /// Latch a gesture on `axis` toward `dir` from the scrollers under `pos`.
    fn begin(
        &mut self,
        doc: &mut BaseDocument,
        axis: Axis,
        dir: Dir,
        kinetic: Kinetic,
        pos: PointerPos,
        env: Env<'_>,
    ) -> Moved {
        let chain = dom::chain_at(doc, pos.x, pos.y, axis);
        let latched = match kinetic {
            Kinetic::Momentum => latch::latch(&chain, axis, dir),
            Kinetic::Plain => latch::latch_rigid(&chain, axis, dir).map(|latch| Latched {
                latch,
                began: AtEdge::Inside,
            }),
        };
        let Some(latched) = latched else {
            // Nothing can move: the gesture is consumed with no effect.
            self.engine = Engine::Idle;
            return Moved::Still;
        };
        self.last_gesture = Some(latched.latch);
        self.run(
            doc,
            ScrollIn::Began {
                latched,
                dir,
                kinetic,
            },
            env,
        )
    }

    /// A palmrest scroll event while the pointer is at `pos` on this surface.
    pub fn palmrest(
        &mut self,
        doc: &mut BaseDocument,
        event: PalmScroll,
        pos: PointerPos,
        now: HostInstant,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        self.driver = Driver::Palmrest;
        match event {
            PalmScroll::MayBegin => {
                self.palm = PalmGesture::Idle;
                self.run(doc, ScrollIn::MayBegin, env)
            }
            PalmScroll::Began { axis } => {
                self.palm = PalmGesture::Waiting(axis);
                Moved::Still
            }
            PalmScroll::Changed { dx, dy, at } => {
                let natural = env.config.scroll.natural;
                let axis = match self.palm {
                    PalmGesture::Waiting(axis) => axis,
                    PalmGesture::Latched => match engine::latch_of(&self.engine) {
                        Some(latch) => latch.axis,
                        None => return Moved::Still,
                    },
                    PalmGesture::Idle => return Moved::Still,
                };
                let finger = match axis {
                    Axis::X => dx,
                    Axis::Y => dy,
                };
                let delta = source::natural(finger, natural);
                let began = match (self.palm, Dir::of(delta)) {
                    (PalmGesture::Waiting(_), Some(dir)) => {
                        self.palm = PalmGesture::Latched;
                        self.begin(doc, axis, dir, Kinetic::Momentum, pos, env)
                    }
                    (PalmGesture::Waiting(_), None) => return Moved::Still,
                    _ => Moved::Still,
                };
                began.or(self.run(
                    doc,
                    ScrollIn::Changed {
                        delta: Px(delta),
                        at,
                    },
                    env,
                ))
            }
            PalmScroll::Ended { at } => {
                self.palm = PalmGesture::Idle;
                self.run(doc, ScrollIn::Ended { at }, env)
            }
            PalmScroll::Cancelled => {
                self.palm = PalmGesture::Idle;
                self.run(doc, ScrollIn::Cancelled, env)
            }
        }
    }
}
