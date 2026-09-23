//! `blitz_traits::shell::ShellProvider` for a surface: how a document asks the host for a
//! redraw, a cursor, the IME, or the clipboard. Called from inside `resolve()` and event
//! handling, so it only records the request and pings the loop; the host acts at flush.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use blitz_traits::shell::{ClipboardError, ShellProvider};
use calloop::ping::Ping;
use cursor_icon::CursorIcon;

use crate::id::SurfaceId;
use crate::region::Rect;

/// The IME state a document last asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImeRequest {
    #[default]
    Disabled,
    /// Enabled, with the caret rectangle in surface-local logical pixels.
    Enabled { cursor_area: Rect },
}

/// A surface's shell provider.
#[derive(Debug)]
pub struct HostShellProvider {
    id: SurfaceId,
    wake: Ping,
    /// Set by `request_redraw`; the host clears it after presenting (SPIKE.md S1 finding 4).
    redraw: AtomicBool,
    cursor: Mutex<Option<CursorIcon>>,
    ime: Mutex<ImeRequest>,
    /// The text this process last copied, served without a round trip through the compositor.
    clipboard: Mutex<Option<String>>,
}

impl HostShellProvider {
    /// The provider for surface `id`, pinging `wake`.
    pub fn new(id: SurfaceId, wake: Ping) -> HostShellProvider {
        todo!()
    }

    /// Whether a redraw was requested since the last call; clears the flag.
    pub fn take_redraw(&self) -> RedrawRequest {
        todo!()
    }

    /// The cursor the document last asked for, if it changed since the last call.
    pub fn take_cursor(&self) -> CursorRequest {
        todo!()
    }

    /// The IME state the document last asked for.
    pub fn ime(&self) -> ImeRequest {
        todo!()
    }
}

/// Whether a document asked to be redrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawRequest {
    Requested,
    NotRequested,
}

/// A cursor change a document asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorRequest {
    Unchanged,
    /// Show this cursor; `None` hides it.
    Changed(Option<CursorIcon>),
}

impl ShellProvider for HostShellProvider {
    fn request_redraw(&self) {
        todo!()
    }

    fn set_cursor(&self, icon: Option<CursorIcon>) {
        todo!()
    }

    fn set_ime_enabled(&self, is_enabled: bool) {
        todo!()
    }

    fn set_ime_cursor_area(&self, x: f32, y: f32, width: f32, height: f32) {
        todo!()
    }

    /// The own-selection cache if this process owns the selection, else a pipe read from the
    /// selection offer with a 200 ms timeout.
    fn get_clipboard_text(&self) -> Result<String, ClipboardError> {
        todo!()
    }

    fn set_clipboard_text(&self, text: String) -> Result<(), ClipboardError> {
        todo!()
    }
}
