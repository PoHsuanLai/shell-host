//! Hover changes a resolve makes on its own: the pure part (FINDINGS F59).
//!
//! Blitz ends every `resolve` with `refresh_hover()`, which re-hit-tests the last pointer
//! position against the fresh layout and moves the hovered node without dispatching anything
//! (its own TODO: no enter/leave synthesis). The event driver diffs a move against that hover
//! state, so an element that slides in under a resting pointer never gets its
//! `pointerenter`/`mouseenter`, and the one it covered never gets its leave, until the pointer
//! leaves the chain and comes back.
//!
//! The host repairs it around the resolve (`SurfaceDocument::resolve`): when the hovered
//! element changed and the document has seen a real move, put the old hover back without
//! dispatch ([`HoverSync::Restore`] at a point that hits it, or [`HoverSync::Clear`] when none
//! does), then feed the last move again, so Blitz's own driver dispatches exactly the
//! leave/enter diff. This module decides which, and remembers the move.

use blitz_traits::events::{BlitzPointerEvent, MouseEventButton, UiEvent};

use crate::input::pointer::PointerPos;

/// The last pointer event with a position that the document was fed, as a move to feed again.
#[derive(Debug, Clone, Default)]
pub enum LastMove {
    /// No pointer on the surface, or it left: nothing to feed again.
    #[default]
    Unknown,
    At(BlitzPointerEvent),
}

/// The record after the document is fed `event`. A move is kept as it is; a press or release
/// is kept as a move at its point with the buttons it left held, so the move fed again carries
/// the buttons that are down now.
pub fn remember(last: LastMove, event: &UiEvent) -> LastMove {
    match event {
        UiEvent::PointerMove(moved) => LastMove::At(moved.clone()),
        UiEvent::PointerDown(changed) | UiEvent::PointerUp(changed) => {
            LastMove::At(BlitzPointerEvent {
                button: MouseEventButton::Main,
                ..changed.clone()
            })
        }
        _ => last,
    }
}

/// The hovered element before and after one resolve (an element: a hovered text node counts
/// as its element, which is what carries the listeners).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shift<N> {
    pub before: Option<N>,
    pub after: Option<N>,
}

/// A laid-out box in surface-local logical px, fractional as Blitz reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What to do after a resolve, before the move is fed again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoverSync {
    /// Nothing moved, or no move to feed again.
    Unchanged,
    /// Hover this point (no dispatch): it hits the element hovered before.
    Restore(PointerPos),
    /// Hover nothing (no dispatch): nothing was hovered before, or the old element cannot be
    /// hit any more (removed from layout, or covered at every probe).
    Clear,
}

/// Where the old element is probed: its centre, then its corners 1 px inside, so an element
/// covered in the middle (a banner over part of a list) is still found at an edge.
pub fn probe_points(bounds: Bounds) -> [PointerPos; 5] {
    let inset_x = 1.0_f64.min(bounds.width / 2.0);
    let inset_y = 1.0_f64.min(bounds.height / 2.0);
    let (left, top) = (bounds.x + inset_x, bounds.y + inset_y);
    let right = bounds.x + bounds.width - inset_x;
    let bottom = bounds.y + bounds.height - inset_y;
    [
        PointerPos {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        },
        PointerPos { x: left, y: top },
        PointerPos { x: right, y: top },
        PointerPos { x: left, y: bottom },
        PointerPos {
            x: right,
            y: bottom,
        },
    ]
}

/// The repair for `shift`: `bounds` gives the old element's current box, `hit` the element at
/// a point. Both are only asked when the hover really moved under a known pointer.
pub fn decide<N: Copy + PartialEq>(
    shift: Shift<N>,
    last: &LastMove,
    bounds: impl FnOnce(N) -> Option<Bounds>,
    mut hit: impl FnMut(PointerPos) -> Option<N>,
) -> HoverSync {
    if matches!(last, LastMove::Unknown) || shift.before == shift.after {
        return HoverSync::Unchanged;
    }
    let Some(before) = shift.before else {
        return HoverSync::Clear;
    };
    bounds(before)
        .into_iter()
        .flat_map(probe_points)
        .find(|&point| hit(point) == Some(before))
        .map_or(HoverSync::Clear, HoverSync::Restore)
}

#[cfg(test)]
mod tests {
    use blitz_traits::events::{MouseEventButtons, PointerCoords};
    use keyboard_types::Modifiers;

    use super::*;
    use crate::input::pointer::{self, move_event};

    const AT: PointerPos = PointerPos { x: 50.0, y: 20.0 };

    fn known() -> LastMove {
        remember(
            LastMove::Unknown,
            &move_event(AT, MouseEventButtons::None, Modifiers::empty()),
        )
    }

    /// A 100 x 40 box at (10, 10) whose element is 1, covered by element 2 everywhere in its
    /// left `covered_to` px; outside the box is element 0.
    fn world(covered_to: f64) -> impl Fn(PointerPos) -> Option<u32> {
        move |p: PointerPos| {
            let inside = (10.0..=110.0).contains(&p.x) && (10.0..=50.0).contains(&p.y);
            match (inside, p.x < 10.0 + covered_to) {
                (false, _) => Some(0),
                (true, true) => Some(2),
                (true, false) => Some(1),
            }
        }
    }

    const BOX: Bounds = Bounds {
        x: 10.0,
        y: 10.0,
        width: 100.0,
        height: 40.0,
    };

    #[test]
    fn a_changed_hover_under_a_known_pointer_restores_the_old_element_or_clears() {
        type Case = (
            &'static str,
            Option<u32>,
            Option<u32>,
            f64,
            Option<Bounds>,
            HoverSync,
        );
        let cases: [Case; 7] = [
            (
                "unchanged",
                Some(1),
                Some(1),
                0.0,
                Some(BOX),
                HoverSync::Unchanged,
            ),
            (
                "nothing before",
                None,
                Some(2),
                0.0,
                Some(BOX),
                HoverSync::Clear,
            ),
            (
                "uncovered: the centre",
                Some(1),
                Some(3),
                0.0,
                Some(BOX),
                HoverSync::Restore(PointerPos { x: 60.0, y: 30.0 }),
            ),
            (
                "left part covered: the top-right corner",
                Some(1),
                Some(2),
                70.0,
                Some(BOX),
                HoverSync::Restore(PointerPos { x: 109.0, y: 11.0 }),
            ),
            (
                "covered everywhere",
                Some(1),
                Some(2),
                200.0,
                Some(BOX),
                HoverSync::Clear,
            ),
            (
                "gone from layout",
                Some(1),
                Some(2),
                0.0,
                None,
                HoverSync::Clear,
            ),
            (
                "to nothing",
                Some(1),
                None,
                0.0,
                Some(BOX),
                HoverSync::Restore(PointerPos { x: 60.0, y: 30.0 }),
            ),
        ];
        for (name, before, after, covered_to, bounds, want) in cases {
            let got = decide(
                Shift { before, after },
                &known(),
                |_| bounds,
                world(covered_to),
            );
            assert_eq!(got, want, "{name}");
        }
    }

    #[test]
    fn without_a_move_to_feed_again_nothing_is_touched() {
        let got = decide(
            Shift {
                before: Some(1u32),
                after: Some(2),
            },
            &LastMove::Unknown,
            |_| panic!("no bounds asked"),
            |_| panic!("no hit asked"),
        );
        assert_eq!(got, HoverSync::Unchanged);
    }

    #[test]
    fn probes_are_the_centre_then_the_corners_one_px_inside() {
        let cases = [
            (
                "a card",
                BOX,
                [
                    (60.0, 30.0),
                    (11.0, 11.0),
                    (109.0, 11.0),
                    (11.0, 49.0),
                    (109.0, 49.0),
                ],
            ),
            (
                "a 1 px line collapses onto its middle",
                Bounds {
                    x: 0.0,
                    y: 5.0,
                    width: 1.0,
                    height: 10.0,
                },
                [
                    (0.5, 10.0),
                    (0.5, 6.0),
                    (0.5, 6.0),
                    (0.5, 14.0),
                    (0.5, 14.0),
                ],
            ),
        ];
        for (name, bounds, want) in cases {
            let got = probe_points(bounds).map(|p| (p.x, p.y));
            assert_eq!(got, want, "{name}");
        }
    }

    fn at(last: &LastMove) -> Option<(f32, f32, MouseEventButtons, MouseEventButton)> {
        match last {
            LastMove::Unknown => None,
            LastMove::At(e) => Some((e.client_x(), e.client_y(), e.buttons, e.button)),
        }
    }

    #[test]
    fn moves_presses_and_releases_are_remembered_as_moves_and_other_events_are_not() {
        let mods = Modifiers::empty();
        let [_, down] = pointer::press_events(
            AT,
            MouseEventButton::Secondary,
            MouseEventButtons::None,
            mods,
        );
        let up = pointer::release_event(
            AT,
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            mods,
        );
        let wheel = UiEvent::Wheel(blitz_traits::events::BlitzWheelEvent {
            delta: blitz_traits::events::BlitzWheelDelta::Lines(0.0, 1.0),
            coords: PointerCoords {
                page_x: 0.0,
                page_y: 0.0,
                screen_x: 0.0,
                screen_y: 0.0,
                client_x: 0.0,
                client_y: 0.0,
            },
            buttons: MouseEventButtons::None,
            mods,
            element: Default::default(),
        });
        let held = MouseEventButtons::Secondary;
        let main = MouseEventButton::Main;
        let cases = [
            (
                "a move",
                LastMove::Unknown,
                move_event(AT, held, mods),
                Some((50.0, 20.0, held, main)),
            ),
            (
                "a press",
                LastMove::Unknown,
                down,
                Some((50.0, 20.0, held, main)),
            ),
            (
                "a release",
                known(),
                up,
                Some((50.0, 20.0, MouseEventButtons::None, main)),
            ),
            (
                "a wheel keeps the record",
                known(),
                wheel.clone(),
                Some((50.0, 20.0, MouseEventButtons::None, main)),
            ),
            ("a wheel with no record", LastMove::Unknown, wheel, None),
        ];
        for (name, last, event, want) in cases {
            assert_eq!(at(&remember(last, &event)), want, "{name}");
        }
    }
}
