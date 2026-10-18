//! Scroll input on the host's terms: `wl_pointer` axis frames, scroll keys, palmrest's events
//! and programmatic scrolls, routed to the right surface's engine (`scroll::surface`), with the
//! double-scroll rule and the `data-wheel="capture"` exception applied first (design/11
//! §11.3.1, §11.3.14). Every path ends by invalidating the surface when an offset moved or an
//! animation started, so the frame machine renders.
//!
//! See FINDINGS F63, F64.

use std::time::Instant;

use blitz_traits::events::BlitzKeyEvent;
use smithay_client_toolkit::seat::pointer::AxisScroll;
use wayland_client::protocol::wl_pointer;

use crate::event::HostEvent;
use crate::frame::Animation;
use crate::geometry::Point;
use crate::id::SurfaceId;
use crate::input::focus::PointerFocus;
use crate::input::key::Repeat;
use crate::input::palmrest::route::{Route, route};
use crate::input::pointer::{self, PointerPos};
use crate::region::Px;
use crate::scroll::source::{AxisSource, Stop};
use crate::scroll::suppress::{AxisKind, Driven, Verdict};
use crate::scroll::surface::{Moved, PalmScroll};
use crate::scroll::tuning::Tuning;
use crate::scroll::wheel::{WheelContext, wheel_event};
use crate::state::HostState;
use blitz_kit::scroll::cmd::ScrollCmd;
use blitz_kit::scroll::doc::{self as dom, WheelRoute};
use blitz_kit::scroll::driver::Claim;
use blitz_kit::scroll::geom::ViewPoint;
use blitz_kit::scroll::keys::scroll_key;
use blitz_kit::scroll::time::Elapsed;
use palmrest_protocol::ToHost;

impl HostState {
    /// Now, on the host clock.
    pub(crate) fn now(&self) -> Elapsed {
        self.clock.instant_at(Instant::now())
    }

    /// Render surface `id` if scrolling moved it or started an animation on it.
    fn after_scroll(&mut self, id: SurfaceId, moved: Moved, now: Elapsed) {
        let animating = self
            .surfaces
            .get(&id)
            .is_some_and(|s| s.scroll.animation(now) == Animation::Running);
        if moved == Moved::Moved || animating {
            self.invalidate(id);
        }
    }

    /// Whether any palmrest-driven animation still runs (the double-scroll rule).
    fn palmrest_driven(&self) -> Driven {
        match self
            .surfaces
            .values()
            .any(|s| s.scroll.driven() == Driven::Running)
        {
            true => Driven::Running,
            false => Driven::Settled,
        }
    }

    /// A `wl_pointer` axis frame on surface `id` at document point `at`.
    pub(crate) fn scroll_axis(
        &mut self,
        id: SurfaceId,
        (horizontal, vertical): (AxisScroll, AxisScroll),
        source: Option<wl_pointer::AxisSource>,
        at: PointerPos,
    ) {
        let now = self.now();
        let frame = pointer::axis_frame(horizontal, vertical, source, self.input.modifiers);
        let kind = match frame.source {
            AxisSource::Wheel | AxisSource::Finger | AxisSource::Unknown => AxisKind::Suppressible,
            AxisSource::Continuous | AxisSource::WheelTilt => AxisKind::Other,
        };
        let grace = self.scroll.settings().double_scroll_grace_ms.duration();
        let verdict = self
            .suppress
            .verdict(kind, self.palmrest_driven(), grace, now);
        if frame.horizontal.stop == Stop::Stopped || frame.vertical.stop == Stop::Stopped {
            self.suppress = self.suppress.axis_stop();
        }
        if verdict == Verdict::Drop {
            return;
        }
        let held = self.input.held_modifiers;
        let detent_px = self.scroll.settings().wheel_detent_px.get();
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let context = WheelContext {
            at: Point {
                x: at.x.round() as Px,
                y: at.y.round() as Px,
            },
            held,
            detent_px,
        };
        let (run, event) = wheel_event(surface.wheel_run, &frame, context);
        surface.wheel_run = run;
        if let Some(event) = event {
            surface.wheels.publish(event);
        }
        let route = dom::wheel_route(&surface.document.base(), ViewPoint { x: at.x, y: at.y });
        if route == WheelRoute::Capture {
            if let Some(delta) = pointer::wheel_delta(horizontal, vertical) {
                let event =
                    pointer::wheel_event(delta, at, self.input.buttons, self.input.modifiers);
                self.deliver(id, [event]);
            }
            return;
        }
        let moved = {
            let mut doc = surface.document.base_mut();
            surface.scroll.axis(&mut doc, &frame, at, now)
        };
        self.after_scroll(id, moved, now);
    }

    /// The pointer moved to document point `at` on surface `id`: the thumb's hover.
    pub(crate) fn scroll_pointer(&mut self, id: SurfaceId, at: PointerPos) {
        let now = self.now();
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let moved = surface.scroll.pointer(&surface.document.base(), at, now);
        self.after_scroll(id, moved, now);
    }

    /// A key press or repeat on focused surface `id`: `Claim::Took` when the engine took it
    /// (the document never sees it), `Left` otherwise.
    pub(crate) fn scroll_key_down(
        &mut self,
        id: SurfaceId,
        event: &BlitzKeyEvent,
        raw: u32,
    ) -> Claim {
        let Some(key) = scroll_key(&event.key, event.modifiers) else {
            return Claim::Left;
        };
        let repeat = match event.is_auto_repeating {
            true => Repeat::Repeated,
            false => Repeat::First,
        };
        if repeat == Repeat::Repeated && self.scroll_key != Some(raw) {
            return Claim::Left;
        }
        let now = self.now();
        let pos = self.pointer_on(id);
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return Claim::Left;
        };
        let (claim, moved) = {
            let mut doc = surface.document.base_mut();
            surface.scroll.key(&mut doc, key, repeat, pos, now)
        };
        if claim == Claim::Took {
            self.scroll_key = Some(raw);
            self.after_scroll(id, moved, now);
        }
        claim
    }

    /// A key release on focused surface `id`: `Claim::Took` when it ends a key the engine took
    /// (a held arrow's spring starts).
    pub(crate) fn scroll_key_up(&mut self, id: SurfaceId, raw: u32) -> Claim {
        if self.scroll_key != Some(raw) {
            return Claim::Left;
        }
        self.scroll_key = None;
        let now = self.now();
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return Claim::Left;
        };
        let moved = {
            let mut doc = surface.document.base_mut();
            surface.scroll.key_up(&mut doc, now)
        };
        self.after_scroll(id, moved, now);
        Claim::Took
    }

    /// Where the pointer is on surface `id` in document coordinates, if it is there.
    fn pointer_on(&self, id: SurfaceId) -> Option<PointerPos> {
        match self.input.pointer_focus {
            PointerFocus::On(entered) if entered.surface == id => {
                Some(self.document_pos(id, self.input.position))
            }
            _ => None,
        }
    }

    /// A programmatic scroll of surface `id`.
    pub(crate) fn scroll_command(&mut self, id: SurfaceId, cmd: &ScrollCmd) {
        let now = self.now();
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let moved = {
            let mut doc = surface.document.base_mut();
            surface.scroll.command(&mut doc, cmd, now)
        };
        self.after_scroll(id, moved, now);
    }

    /// Use `tuning` on every surface from now on.
    pub(crate) fn set_scroll_tuning(&mut self, tuning: Tuning) {
        self.scroll = tuning;
        for surface in self.surfaces.values_mut() {
            surface.scroll.set_tuning(tuning);
        }
    }

    /// One message from palmrest (design/12 §12.3.11, `input::palmrest::route`).
    pub(crate) fn palmrest_message(&mut self, message: ToHost) {
        if let ToHost::Run { action } = message {
            // The shell carries it out; an action over the bounds is not one the daemon sends.
            if action.fits() {
                self.events.push_back(HostEvent::Palmrest(action));
            }
            return;
        }
        let now = self.now();
        let Route { fingers, scroll } = route(&message);
        if let Some(fingers) = fingers {
            self.suppress = self.suppress.fingers(fingers, now);
        }
        if let Some(event) = scroll {
            self.palm_scroll(event, now);
        }
    }

    /// Feed a palmrest scroll event to the surface under the pointer, if it is ours.
    fn palm_scroll(&mut self, event: PalmScroll, now: Elapsed) {
        let PointerFocus::On(entered) = self.input.pointer_focus else {
            return;
        };
        let id = entered.surface;
        let pos = self.document_pos(id, self.input.position);
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let moved = {
            let mut doc = surface.document.base_mut();
            surface.scroll.palmrest(&mut doc, event, pos, now)
        };
        self.after_scroll(id, moved, now);
    }
}
