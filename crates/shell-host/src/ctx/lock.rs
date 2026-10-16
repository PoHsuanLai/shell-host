//! The session lock's effects: asking for the lock, a lock surface per output (each its own
//! document from the shell's factory, with a `LockHandle` context), the documents told before
//! `unlock_and_destroy`, and `HostEvent::Lock`. The decisions are `lock::machine`'s; this file
//! only carries them out, in the flush's request phase (`advance`).
//!
//! See FINDINGS F65.

use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::compositor::Surface;
use wayland_client::protocol::wl_output::WlOutput;

use crate::blur::BlurSpec;
use crate::csd::Inset;
use crate::ctx::spawn::{Initial, insert, new_wl_surface};
use crate::ctx::{HostCtx, Running};
use crate::error::HostError;
use crate::event::HostEvent;
use crate::handlers::output::output_info;
use crate::id::SurfaceName;
use crate::id::{OutputId, SurfaceId};
use crate::input::palmrest::lock_for;
use crate::live::Live;
use crate::lock::active::ActiveLock;
use crate::lock::handle::LockHandle;
use crate::lock::machine::{self, Ended, LockAction, LockInput, LockPhase};
use crate::lock::retry::{Retry, retry};
use crate::lock::{LockConfig, LockError, LockId};
use crate::msg::Request;
use crate::output::OutputInfo;
use crate::region::RegionSpec;
use crate::role::Role;
use crate::root::HostRoot;
use crate::scale::Size;
use crate::state::HostState;

impl<M: 'static> HostCtx<M> {
    /// Lock the session (`ext_session_lock_manager_v1.lock`). A lock surface is made at once for
    /// every output, and later for each output added while the lock lasts, each rendering
    /// `factory(output)` with a [`LockHandle`] context (`use_lock`); the compositor gives them
    /// the keyboard. `HostEvent::Lock { id, state: Locked }` follows once the compositor has
    /// locked, or `Finished` when it refuses (another locker holds the lock).
    /// `LockError::Unsupported` without the protocol, `InProgress` while a lock of ours lasts.
    pub fn lock_session(
        &mut self,
        config: LockConfig,
        factory: impl Fn(OutputInfo) -> HostRoot + 'static,
    ) -> Result<LockId, LockError> {
        if let Some(active) = &self.state.lock {
            return Err(LockError::InProgress(active.id));
        }
        let lock = self
            .state
            .session_lock
            .lock(&self.state.qh)
            .map_err(|_| LockError::Unsupported)?;
        let id = self.state.lock_ids.next();
        let (machine, actions) = machine::start(self.outputs().into_iter().map(|info| info.id));
        self.state.lock = Some(ActiveLock {
            id,
            lock,
            machine,
            config,
            factory: Box::new(factory),
            phase: Live::new(LockPhase::Requested),
            surfaces: Default::default(),
            failures: Default::default(),
            inbox: Vec::new(),
        });
        perform_all(self, actions);
        Ok(id)
    }

    /// Unlock session lock `id`, as its documents' `LockHandle::unlock` does: every lock
    /// document is told `Unlocking` and polled, then `unlock_and_destroy` is sent and the lock
    /// surfaces go; `HostEvent::Lock { state: Unlocked }` follows. A stale id does nothing.
    pub fn unlock_session(&mut self, id: LockId) {
        self.state.requests.push(Request::Unlock(id));
    }

    /// Lock `id`'s surfaces by output, while it lasts (for `stats` and debug commands); empty
    /// for any other id.
    pub fn lock_surfaces(&self, id: LockId) -> Vec<(OutputId, SurfaceId)> {
        self.state
            .lock
            .as_ref()
            .filter(|active| active.id == id)
            .map(|active| active.surfaces.iter().map(|(&o, &s)| (o, s)).collect())
            .unwrap_or_default()
    }
}

/// A queued unlock for lock `id`: an input for the lock in force, when it is that lock.
pub(super) fn ask_unlock(state: &mut HostState, id: LockId) {
    if state.lock.as_ref().is_some_and(|active| active.id == id) {
        state.lock_input(LockInput::UnlockAsked);
    }
}

/// Feed the lock's queued inputs to its machine and carry out what it decides. A lock that has
/// ended is dropped, which sends `ext_session_lock_v1.destroy` when it was never locked (after
/// `unlock_and_destroy` that is a no-op).
pub(super) fn advance<M: 'static>(ctx: &mut HostCtx<M>) {
    let Some(active) = ctx.state.lock.as_mut() else {
        return;
    };
    let inputs = std::mem::take(&mut active.inbox);
    for input in inputs {
        let Some(active) = ctx.state.lock.as_mut() else {
            return;
        };
        let (next, actions) = machine::step(&active.machine, input);
        active.machine = next;
        perform_all(ctx, actions);
    }
    if let Some(active) = &ctx.state.lock
        && active.machine.phase.ended() == Ended::Yes
    {
        ctx.state.lock = None;
    }
}

fn perform_all<M: 'static>(ctx: &mut HostCtx<M>, actions: Vec<LockAction>) {
    for action in actions {
        perform(ctx, action);
    }
}

fn perform<M: 'static>(ctx: &mut HostCtx<M>, action: LockAction) {
    let Some(active) = ctx.state.lock.as_mut() else {
        return;
    };
    match action {
        LockAction::CreateSurface(output) => create_surface(ctx, output),
        LockAction::DestroySurface(output) => {
            active.failures.recovered(output);
            if let Some(id) = active.surfaces.remove(&output) {
                remove_surface(ctx, id);
            }
        }
        LockAction::Tell(phase) => {
            active.phase.publish(phase);
            if phase == LockPhase::Unlocking {
                // Told once the documents have been polled: at the next flush, whose first
                // phase polls the ones this publish woke. The ping makes sure there is one.
                active.inbox.push(LockInput::Told);
                ctx.state.requests.ping().ping();
            }
        }
        LockAction::UnlockAndDestroy => active.lock.unlock(),
        // Sent when the ended lock is dropped, at the end of `advance`.
        LockAction::Destroy => {}
        LockAction::Report(state) => {
            let id = active.id;
            // The one place the lock's state changes (the handlers' `locked`/`finished` and an
            // unlock all arrive here), so palmrest hears every change once, in order.
            ctx.state.palmrest.session_lock(lock_for(state));
            ctx.state.events.push_back(HostEvent::Lock { id, state });
        }
    }
}

/// Output `output`'s lock surface, with its document. On failure the output shows the
/// compositor's solid colour, which takes no password, so it is made again after a pause
/// (`surface_failed`).
fn create_surface<M: 'static>(ctx: &mut HostCtx<M>, output: OutputId) {
    let Some((wl_output, info)) = find_output(&ctx.state, output) else {
        return;
    };
    let wl = new_wl_surface(&mut ctx.state);
    let state = &mut ctx.state;
    let Some(active) = state.lock.as_mut() else {
        return;
    };
    let surface = active
        .lock
        .create_lock_surface(Surface::from(wl), &wl_output, &state.qh);
    let handle = LockHandle::new(active.id, state.requests.clone(), active.phase.clone());
    let root = (active.factory)(info.clone()).context(handle);
    let label = info.name.clone().unwrap_or_else(|| output.0.to_string());
    let name = SurfaceName(format!("{}/{label}", active.config.namespace));
    let role = Role::Lock { surface };
    let id = state.ids.next();
    active.surfaces.insert(output, id);
    let at = Initial {
        logical: Size {
            width: info.logical.width,
            height: info.logical.height,
        },
        inset: Inset::NONE,
        scale: info.scale,
    };
    // No commit here: a lock surface's first commit must follow its configure and carry a
    // buffer (the protocol's `commit_before_first_ack`, `null_buffer`).
    insert(
        state,
        id,
        role,
        name,
        at,
        (BlurSpec::None, RegionSpec::Whole),
        root,
    );
    if let Err(error) = super::paint::ensure_presenter(ctx, id) {
        eprintln!("shell-host: lock surface on {output:?}: {error}");
        remove_surface(ctx, id);
        if let Some(active) = ctx.state.lock.as_mut() {
            active.surfaces.remove(&output);
        }
        surface_failed(ctx, output);
    }
}

/// Lock surface `id` was closed while its lock lasts (its presenter failed for good): make it
/// again like one that failed to start.
pub(super) fn surface_lost<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) {
    if ctx.running == Running::Exit {
        return;
    }
    let Some(active) = ctx.state.lock.as_mut() else {
        return;
    };
    let lost = active
        .surfaces
        .iter()
        .find_map(|(&output, &surface)| (surface == id).then_some(output));
    if let Some(output) = lost {
        active.surfaces.remove(&output);
        surface_failed(ctx, output);
    }
}

/// `output` has no lock surface after a failure: say so, and make it again after a pause that
/// grows with each failure in a row; on the only output, stop the host instead after a few, so
/// that its unit starts it again and the session is locked again with a prompt on screen.
fn surface_failed<M: 'static>(ctx: &mut HostCtx<M>, output: OutputId) {
    let Some(active) = ctx.state.lock.as_mut() else {
        return;
    };
    if !active.machine.outputs.contains(&output) {
        return;
    }
    let failures = active.failures.failed(output);
    let lock = active.id;
    match retry(failures, active.machine.outputs.len()) {
        Retry::After(pause) => {
            eprintln!(
                "shell-host: the lock screen on {output:?} failed {failures} time(s) in a row; \
                 trying again in {pause:?}"
            );
            remake_after(ctx, lock, output, pause);
        }
        Retry::GiveUp => {
            eprintln!(
                "shell-host: the only lock screen failed {failures} times in a row; stopping"
            );
            ctx.failed.get_or_insert_with(|| {
                HostError::GaveUp("no lock screen could be made for the only output".into())
            });
        }
    }
}

fn remake_after<M: 'static>(ctx: &mut HostCtx<M>, lock: LockId, output: OutputId, pause: Duration) {
    let armed = ctx
        .handle
        .insert_source(Timer::from_duration(pause), move |_, _, ctx| {
            remake_surface(ctx, lock, output);
            TimeoutAction::Drop
        });
    if let Err(error) = armed {
        eprintln!("shell-host: lock screen retry timer: {error}");
        ctx.failed
            .get_or_insert_with(|| HostError::GaveUp("the lock screen cannot be retried".into()));
    }
}

/// Make `output`'s lock surface again if lock `lock` still wants one there.
fn remake_surface<M: 'static>(ctx: &mut HostCtx<M>, lock: LockId, output: OutputId) {
    let wanted = ctx.state.lock.as_ref().is_some_and(|active| {
        active.id == lock
            && active.machine.outputs.contains(&output)
            && !active.surfaces.contains_key(&output)
    });
    if wanted {
        create_surface(ctx, output);
    }
}

fn find_output(state: &HostState, output: OutputId) -> Option<(WlOutput, OutputInfo)> {
    let outputs = &state.outputs;
    outputs.outputs().find_map(|wl_output| {
        let info = outputs.info(&wl_output)?;
        (info.id == output.0).then(|| (wl_output.clone(), output_info(info.id, &info)))
    })
}

/// Remove lock surface `id` without a `SurfaceClosed` (the shell never held its id): the
/// pointer's stay and the keyboard focus end with it, then the surface
/// (its presenter before its `wl_surface`, and the lock surface object with it).
fn remove_surface<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) {
    ctx.state.withdraw_pointer(id);
    ctx.state.withdraw_keyboard(id);
    super::timer::cancel_watchdog(ctx, id);
    ctx.state.remove_surface(id);
}
