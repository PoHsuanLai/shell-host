//! What a seat capability announcement means for the device object the host holds: the pure
//! decision behind `handlers::seat`'s `new_capability` and `remove_capability`.
//!
//! A compositor re-announces a capability whenever its set of devices changes: KWin drops and
//! re-adds `pointer` for every `org_kde_kwin_fake_input` client, and a real seat does the same
//! when a mouse is unplugged and plugged back. Every `wl_pointer` a client has not
//! released keeps receiving events, so an object created per announcement, with the old one
//! merely dropped, delivers each press once per object. The host therefore holds at most one
//! object per capability: it creates one only when it holds none, and releases the one it holds
//! when the capability goes.

/// What the host does with the capability's device object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityChange {
    /// Create the object (and what hangs off it: the cursor-shape device).
    New,
    /// Keep the object already held: a second one would receive every event again.
    Repeat,
    /// Release the object (`wl_pointer.release`/`wl_keyboard.release`) and forget the state that
    /// came from it: focus, held buttons, the repeating key.
    Removed,
    /// Nothing to release: the capability went before an object was ever made.
    Absent,
}

/// The decision table. `added`: the seat announced the capability (sctk's `new_capability`),
/// else it dropped it (`remove_capability`). `held`: the host already holds the capability's
/// device object (`wl_pointer`, `wl_keyboard`).
pub(crate) fn capability_change(added: bool, held: bool) -> CapabilityChange {
    match (added, held) {
        (true, false) => CapabilityChange::New,
        (true, true) => CapabilityChange::Repeat,
        (false, true) => CapabilityChange::Removed,
        (false, false) => CapabilityChange::Absent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// KWin's sequence for one fake-input client connecting and leaving, replayed against one
    /// held object: each step's decision, and the object count after it, never above one.
    #[test]
    fn a_reannounce_cycle_never_holds_two_objects() {
        let sequence = [
            (true, CapabilityChange::New),
            (false, CapabilityChange::Removed),
            (true, CapabilityChange::New),
            (true, CapabilityChange::Repeat),
            (false, CapabilityChange::Removed),
            (false, CapabilityChange::Absent),
        ];
        let mut objects = 0_u32;
        for (step, (announcement, want)) in sequence.into_iter().enumerate() {
            let change = capability_change(announcement, objects > 0);
            assert_eq!(change, want, "step {step}");
            objects = match change {
                CapabilityChange::New => objects + 1,
                CapabilityChange::Removed => objects - 1,
                CapabilityChange::Repeat | CapabilityChange::Absent => objects,
            };
            assert!(objects <= 1, "step {step}: {objects} objects held");
        }
        assert_eq!(objects, 0);
    }
}
