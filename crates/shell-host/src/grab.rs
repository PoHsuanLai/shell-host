//! The serial rule for interactive move and resize.
//!
//! `xdg_toplevel.move` and `.resize` take a seat and the serial of the user action that started
//! them, and a compositor starts the grab only while that action is still going on: smithay
//! (cosmic-comp) checks that the pointer's current implicit grab began with that serial, KWin
//! that the serial is the pointer's latest button press. A request with any other serial is
//! ignored without an error, which would leave a titlebar that silently does not drag. So the
//! host keeps, per seat, the serial of the last button press and the surface it landed on, and
//! refuses up front, with a reason, whenever the compositor would ignore the request: no button
//! is held, it was pressed on another surface, or the pointer is not on this surface.
//!
//! The record is shared between the pointer handler (which writes it) and every
//! `SurfaceHandle` (which checks it synchronously, so `begin_move()` can return the refusal),
//! on the main thread only, hence `Rc<Cell>`.
//!
//! See FINDINGS F49.

use std::cell::Cell;
use std::rc::Rc;

use crate::id::SurfaceId;

/// A button press: the surface under the pointer and the `wl_pointer.button` serial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Press {
    pub surface: SurfaceId,
    pub serial: u32,
}

/// What the grab rule needs to know about the seat's pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PointerFacts {
    /// The surface the pointer is on (since its `enter`, until its `leave`).
    pub focus: Option<SurfaceId>,
    /// The latest press while any button is held; `None` once every button is up.
    pub press: Option<Press>,
}

/// Why a move or resize was not started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GrabRefused {
    /// The pointer is not on this surface.
    #[error("the pointer is not on this surface")]
    PointerElsewhere,
    /// No button is held: a move or resize must start from a press that is still down.
    #[error("no pointer button is held")]
    NoPress,
    /// The held button was pressed on another surface.
    #[error("the held button was pressed on another surface")]
    PressedElsewhere,
    /// The surface is not an xdg toplevel.
    #[error("only a toplevel can be moved or resized")]
    NotAToplevel,
    /// The compositor has not given the host a seat.
    #[error("no seat")]
    NoSeat,
}

/// The serial to start a move or resize of surface `id` with, or why the compositor would
/// ignore it. Pure.
pub(crate) fn grab_serial(facts: PointerFacts, id: SurfaceId) -> Result<u32, GrabRefused> {
    if facts.focus != Some(id) {
        return Err(GrabRefused::PointerElsewhere);
    }
    match facts.press {
        None => Err(GrabRefused::NoPress),
        Some(press) if press.surface != id => Err(GrabRefused::PressedElsewhere),
        Some(press) => Ok(press.serial),
    }
}

/// The seat's [`PointerFacts`], shared by the pointer handler and every `SurfaceHandle`.
#[derive(Debug, Clone, Default)]
pub(crate) struct GrabRecord(Rc<Cell<PointerFacts>>);

impl GrabRecord {
    /// The facts now.
    pub(crate) fn get(&self) -> PointerFacts {
        self.0.get()
    }

    /// Change the facts with `change`.
    pub(crate) fn update(&self, change: impl FnOnce(&mut PointerFacts)) {
        let mut facts = self.0.get();
        change(&mut facts);
        self.0.set(facts);
    }

    /// [`grab_serial`] on the facts now.
    pub(crate) fn serial_for(&self, id: SurfaceId) -> Result<u32, GrabRefused> {
        grab_serial(self.get(), id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grab_needs_the_pointer_and_a_held_press_on_the_same_surface() {
        let me = SurfaceId(1);
        let other = SurfaceId(2);
        let press = |surface| {
            Some(Press {
                surface,
                serial: 40,
            })
        };
        let cases = [
            (
                "pointer and press here",
                PointerFacts {
                    focus: Some(me),
                    press: press(me),
                },
                Ok(40),
            ),
            (
                "no button held",
                PointerFacts {
                    focus: Some(me),
                    press: None,
                },
                Err(GrabRefused::NoPress),
            ),
            (
                "pressed on another surface, then moved here",
                PointerFacts {
                    focus: Some(me),
                    press: press(other),
                },
                Err(GrabRefused::PressedElsewhere),
            ),
            (
                "pressed here, pointer has left",
                PointerFacts {
                    focus: Some(other),
                    press: press(me),
                },
                Err(GrabRefused::PointerElsewhere),
            ),
            (
                "pointer on no surface",
                PointerFacts::default(),
                Err(GrabRefused::PointerElsewhere),
            ),
        ];
        for (name, facts, want) in cases {
            assert_eq!(grab_serial(facts, me), want, "{name}");
        }
    }

    #[test]
    fn every_clone_sees_the_same_facts() {
        let record = GrabRecord::default();
        let seen_by_a_handle = record.clone();
        record.update(|facts| facts.focus = Some(SurfaceId(3)));
        assert_eq!(seen_by_a_handle.get().focus, Some(SurfaceId(3)));
    }
}
