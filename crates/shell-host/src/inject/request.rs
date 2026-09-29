//! What can be injected, and what an injection returns.

use std::path::PathBuf;

use keyboard_types::{Code, Key, Modifiers};

use crate::geometry::Point;
use crate::scale::Size;

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
    /// A key does `action`. It enters at the point a real xkb-translated key does
    /// (`HostState::key_down` and its release twin, reached from `handlers/keyboard.rs`), so
    /// focus, modality, the scroll keys and the IME see it as they see a real key. It starts no
    /// key repeat.
    Key {
        stroke: KeyStroke,
        action: KeyAction,
    },
    /// Render the surface offscreen at its buffer size and write it to `path` as a PNG.
    Capture { path: PathBuf },
}

/// A pointer button, as `Inject::Click` and `Inject::Click` name it: the Linux evdev codes
/// `input::pointer::button_from_evdev` maps to a DOM button. `Primary` is `BTN_LEFT` (272),
/// `Secondary` is `BTN_RIGHT` (273), `Middle` is `BTN_MIDDLE` (274).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerButton {
    #[default]
    Primary,
    Secondary,
    Middle,
}

/// A key as the DOM names it: the logical `key`, the physical `code` and the held `mods`
/// (`Ctrl+K` is `Key::Character("k")`, `Code::KeyK`, `Modifiers::CONTROL`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: Key,
    pub code: Code,
    pub mods: Modifiers,
}

/// What a key does. `Tap` is a `Press` immediately followed by a `Release`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Release,
    Tap,
}

/// What an [`Inject`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Injected {
    /// The input was delivered to the document.
    Done,
    /// The capture was written: `path`, `size` in physical pixels (the buffer size).
    Captured { path: PathBuf, size: Size },
}
