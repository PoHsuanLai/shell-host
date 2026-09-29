//! The `debug-inject` feature: synthetic input and offscreen captures on one surface, as a
//! typed API that returns its results ([`HostCtx::inject`]), and the stdin command source the
//! demos are driven by ([`stdin`], which maps each line onto that API and prints the result).
//! Frame statistics are not an injection: [`HostCtx::stats`] and [`HostCtx::take_stats`] are
//! the one source of them, with or without this feature.

pub mod stdin;

use std::path::{Path, PathBuf};

use blitz_traits::events::{MouseEventButton, MouseEventButtons, UiEvent};
use keyboard_types::Modifiers;

use crate::ctx::HostCtx;
use crate::debug::PointerButton;
use crate::dom::document::Polled;
use crate::error::{HostError, Result};
use crate::frame::FrameInput;
use crate::geometry::Point;
use crate::id::SurfaceId;
use crate::input::pointer::{
    ButtonChange, PointerPos, buttons_after, move_event, press_events, release_event,
};
use crate::modality::Stroke;
use crate::region::Px;
use crate::scale::Size;
use crate::state::HostState;

/// Something done to one surface as if the user had done it. Coordinates are the document's
/// logical pixels: surface-local, except that a client-decorated window's document starts
/// inside its resize band (`csd`), which injection never reaches.
///
/// `PointerDown`, `Pointer` and `PointerUp` script a drag through the same code path a real
/// `wl_pointer` press, motion and release do (`handlers/pointer.rs`, `HostState::deliver`):
/// a press, one or more `Pointer` moves while the button is still down (each
/// carrying the held button, as a real `Motion` would) and a release. `Click` is the
/// degenerate case that never moves between the two: a whole click, in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inject {
    /// The pointer moves to `at`, carrying whatever buttons injection currently has held (none,
    /// for a plain hover; a drag's, between a `PointerDown` and its `PointerUp`).
    Pointer { at: Point },
    /// `button` presses at `at`: the move-before-down pair a real press produces
    /// (`input::pointer::press_events`), and `button` joins the held set every later `Pointer`,
    /// `PointerUp` or `Click` sees.
    PointerDown { at: Point, button: PointerButton },
    /// `button` releases at `at`, dropped from the held set the same way.
    PointerUp { at: Point, button: PointerButton },
    /// The pointer moves to `at` and `button` is pressed and released there, with no move in
    /// between: `PointerDown` immediately followed by `PointerUp`.
    Click { at: Point, button: PointerButton },
    /// The pointer leaves the surface (hover clears); any buttons injection had held are
    /// forgotten, as a real leave means the compositor took the pointer
    /// (`HostState::forget_buttons`).
    Leave,
    /// Render the surface offscreen at its buffer size and write it to `path` as a PNG.
    Capture { path: PathBuf },
}

/// What an [`Inject`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Injected {
    /// The input was delivered to the document.
    Done,
    /// The capture was written: `path`, `size` in physical pixels (the buffer size).
    Captured { path: PathBuf, size: Size },
}

impl<M: 'static> HostCtx<M> {
    /// Deliver `inject` to surface `id`, through `HostState::deliver` — the same delivery a real
    /// `wl_pointer` press, motion or release uses (`handlers/pointer.rs`, `handlers/band.rs`):
    /// the document's cursor request is read back and the surface invalidated, exactly as for
    /// real input, so hover, the render loop and the resolve's hover repair all
    /// see an injected drag the way they would a real one. Input lands in the document at once
    /// and renders on the next flush; a capture renders now and writes the file before
    /// returning. `HostError::NoSurface` for an unknown id; a capture before the surface's first
    /// present is `HostError::Render`.
    pub fn inject(&mut self, id: SurfaceId, inject: Inject) -> Result<Injected> {
        if !self.state.surfaces.contains_key(&id) {
            return Err(HostError::NoSurface(id));
        }
        match inject {
            Inject::Pointer { at } => {
                let event = moved(at, self.state.input.buttons);
                self.state.deliver(id, [event]);
            }
            Inject::PointerDown { at, button } => {
                self.state.note_stroke(Stroke::PointerPress);
                let (events, held) = pressed(at, button, self.state.input.buttons);
                self.state.input.buttons = held;
                self.state.deliver(id, events);
            }
            Inject::PointerUp { at, button } => {
                let (event, held) = released(at, button, self.state.input.buttons);
                self.state.input.buttons = held;
                self.state.deliver(id, [event]);
            }
            Inject::Click { at, button } => {
                self.state.note_stroke(Stroke::PointerPress);
                let (down, held) = pressed(at, button, self.state.input.buttons);
                let (up, held) = released(at, button, held);
                self.state.input.buttons = held;
                self.state.deliver(id, down.into_iter().chain([up]));
            }
            Inject::Leave => leave(&mut self.state, id),
            Inject::Capture { path } => {
                let image = crate::ctx::capture(&mut self.state, id)?;
                return write_capture(&image, path);
            }
        }
        Ok(Injected::Done)
    }
}

/// Write `image` to `path` as a PNG: the second half of [`Inject::Capture`], apart so a
/// headless render can go through the same writer.
pub fn write_capture(image: &image::RgbaImage, path: PathBuf) -> Result<Injected> {
    save_png(image, &path)?;
    let px = |v: u32| Px::try_from(v).unwrap_or(Px::MAX);
    let size = Size {
        width: px(image.width()),
        height: px(image.height()),
    };
    Ok(Injected::Captured { path, size })
}

fn save_png(image: &image::RgbaImage, path: &Path) -> Result<()> {
    image
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| HostError::Io(std::io::Error::other(e.to_string())))
}

/// A motion to `at` carrying `held`: the same `PointerMove` a real `Motion` produces while
/// `held` buttons are down (`handlers/pointer.rs`'s `Motion` arm, through `HostState::
/// pointer_moved`'s document branch) — `MouseEventButtons::None` for a plain hover, or a drag's
/// buttons for a move injected between its `PointerDown` and `PointerUp`.
fn moved(at: Point, held: MouseEventButtons) -> UiEvent {
    move_event(pos(at), held, Modifiers::empty())
}

/// `button` presses at `at`, given the buttons `held` before it: the move-before-down pair a
/// real press produces (`input::pointer::press_events`, a press needs the hover target set
/// first), and the held set after (`input::pointer::buttons_after`, the same tracking
/// `handlers/pointer.rs`'s `Press` arm keeps in `SeatInput::buttons`).
fn pressed(
    at: Point,
    button: PointerButton,
    held: MouseEventButtons,
) -> ([UiEvent; 2], MouseEventButtons) {
    let mouse_button = dom_button(button);
    let events = press_events(pos(at), mouse_button, held, Modifiers::empty());
    let after = buttons_after(held, mouse_button, ButtonChange::Pressed);
    (events, after)
}

/// `button` releases at `at`, given the buttons `held` before it, and the held set after.
fn released(
    at: Point,
    button: PointerButton,
    held: MouseEventButtons,
) -> (UiEvent, MouseEventButtons) {
    let mouse_button = dom_button(button);
    let event = release_event(pos(at), mouse_button, held, Modifiers::empty());
    let after = buttons_after(held, mouse_button, ButtonChange::Released);
    (event, after)
}

/// The DOM button `PointerButton` maps to (`input::pointer::button_from_evdev` maps the other
/// direction, from a real `wl_pointer` press).
fn dom_button(button: PointerButton) -> MouseEventButton {
    match button {
        PointerButton::Primary => MouseEventButton::Main,
        PointerButton::Secondary => MouseEventButton::Secondary,
        PointerButton::Middle => MouseEventButton::Auxiliary,
    }
}

/// The pointer leaves, as `handlers/pointer.rs` does it (`SurfaceDocument::pointer_left`, then
/// forgetting any held buttons — `HostState::forget_buttons`'s reasoning: a real leave with a
/// button down means the compositor took the pointer, so nothing of ours is still held); an
/// invalidation when something was hovered.
fn leave(state: &mut HostState, id: SurfaceId) {
    state.input.buttons = MouseEventButtons::None;
    let left = state
        .surfaces
        .get_mut(&id)
        .map(|surface| surface.document.pointer_left(Modifiers::empty()));
    if left == Some(Polled::Changed) {
        state.frame_input(id, FrameInput::Invalidate);
    }
}

fn pos(point: Point) -> PointerPos {
    PointerPos {
        x: f64::from(point.x),
        y: f64::from(point.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON_CASES: &[(PointerButton, MouseEventButton, MouseEventButtons)] = &[
        (
            PointerButton::Primary,
            MouseEventButton::Main,
            MouseEventButtons::Primary,
        ),
        (
            PointerButton::Secondary,
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
        ),
        (
            PointerButton::Middle,
            MouseEventButton::Auxiliary,
            MouseEventButtons::Auxiliary,
        ),
    ];

    #[test]
    fn every_pointer_button_maps_to_its_dom_button() {
        for &(button, want, _) in BUTTON_CASES {
            assert_eq!(dom_button(button), want, "{button:?}");
        }
    }

    /// `pressed` is a move to `at` then a down of `button` there (matching
    /// `input::pointer::press_events`, a press needs the hover target set first), and reports
    /// `button` newly held.
    #[test]
    fn pressed_moves_then_presses_the_named_button() {
        let at = Point { x: 12, y: 4 };
        for &(button, dom, held) in BUTTON_CASES {
            let ([mv, down], after) = pressed(at, button, MouseEventButtons::None);
            let UiEvent::PointerMove(mv) = mv else {
                panic!("first event is not a move: {mv:?}");
            };
            let UiEvent::PointerDown(pd) = down else {
                panic!("second event is not a down: {down:?}");
            };
            assert_eq!(mv.buttons, MouseEventButtons::None, "{button:?}");
            assert_eq!(pd.button, dom, "{button:?}");
            assert_eq!(pd.buttons, held, "{button:?}");
            assert_eq!(after, held, "{button:?}");
        }
    }

    /// `released` drops `button` from the held set, and the `PointerUp` it emits carries the
    /// set left after (matching `input::pointer::release_event`).
    #[test]
    fn released_drops_the_named_button() {
        let at = Point { x: 12, y: 4 };
        for &(button, dom, held) in BUTTON_CASES {
            let (up, after) = released(at, button, held);
            let UiEvent::PointerUp(pu) = up else {
                panic!("not an up: {up:?}");
            };
            assert_eq!(pu.button, dom, "{button:?}");
            assert_eq!(pu.buttons, MouseEventButtons::None, "{button:?}");
            assert_eq!(after, MouseEventButtons::None, "{button:?}");
        }
    }

    /// A press, one or more held moves and a release script a drag through the same event
    /// shapes as the real seat path (`handlers/pointer.rs`): the move-before-down carries no
    /// buttons yet, a move fed between the press and the release carries the pressed button
    /// (a real `Motion` while it is down would too, `HostState::pointer_moved`), and the button
    /// is gone again after the release. This is what a scripted drag needs and a
    /// whole `Click` cannot give — a move at the point in between.
    #[test]
    fn a_drag_presses_moves_with_the_button_held_then_releases() {
        let start = Point { x: 12, y: 4 };
        let mid = Point { x: 20, y: 9 };
        let end = Point { x: 30, y: 15 };

        let ([_mv, down], held) = pressed(start, PointerButton::Primary, MouseEventButtons::None);
        let UiEvent::PointerDown(down) = down else {
            panic!("not a down");
        };
        assert_eq!(held, MouseEventButtons::Primary);
        assert_eq!(down.buttons, MouseEventButtons::Primary);

        let UiEvent::PointerMove(drag_move) = moved(mid, held) else {
            panic!("not a move");
        };
        assert_eq!(
            drag_move.buttons,
            MouseEventButtons::Primary,
            "a move mid-drag carries the held button, like a real Motion"
        );
        assert_eq!((drag_move.client_x(), drag_move.client_y()), (20.0, 9.0));

        let (up, held) = released(end, PointerButton::Primary, held);
        let UiEvent::PointerUp(up) = up else {
            panic!("not an up");
        };
        assert_eq!(held, MouseEventButtons::None);
        assert_eq!(up.buttons, MouseEventButtons::None);

        // The pointer is free again: a hover fed after the release carries no buttons.
        let UiEvent::PointerMove(after) = moved(end, held) else {
            panic!("not a move");
        };
        assert_eq!(after.buttons, MouseEventButtons::None);
    }
}
