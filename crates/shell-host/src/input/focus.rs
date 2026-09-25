//! The pointer-focus record: which surface the pointer is on, and the `enter` serial a cursor
//! change needs. Pure; `handlers::pointer` holds it and acts on what a departure means.
//!
//! A surface stops having the pointer in two ways. The compositor says so with
//! `wl_pointer.leave`, or the host takes the surface away itself: an unmap rebuilds the
//! `wl_surface` and a close destroys it, and the compositor's leave that follows names an
//! object the host no longer knows (KWin sends `leave(serial, nil)`), so it cannot be matched
//! to anything. The host therefore ends the stay itself at the unmap or close, while the
//! surface is still known, and ignores the late leave (FINDINGS F58).

use crate::id::SurfaceId;

/// The pointer is on `surface`, since the `enter` with `serial`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entered {
    pub surface: SurfaceId,
    pub serial: u32,
}

/// Where the pointer is, as far as the host's surfaces go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerFocus {
    /// On none of the host's surfaces.
    Nowhere,
    On(Entered),
}

impl PointerFocus {
    /// The surface the pointer is on, if any.
    pub fn surface(self) -> Option<SurfaceId> {
        match self {
            PointerFocus::Nowhere => None,
            PointerFocus::On(entered) => Some(entered.surface),
        }
    }
}

/// What a surface's departure (its leave, unmap or close) meant for the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Departure {
    /// The pointer was on it; the record is now `Nowhere`, and the pointer's per-surface state
    /// (zone, held buttons, the document's hover) goes with it.
    Lost,
    /// The pointer was on another surface or none; the record is unchanged.
    Unaffected,
}

/// The record after an `enter` on `surface` with `serial`, whatever it was before: an enter
/// always names where the pointer is now.
pub fn enter(surface: SurfaceId, serial: u32) -> PointerFocus {
    PointerFocus::On(Entered { surface, serial })
}

/// The record after `surface` departs, and whether that took the pointer's focus.
pub fn depart(focus: PointerFocus, surface: SurfaceId) -> (PointerFocus, Departure) {
    match focus {
        PointerFocus::On(entered) if entered.surface == surface => {
            (PointerFocus::Nowhere, Departure::Lost)
        }
        PointerFocus::On(_) | PointerFocus::Nowhere => (focus, Departure::Unaffected),
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
            (on_a, A, PointerFocus::Nowhere, Departure::Lost),
            (on_a, B, on_a, Departure::Unaffected),
            (
                PointerFocus::Nowhere,
                A,
                PointerFocus::Nowhere,
                Departure::Unaffected,
            ),
        ];
        for (focus, surface, after, departure) in cases {
            assert_eq!(
                depart(focus, surface),
                (after, departure),
                "{focus:?} - {surface:?}"
            );
        }
    }

    /// Unmapping the surface under the pointer, then the compositor's enter on the one beneath:
    /// the record follows the pointer, and a second departure of the unmapped surface (its late
    /// leave, were it ever matched) changes nothing.
    #[test]
    fn unmap_then_enter_beneath() {
        assert_eq!(
            depart(enter(A, 3), A),
            (PointerFocus::Nowhere, Departure::Lost)
        );
        let focus = enter(B, 4);
        assert_eq!(focus.surface(), Some(B));
        assert_eq!(depart(focus, A), (focus, Departure::Unaffected));
    }

    #[test]
    fn an_enter_replaces_the_serial() {
        let again = enter(A, 9);
        assert_eq!(
            again,
            PointerFocus::On(Entered {
                surface: A,
                serial: 9
            })
        );
        assert_ne!(again, enter(A, 3));
    }
}
