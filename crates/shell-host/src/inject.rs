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
use crate::region::Px;
use crate::scale::Size;
use crate::state::HostState;

/// Something done to one surface as if the user had done it. Coordinates are surface-local
/// logical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inject {
    /// The pointer moves to `at` (hover follows).
    Pointer { at: Point },
    /// The pointer moves to `at` and `button` is pressed and released there.
    Click { at: Point, button: PointerButton },
    /// The pointer leaves the surface (hover clears).
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
    /// Deliver `inject` to surface `id`. Input lands in the document at once and renders on
    /// the next flush like real input; a capture renders now and writes the file before
    /// returning. `HostError::NoSurface` for an unknown id; a capture before the surface's
    /// first present is `HostError::Render`.
    pub fn inject(&mut self, id: SurfaceId, inject: Inject) -> Result<Injected> {
        if !self.state.surfaces.contains_key(&id) {
            return Err(HostError::NoSurface(id));
        }
        match inject {
            Inject::Pointer { at } => feed(&mut self.state, id, vec![pointer_moved(at)]),
            Inject::Click { at, button } => feed(&mut self.state, id, clicked(at, button).to_vec()),
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

fn pointer_moved(at: Point) -> UiEvent {
    move_event(pos(at), MouseEventButtons::None, Modifiers::empty())
}

fn clicked(at: Point, button: PointerButton) -> [UiEvent; 3] {
    let mods = Modifiers::empty();
    let mouse_button = dom_button(button);
    let [moved, down] = press_events(pos(at), mouse_button, MouseEventButtons::None, mods);
    let held = buttons_after(MouseEventButtons::None, mouse_button, ButtonChange::Pressed);
    let up = release_event(pos(at), mouse_button, held, mods);
    [moved, down, up]
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

/// The pointer leaves, as `handlers/pointer.rs` does it (`SurfaceDocument::pointer_left`); an
/// invalidation when something was hovered.
fn leave(state: &mut HostState, id: SurfaceId) {
    let left = state
        .surfaces
        .get_mut(&id)
        .map(|surface| surface.document.pointer_left(Modifiers::empty()));
    if left == Some(Polled::Changed) {
        state.frame_input(id, FrameInput::Invalidate);
    }
}

/// Feed `events` to surface `id`'s document. A changed hover asks for a redraw through the
/// shell provider; a Dioxus handler that ran wakes the document's waker; so neither needs an
/// invalidation here, and an event that changes nothing renders nothing.
fn feed(state: &mut HostState, id: SurfaceId, events: Vec<UiEvent>) {
    if let Some(surface) = state.surfaces.get_mut(&id) {
        for event in events {
            surface.document.handle_event(event);
        }
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

    /// `clicked` is a move to `at`, then a down and an up of `button` there, with `buttons`
    /// held only across the down/up pair (matching `input::pointer::press_events`).
    #[test]
    fn clicked_moves_then_presses_and_releases_the_named_button() {
        let at = Point { x: 12, y: 4 };
        for &(button, dom, held) in BUTTON_CASES {
            let [moved, down, up] = clicked(at, button);
            let UiEvent::PointerMove(mv) = moved else {
                panic!("first event is not a move: {moved:?}");
            };
            let UiEvent::PointerDown(pd) = down else {
                panic!("second event is not a down: {down:?}");
            };
            let UiEvent::PointerUp(pu) = up else {
                panic!("third event is not an up: {up:?}");
            };
            assert_eq!(mv.buttons, MouseEventButtons::None, "{button:?}");
            assert_eq!(pd.button, dom, "{button:?}");
            assert_eq!(pd.buttons, held, "{button:?}");
            assert_eq!(pu.button, dom, "{button:?}");
            assert_eq!(pu.buttons, MouseEventButtons::None, "{button:?}");
        }
    }
}
