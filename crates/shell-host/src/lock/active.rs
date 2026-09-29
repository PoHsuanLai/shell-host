//! The lock in force: the protocol object, the pure machine, the documents' factory and the
//! surface made for each output. Only `ctx::lock` changes it; the handlers only add inputs.

use std::collections::BTreeMap;
use std::fmt;

use smithay_client_toolkit::session_lock::SessionLock;

use crate::id::{OutputId, SurfaceId};
use crate::live::Live;
use crate::lock::machine::{LockInput, LockMachine, LockPhase};
use crate::lock::{LockConfig, LockId};
use crate::output::OutputInfo;
use crate::root::HostRoot;

/// Builds one output's lock document.
pub(crate) type LockFactory = Box<dyn Fn(OutputInfo) -> HostRoot>;

/// A session lock from its request until it ends.
pub(crate) struct ActiveLock {
    pub(crate) id: LockId,
    /// Dropping the last clone sends `ext_session_lock_v1.destroy` (a no-op after
    /// `unlock_and_destroy`), so it is dropped only once the machine has ended.
    pub(crate) lock: SessionLock,
    pub(crate) machine: LockMachine,
    pub(crate) config: LockConfig,
    pub(crate) factory: LockFactory,
    /// Published to every lock document's `LockHandle`.
    pub(crate) phase: Live<LockPhase>,
    pub(crate) surfaces: BTreeMap<OutputId, SurfaceId>,
    /// Inputs for the next `ctx::lock::advance` (from the handlers, the requests, and the
    /// previous advance's `Told`).
    pub(crate) inbox: Vec<LockInput>,
}

impl fmt::Debug for ActiveLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActiveLock")
            .field("id", &self.id)
            .field("machine", &self.machine)
            .field("config", &self.config)
            .field("surfaces", &self.surfaces)
            .field("inbox", &self.inbox)
            .finish_non_exhaustive()
    }
}

/// The next `LockId`s.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LockIds(u64);

impl LockIds {
    /// The next unused id; the first is `LockId(1)`.
    pub(crate) fn next(&mut self) -> LockId {
        self.0 += 1;
        LockId(self.0)
    }
}
