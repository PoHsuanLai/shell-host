//! The session lock (`ext-session-lock-v1`): the shell's own lock screen, one document per
//! output (quire design/20 §1.9). `HostCtx::lock_session` asks for it, `use_lock` is what a lock
//! document reads, `machine` is the pure state machine, `active` the host's record of the lock
//! in force. Authentication is the shell's (PAM in sill), never the host's.

pub(crate) mod active;
pub mod handle;
pub mod machine;

use serde::{Deserialize, Serialize};

/// One session lock, from `HostCtx::lock_session`. Never reused within a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LockId(pub u64);

/// How lock surfaces are set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockConfig {
    /// The lock surfaces' name prefix: each is `<namespace>/<output name>` (the output's
    /// registry name when it has no connector name), for `HostCtx::surface_named` and stats.
    pub namespace: String,
}

impl Default for LockConfig {
    fn default() -> LockConfig {
        LockConfig {
            namespace: "lock".to_owned(),
        }
    }
}

/// Why `HostCtx::lock_session` could not ask for a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum LockError {
    /// The compositor has no `ext_session_lock_manager_v1` (KWin 6.7, for one).
    #[error("the compositor does not provide ext_session_lock_manager_v1")]
    Unsupported,
    /// A lock of this host's is still in force (requested, locked or unlocking).
    #[error("session lock {0:?} is still in force")]
    InProgress(LockId),
}
