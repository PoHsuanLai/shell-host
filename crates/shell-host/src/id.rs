//! Identifiers the host hands out. Newtypes, so a surface id can never be passed where an
//! output id belongs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// One surface the host manages: a layer surface, a popup or a toplevel. Never reused within a
/// process, so an id held after its surface closed simply stops matching anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceId(pub u64);

/// One `wl_output`, by the registry name the compositor gave its global. Stable for as long as
/// the output stays connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutputId(pub u32);

/// Hands out [`SurfaceId`]s. Shared between the host and every `SurfaceHandle`, because
/// `SurfaceHandle::open_popup` must return the popup's id before the host has created it.
#[derive(Debug, Clone, Default)]
pub struct SurfaceIds(Arc<AtomicU64>);

impl SurfaceIds {
    /// A fresh allocator; the first id it returns is `SurfaceId(1)`.
    pub fn new() -> SurfaceIds {
        SurfaceIds::default()
    }

    /// The next unused id.
    pub fn next(&self) -> SurfaceId {
        // Relaxed is enough: uniqueness needs only the atomicity of the add, not ordering with
        // any other memory.
        SurfaceId(self.0.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_start_at_one() {
        let ids = SurfaceIds::new();
        let shared = ids.clone();
        let got = [ids.next(), shared.next(), ids.next()];
        assert_eq!(got, [SurfaceId(1), SurfaceId(2), SurfaceId(3)]);
    }
}
