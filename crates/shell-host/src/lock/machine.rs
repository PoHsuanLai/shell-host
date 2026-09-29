//! The session lock's state machine (`ext-session-lock-v1`), pure: what the host does in answer
//! to the compositor's `locked` and `finished`, outputs coming and going, and an unlock.
//!
//! ```text
//! Requested --locked--> Locked --unlock--> Unlocking --told--> Unlocked
//!     |                   |                    |
//!     +--finished--> Finished <--finished------+
//!     +--unlock----> Unlocked (cancelled: `destroy`, never `unlock_and_destroy`)
//! ```
//!
//! Lock surfaces are made at the request, one per output, as the protocol asks ("clients should
//! immediately create lock surfaces for all outputs on creation of this object"), so the
//! compositor can show them in the first locked frame; an output added before the lock ends
//! gets one too, and a removed output's is destroyed. `Unlocking` lasts until the documents have
//! been told and polled (`Told`), and only then is `unlock_and_destroy` sent.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::id::OutputId;

/// Where a session lock is, as its documents see it (`LockHandle::phase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockPhase {
    /// Asked for; the compositor has not answered yet.
    Requested,
    /// The compositor says the session is locked.
    Locked,
    /// An unlock was asked for; the documents are being told before the compositor is.
    Unlocking,
    /// `unlock_and_destroy` was sent (or the request was cancelled before `locked`).
    Unlocked,
    /// The compositor refused or ended the lock (`finished`).
    Finished,
}

impl LockPhase {
    /// Whether the lock is over: nothing moves it any more.
    pub fn ended(self) -> Ended {
        match self {
            LockPhase::Unlocked | LockPhase::Finished => Ended::Yes,
            LockPhase::Requested | LockPhase::Locked | LockPhase::Unlocking => Ended::No,
        }
    }
}

/// Whether a lock has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ended {
    Yes,
    No,
}

/// What the host reports to the shell (`HostEvent::Lock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    /// The compositor locked the session: only lock surfaces are shown and get input.
    Locked,
    /// The host unlocked the session (or cancelled the request before it was locked).
    Unlocked,
    /// The compositor refused the lock (another locker holds it) or ended it by its own means.
    Finished,
}

/// Something that happened to the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockInput {
    /// `ext_session_lock_v1.locked`.
    Locked,
    /// `ext_session_lock_v1.finished`.
    Finished,
    OutputAdded(OutputId),
    OutputRemoved(OutputId),
    /// `LockHandle::unlock` or `HostCtx::unlock_session`.
    UnlockAsked,
    /// The documents were told `Unlocking` and have been polled since.
    Told,
}

/// What the host must do, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockAction {
    CreateSurface(OutputId),
    DestroySurface(OutputId),
    /// Publish the phase to every lock document.
    Tell(LockPhase),
    /// `ext_session_lock_v1.unlock_and_destroy` (only after `locked`).
    UnlockAndDestroy,
    /// `ext_session_lock_v1.destroy` (only before `locked`).
    Destroy,
    Report(LockState),
}

/// A lock's phase and the outputs that have a lock surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LockMachine {
    pub phase: LockPhase,
    pub outputs: BTreeSet<OutputId>,
}

/// A new lock over `outputs`: `Requested`, a surface for each.
pub(crate) fn start(outputs: impl IntoIterator<Item = OutputId>) -> (LockMachine, Vec<LockAction>) {
    let outputs: BTreeSet<OutputId> = outputs.into_iter().collect();
    let actions = outputs
        .iter()
        .map(|&output| LockAction::CreateSurface(output))
        .collect();
    let machine = LockMachine {
        phase: LockPhase::Requested,
        outputs,
    };
    (machine, actions)
}

/// The machine after `input`, and what to do. An input that means nothing in the current
/// phase (a second `locked`, an unlock while unlocking, anything after the end) does nothing.
pub(crate) fn step(machine: &LockMachine, input: LockInput) -> (LockMachine, Vec<LockAction>) {
    let phase = machine.phase;
    match (phase, input) {
        (LockPhase::Unlocked | LockPhase::Finished, _) => (machine.clone(), Vec::new()),
        (_, LockInput::OutputAdded(output)) => add_output(machine, output),
        (_, LockInput::OutputRemoved(output)) => remove_output(machine, output),
        (LockPhase::Requested, LockInput::Locked) => (
            with_phase(machine, LockPhase::Locked),
            vec![
                LockAction::Tell(LockPhase::Locked),
                LockAction::Report(LockState::Locked),
            ],
        ),
        (LockPhase::Requested, LockInput::Finished) => end(
            machine,
            LockAction::Destroy,
            LockPhase::Finished,
            LockState::Finished,
        ),
        (LockPhase::Requested, LockInput::UnlockAsked) => end(
            machine,
            LockAction::Destroy,
            LockPhase::Unlocked,
            LockState::Unlocked,
        ),
        (LockPhase::Locked, LockInput::UnlockAsked) => (
            with_phase(machine, LockPhase::Unlocking),
            vec![LockAction::Tell(LockPhase::Unlocking)],
        ),
        (LockPhase::Unlocking, LockInput::Told) => end(
            machine,
            LockAction::UnlockAndDestroy,
            LockPhase::Unlocked,
            LockState::Unlocked,
        ),
        // Locked once: the object must go with `unlock_and_destroy` even when the compositor
        // ends it (the protocol's `finished`).
        (LockPhase::Locked | LockPhase::Unlocking, LockInput::Finished) => end(
            machine,
            LockAction::UnlockAndDestroy,
            LockPhase::Finished,
            LockState::Finished,
        ),
        (
            LockPhase::Requested | LockPhase::Locked | LockPhase::Unlocking,
            LockInput::Locked | LockInput::UnlockAsked | LockInput::Told,
        ) => (machine.clone(), Vec::new()),
    }
}

fn with_phase(machine: &LockMachine, phase: LockPhase) -> LockMachine {
    LockMachine {
        phase,
        outputs: machine.outputs.clone(),
    }
}

fn add_output(machine: &LockMachine, output: OutputId) -> (LockMachine, Vec<LockAction>) {
    if machine.outputs.contains(&output) {
        return (machine.clone(), Vec::new());
    }
    let mut next = machine.clone();
    next.outputs.insert(output);
    (next, vec![LockAction::CreateSurface(output)])
}

fn remove_output(machine: &LockMachine, output: OutputId) -> (LockMachine, Vec<LockAction>) {
    let mut next = machine.clone();
    if next.outputs.remove(&output) {
        (next, vec![LockAction::DestroySurface(output)])
    } else {
        (next, Vec::new())
    }
}

/// The end of the lock: the protocol request, every surface destroyed, the documents told, the
/// shell told.
fn end(
    machine: &LockMachine,
    request: LockAction,
    phase: LockPhase,
    report: LockState,
) -> (LockMachine, Vec<LockAction>) {
    let mut actions = vec![request];
    actions.extend(
        machine
            .outputs
            .iter()
            .map(|&output| LockAction::DestroySurface(output)),
    );
    actions.push(LockAction::Tell(phase));
    actions.push(LockAction::Report(report));
    let next = LockMachine {
        phase,
        outputs: BTreeSet::new(),
    };
    (next, actions)
}

#[cfg(test)]
mod tests;
