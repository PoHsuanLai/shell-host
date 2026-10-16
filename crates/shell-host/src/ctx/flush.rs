//! The flush: what the host does after every dispatch, always in the same order.
//!
//! 1. **Poll woken documents.** First every surface takes its latest preferred scale, which
//!    wakes the documents that read it (`ctx::rescale`). Only surfaces whose Dioxus tasks woke
//!    are polled; a poll that changed the document invalidates it. `SurfaceDocument::poll` applies the head queue
//!    (`document::Style`, `Link`, `Title`) right after the Dioxus work, so draining the head
//!    queues is part of this phase.
//! 2. **Apply requests.** Everything `SurfaceHandle`s and `HostHandle`s queued, including ops
//!    queued by the polls in phase 1: an op lands after the poll that caused it and before the
//!    paint that shows it, so a region change and its frame share one commit. Text a document
//!    copied through its shell provider claims the selection here too. Last, the session lock
//!    advances on what the handlers and the requests fed it (`ctx::lock`).
//! 3. **Step frames.** Per configured, visible surface: create the presenter on first use,
//!    follow size and scale changes, take the document's redraw request, then render iff the
//!    machine says so (`wl_surface.frame()`, then present, then clear the redraw flag).
//! 4. **Arm timers.** The watchdog, key-repeat and scroll-thumb changes the handlers and phase
//!    3 asked for, the loop sources for pipes a drag target asked us to fill (`ctx::drag`), and
//!    the palmrest socket's source when a connection attempt is due (`ctx::palmrest`).
//! 5. **Flush the connection**, and stop on a protocol error.

use std::task::Context;

use crate::ctx::HostCtx;
use crate::dom::document::Polled;
use crate::error::{HostError, Result};
use crate::frame::FrameInput;
use crate::wake::surface_waker;

/// One phase of a flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    PollWoken,
    ApplyRequests,
    StepFrames,
    ArmTimers,
    FlushConnection,
}

/// The phases, in the order every flush runs them.
pub(crate) const ORDER: [Phase; 5] = [
    Phase::PollWoken,
    Phase::ApplyRequests,
    Phase::StepFrames,
    Phase::ArmTimers,
    Phase::FlushConnection,
];

/// What a flush acts on. The host's context is one implementation; a recording fake in
/// `tests/core.rs` is the other, which is how the order is tested without a compositor.
pub(crate) trait Flush {
    fn poll_woken(&mut self);
    fn apply_requests(&mut self);
    fn step_frames(&mut self);
    fn arm_timers(&mut self);
    fn flush_connection(&mut self) -> Result<()>;
}

/// Run every phase of one flush on `target`, in [`ORDER`].
pub(crate) fn run(target: &mut impl Flush) -> Result<()> {
    for phase in ORDER {
        match phase {
            Phase::PollWoken => target.poll_woken(),
            Phase::ApplyRequests => target.apply_requests(),
            Phase::StepFrames => target.step_frames(),
            Phase::ArmTimers => target.arm_timers(),
            Phase::FlushConnection => target.flush_connection()?,
        }
    }
    Ok(())
}

impl<M: 'static> Flush for HostCtx<M> {
    fn poll_woken(&mut self) {
        super::rescale::follow_all(&mut self.state);
        let state = &mut self.state;
        let ping = state.requests.ping();
        for id in state.woken.take() {
            let Some(surface) = state.surfaces.get_mut(&id) else {
                continue;
            };
            let waker = surface_waker(id, state.woken.clone(), ping.clone());
            let polled = surface.document.poll(&mut Context::from_waker(&waker));
            if polled == Polled::Changed {
                state.frame_input(id, FrameInput::Invalidate);
            }
        }
    }

    fn apply_requests(&mut self) {
        for request in self.state.requests.drain() {
            super::ops::request(self, request);
        }
        super::ops::claim_copied(self);
        super::lock::advance(self);
    }

    fn step_frames(&mut self) {
        super::paint::step_all(self);
    }

    fn arm_timers(&mut self) {
        super::timer::arm_requested(self);
        super::pipe::register_writes(self);
        super::paste::register_read(self);
        super::palmrest::connect_if_due(self);
    }

    fn flush_connection(&mut self) -> Result<()> {
        let conn = &self.state.conn;
        let flushed = conn.flush();
        if let Some(error) = conn.protocol_error() {
            return Err(HostError::Protocol(format!("{error:?}")));
        }
        match flushed {
            Ok(()) => Ok(()),
            // A full socket drains on the next flush; the loop will be woken again.
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                Ok(())
            }
            Err(e) => Err(HostError::Protocol(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests;
