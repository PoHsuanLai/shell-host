//! Phase 4 of the flush: arming and cancelling the timers the handlers and the frame machine
//! asked for (`timers::TimerRequest`, FINDINGS F9). Each surface has at most one watchdog; the
//! key-repeat timer is one per host.

use std::time::{Duration, Instant};

use calloop::timer::{TimeoutAction, Timer};

use crate::ctx::HostCtx;
use crate::frame::{FrameInput, TimerCmd};
use crate::id::SurfaceId;
use crate::timers::TimerRequest;

/// Apply every queued timer request, oldest first.
pub(super) fn arm_requested<M: 'static>(ctx: &mut HostCtx<M>) {
    for request in std::mem::take(&mut ctx.state.timers) {
        match request {
            TimerRequest::Watchdog {
                id,
                cmd: TimerCmd::Arm(wait),
            } => arm_watchdog(ctx, id, wait),
            TimerRequest::Watchdog {
                id,
                cmd: TimerCmd::Cancel,
            } => cancel_watchdog(ctx, id),
            TimerRequest::Watchdog {
                cmd: TimerCmd::Keep,
                ..
            } => {}
            TimerRequest::RepeatStart { delay, interval } => start_repeat(ctx, delay, interval),
            TimerRequest::RepeatStop => stop_repeat(ctx),
        }
    }
}

fn arm_watchdog<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId, wait: Duration) {
    cancel_watchdog(ctx, id);
    let armed = ctx
        .handle
        .insert_source(Timer::from_duration(wait), move |_, _, ctx| {
            watchdog_fired(ctx, id);
            TimeoutAction::Drop
        });
    match armed {
        Ok(token) => {
            ctx.watchdogs.insert(id, token);
            if let Some(surface) = ctx.state.surfaces.get_mut(&id) {
                surface.watchdog = Some(token);
            }
        }
        // Without a timer the machine would wait for a callback that may never come; fire now
        // instead, so the next flush retries at once.
        Err(error) => {
            eprintln!("shell-host: watchdog timer for {id:?}: {error}");
            watchdog_fired(ctx, id);
        }
    }
}

fn watchdog_fired<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) {
    ctx.watchdogs.remove(&id);
    let at = ctx.state.clock.instant_at(Instant::now());
    if let Some(surface) = ctx.state.surfaces.get_mut(&id) {
        surface.watchdog = None;
        surface.stats.watchdog_fired();
        // The expiry stands in for the callback that never came, so a driven surface still
        // ticks, at the watchdog's pace.
        surface.frame_due(at);
    }
    ctx.state.frame_input(id, FrameInput::WatchdogFired);
}

/// Remove surface `id`'s pending watchdog, if any.
pub(super) fn cancel_watchdog<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) {
    if let Some(token) = ctx.watchdogs.remove(&id) {
        ctx.handle.remove(token);
    }
    if let Some(surface) = ctx.state.surfaces.get_mut(&id) {
        surface.watchdog = None;
    }
}

fn start_repeat<M: 'static>(ctx: &mut HostCtx<M>, delay: Duration, interval: Duration) {
    stop_repeat(ctx);
    let started = ctx
        .handle
        .insert_source(Timer::from_duration(delay), move |_, _, ctx| {
            ctx.state.key_repeat();
            TimeoutAction::ToDuration(interval)
        });
    match started {
        Ok(token) => ctx.repeat = Some(token),
        Err(error) => eprintln!("shell-host: key repeat timer: {error}"),
    }
}

fn stop_repeat<M: 'static>(ctx: &mut HostCtx<M>) {
    if let Some(token) = ctx.repeat.take() {
        ctx.handle.remove(token);
    }
}
