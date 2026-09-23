//! `blitz_traits::shell::ShellProvider` for a surface: how a document asks the host for a
//! redraw, a cursor, the IME, or the clipboard. Called from inside `resolve()` and event
//! handling, so it only records the request and pings the loop; the host acts at flush.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use blitz_traits::shell::{ClipboardError, ShellProvider};
use calloop::ping::Ping;
use cursor_icon::CursorIcon;

use crate::id::SurfaceId;
use crate::region::{Px, Rect};

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
    /// Set whenever `set_cursor` is called, so `take_cursor` can report whether the cursor
    /// changed since the last take (mirrors `redraw`'s take/clear pattern).
    cursor_changed: AtomicBool,
    ime: Mutex<ImeRequest>,
    /// The text this process last copied, served without a round trip through the compositor.
    clipboard: Mutex<Option<String>>,
}

impl HostShellProvider {
    /// The provider for surface `id`, pinging `wake`.
    pub fn new(id: SurfaceId, wake: Ping) -> HostShellProvider {
        HostShellProvider {
            id,
            wake,
            redraw: AtomicBool::new(false),
            cursor: Mutex::new(None),
            cursor_changed: AtomicBool::new(false),
            ime: Mutex::new(ImeRequest::default()),
            clipboard: Mutex::new(None),
        }
    }

    /// The surface this provider belongs to (for routing per-surface state such as
    /// `FrameStats` back to the surface that owns it).
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// Whether a redraw was requested since the last call; clears the flag.
    pub fn take_redraw(&self) -> RedrawRequest {
        if self.redraw.swap(false, Ordering::SeqCst) {
            RedrawRequest::Requested
        } else {
            RedrawRequest::NotRequested
        }
    }

    /// The cursor the document last asked for, if it changed since the last call.
    pub fn take_cursor(&self) -> CursorRequest {
        if self.cursor_changed.swap(false, Ordering::SeqCst) {
            let icon = *self.cursor.lock().unwrap_or_else(PoisonError::into_inner);
            CursorRequest::Changed(icon)
        } else {
            CursorRequest::Unchanged
        }
    }

    /// The IME state the document last asked for.
    pub fn ime(&self) -> ImeRequest {
        *self.ime.lock().unwrap_or_else(PoisonError::into_inner)
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
        self.redraw.store(true, Ordering::SeqCst);
        self.wake.ping();
    }

    fn set_cursor(&self, icon: Option<CursorIcon>) {
        *self.cursor.lock().unwrap_or_else(PoisonError::into_inner) = icon;
        self.cursor_changed.store(true, Ordering::SeqCst);
    }

    fn set_ime_enabled(&self, is_enabled: bool) {
        let mut ime = self.ime.lock().unwrap_or_else(PoisonError::into_inner);
        *ime = match (is_enabled, *ime) {
            (true, current @ ImeRequest::Enabled { .. }) => current,
            (true, ImeRequest::Disabled) => ImeRequest::Enabled {
                cursor_area: Rect::ZERO,
            },
            (false, _) => ImeRequest::Disabled,
        };
    }

    fn set_ime_cursor_area(&self, x: f32, y: f32, width: f32, height: f32) {
        let cursor_area = Rect {
            x: x.round() as Px,
            y: y.round() as Px,
            width: width.round() as Px,
            height: height.round() as Px,
        };
        *self.ime.lock().unwrap_or_else(PoisonError::into_inner) =
            ImeRequest::Enabled { cursor_area };
    }

    /// The own-selection cache if this process owns the selection, else a pipe read from the
    /// selection offer with a 200 ms timeout.
    fn get_clipboard_text(&self) -> Result<String, ClipboardError> {
        self.clipboard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(ClipboardError)
    }

    fn set_clipboard_text(&self, text: String) -> Result<(), ClipboardError> {
        *self
            .clipboard
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(text);
        Ok(())
    }
}
