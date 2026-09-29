//! The serial a drag starts from.
//!
//! `wl_data_device.start_drag` takes the serial of the implicit grab it continues: a pointer
//! press that is still down, on the drag's origin surface. A compositor ignores (KWin) or
//! cancels (smithay) a drag with any other serial, so the host refuses it up front. The press
//! is the one the grab record keeps (`grab::PointerFacts::press`, set on every press, cleared
//! when the last button comes up or the pointer leaves); unlike a move, the pointer need not
//! still be on the surface, since the implicit grab keeps delivering to it.

use crate::drag::outcome::DragRefused;
use crate::grab::PointerFacts;
use crate::id::SurfaceId;

/// The serial to start a drag from surface `id` with, or why none fits.
pub(crate) fn drag_serial(facts: PointerFacts, id: SurfaceId) -> Result<u32, DragRefused> {
    match facts.press {
        None => Err(DragRefused::NoPress),
        Some(press) if press.surface != id => Err(DragRefused::PressedElsewhere),
        Some(press) => Ok(press.serial),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grab::Press;

    #[test]
    fn a_drag_starts_from_the_held_press_on_its_surface() {
        let me = SurfaceId(1);
        let other = SurfaceId(2);
        let press = |surface, serial| Some(Press { surface, serial });
        let cases = [
            (
                "pressed here, pointer here",
                PointerFacts {
                    focus: Some(me),
                    press: press(me, 7),
                },
                Ok(7),
            ),
            (
                "pressed here, pointer already reported elsewhere",
                PointerFacts {
                    focus: Some(other),
                    press: press(me, 8),
                },
                Ok(8),
            ),
            (
                "no button held",
                PointerFacts {
                    focus: Some(me),
                    press: None,
                },
                Err(DragRefused::NoPress),
            ),
            (
                "pressed on another surface",
                PointerFacts {
                    focus: Some(me),
                    press: press(other, 9),
                },
                Err(DragRefused::PressedElsewhere),
            ),
            (
                "nothing known",
                PointerFacts::default(),
                Err(DragRefused::NoPress),
            ),
        ];
        for (name, facts, want) in cases {
            assert_eq!(drag_serial(facts, me), want, "{name}");
        }
    }
}
