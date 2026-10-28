//! Pointer-driven scrolling: a `wl_pointer` axis frame (wheel detents, a touchpad's phased pan)
//! and palmrest's gestures, latched from the scrollers under the pointer.

use blitz_dom::BaseDocument;

use super::{Moved, PalmGesture, PalmScroll, SurfaceScroll};
use crate::input::pointer::PointerPos;
use crate::scroll::source::{self, AxisFrame};
use blitz_kit::scroll::engine::{Kinetic, ScrollIn};
use blitz_kit::scroll::geom::{Dir, Px, ScrollAxis, ViewPoint};
use blitz_kit::scroll::pad::natural;
use blitz_kit::scroll::time::Elapsed;

/// `pos` as the engine's view point.
pub(super) fn view_point(pos: PointerPos) -> ViewPoint {
    ViewPoint { x: pos.x, y: pos.y }
}

impl SurfaceScroll {
    /// A `wl_pointer` axis frame at surface-local `pos`.
    pub(crate) fn axis(
        &mut self,
        doc: &mut BaseDocument,
        frame: &AxisFrame,
        pos: PointerPos,
        now: Elapsed,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let moves = self
            .scroll
            .pointer(doc, source::classify(frame), view_point(pos), env.kit);
        self.applied(moves, now)
    }

    /// A palmrest scroll event while the pointer is at `pos` on this surface.
    pub(crate) fn palmrest(
        &mut self,
        doc: &mut BaseDocument,
        event: PalmScroll,
        pos: PointerPos,
        now: Elapsed,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        self.driven_by_palmrest = true;
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
                let setting = env.kit.settings.natural;
                let axis = match self.palm {
                    PalmGesture::Waiting(axis) => axis,
                    PalmGesture::Latched => match self.scroll.latch() {
                        Some(latch) => latch.axis,
                        None => return Moved::Still,
                    },
                    PalmGesture::Idle => return Moved::Still,
                };
                let finger = match axis {
                    ScrollAxis::X => dx,
                    ScrollAxis::Y => dy,
                };
                let delta = natural(finger, setting);
                let began = match (self.palm, Dir::of(delta)) {
                    (PalmGesture::Waiting(_), Some(dir)) => {
                        self.palm = PalmGesture::Latched;
                        let at = view_point(pos);
                        self.scroll
                            .begin(doc, axis, dir, Kinetic::Momentum, at, env.kit)
                    }
                    (PalmGesture::Waiting(_), None) => return Moved::Still,
                    _ => blitz_kit::scroll::driver::Moves::none(),
                };
                let changed = ScrollIn::Changed {
                    delta: Px(delta),
                    at,
                };
                let moves = began.then(self.scroll.run(doc, changed, env.kit));
                self.applied(moves, now)
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
