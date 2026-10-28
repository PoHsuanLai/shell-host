//! Focus records: which surface the pointer is on (with the `enter` serial a cursor change
//! needs), and which surface holds the keyboard. Pure; `handlers::pointer` and
//! `handlers::keyboard` hold the records and act on what a departure means.
//!
//! A surface stops having a focus in two ways. The compositor says so with `wl_pointer.leave`
//! or `wl_keyboard.leave`, or the host takes the surface away itself: an unmap rebuilds the
//! `wl_surface` and a close destroys it, and the compositor's leave that follows names an
//! object the host no longer knows (KWin sends `leave(serial, nil)` for the pointer; the
//! keyboard's leave names the destroyed or superseded `wl_surface`), so it cannot be matched to
//! anything. The host therefore ends the stay itself at the unmap or close, while the surface is
//! still known, and ignores the late leave, for the pointer and the keyboard alike.

use crate::id::SurfaceId;

/// The pointer is on `surface`, since the `enter` with `serial`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entered {
    pub surface: SurfaceId,
    pub serial: u32,
}

/// Where the pointer is, as far as the host's surfaces go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerFocus {
    /// On none of the host's surfaces.
    Nowhere,
    On(Entered),
}

impl PointerFocus {
    /// The surface the pointer is on, if any.
    pub(crate) fn surface(self) -> Option<SurfaceId> {
        match self {
            PointerFocus::Nowhere => None,
            PointerFocus::On(entered) => Some(entered.surface),
        }
    }
}

/// The record after an `enter` on `surface` with `serial`, whatever it was before: an enter
/// always names where the pointer is now.
pub(crate) fn enter(surface: SurfaceId, serial: u32) -> PointerFocus {
    PointerFocus::On(Entered { surface, serial })
}

/// The record after `surface` departs (its leave, unmap or close), and whether that took the
/// pointer's focus (lost): the record is now `Nowhere`, and the pointer's per-surface state
/// (zone, held buttons, the document's hover) goes with it. Otherwise the pointer was on another
/// surface or none, and the record is unchanged.
pub(crate) fn depart(focus: PointerFocus, surface: SurfaceId) -> (PointerFocus, bool) {
    match focus {
        PointerFocus::On(entered) if entered.surface == surface => (PointerFocus::Nowhere, true),
        PointerFocus::On(_) | PointerFocus::Nowhere => (focus, false),
    }
}

/// The record after `surface` departs keyboard focus (a `wl_keyboard.leave`, an unmap or a
/// close), and whether that took it. Unlike the pointer's record there is no serial to keep —
/// a surface either holds the keyboard or it does not — so the record is a bare
/// `Option<SurfaceId>` rather than its own type.
pub(crate) fn depart_keyboard(
    focus: Option<SurfaceId>,
    surface: SurfaceId,
) -> (Option<SurfaceId>, bool) {
    if focus == Some(surface) {
        (None, true)
    } else {
        (focus, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: SurfaceId = SurfaceId(1);
    const B: SurfaceId = SurfaceId(2);

    #[test]
    fn departures() {
        let on_a = enter(A, 7);
        let cases = [
            (on_a, A, PointerFocus::Nowhere, true),
            (on_a, B, on_a, false),
            (PointerFocus::Nowhere, A, PointerFocus::Nowhere, false),
        ];
        for (focus, surface, after, departure) in cases {
            assert_eq!(
                depart(focus, surface),
                (after, departure),
                "{focus:?} - {surface:?}"
            );
        }
        // After an unmap, the compositor's enter on the surface beneath: the record follows.
        assert_eq!(enter(B, 4).surface(), Some(B));
    }

    #[test]
    fn keyboard_departures() {
        let cases = [
            (Some(A), A, None, true),
            (Some(A), B, Some(A), false),
            (None, A, None, false),
        ];
        for (focus, surface, after, departure) in cases {
            assert_eq!(
                depart_keyboard(focus, surface),
                (after, departure),
                "{focus:?} - {surface:?}"
            );
        }
    }
}
