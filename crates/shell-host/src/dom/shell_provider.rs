//! `blitz_traits::shell::ShellProvider` for a surface: how a document asks the host for a
//! redraw, a cursor, the IME, or the clipboard. Called from inside `resolve()` and event
//! handling, so it only records the request and pings the loop; the host acts at flush.
//!
//! A redraw request is kept apart by where it came from ([`Origin`]). The host's own thread
//! asks while the host is resolving the very frame it is about to present; that request is an
//! echo of the render and is dropped after presenting, or the surface would never idle.
//! Any other thread is a load completing (an image, a stylesheet, a font, sent
//! to the document's channel by Blitz's net handlers): the frame being presented may have been
//! resolved before it arrived, so it must survive the present and dirty the surface again.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread::ThreadId;

use blitz_traits::shell::{ClipboardError, ShellProvider};
use calloop::ping::Ping;
use cursor_icon::CursorIcon;

use crate::clipboard::SharedSelection;
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

/// Which thread asked for a redraw, relative to the host's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The host's own thread: event handling, a poll, or the render itself.
    HostThread,
    /// Any other thread: a resource load that completed off the main thread.
    Elsewhere,
}

impl Origin {
    /// Where a request made on `current` came from, for a host running on `host`. Pure.
    pub fn of(host: ThreadId, current: ThreadId) -> Origin {
        if host == current {
            Origin::HostThread
        } else {
            Origin::Elsewhere
        }
    }
}

/// A surface's shell provider.
#[derive(Debug)]
pub struct HostShellProvider {
    id: SurfaceId,
    wake: Ping,
    /// The thread the host runs every document on: the one that built this provider.
    host_thread: ThreadId,
    /// Set by a `request_redraw` on the host's thread; the host takes it at flush and drops it
    /// after presenting.
    redraw: AtomicBool,
    /// Set by a `request_redraw` from any other thread; only the flush takes it, never the
    /// present, so a load that lands mid-render is painted by the next frame.
    arrived: AtomicBool,
    cursor: Mutex<Option<CursorIcon>>,
    /// Set whenever `set_cursor` is called, so `take_cursor` can report whether the cursor
    /// changed since the last take (mirrors `redraw`'s take/clear pattern).
    cursor_changed: AtomicBool,
    ime: Mutex<ImeRequest>,
    /// The selection's text as the seat last published it (`clipboard::SharedSelection`).
    selection: SharedSelection,
    /// Text the document copied, waiting for the flush to claim the selection with it.
    copied: Mutex<Option<String>>,
}

impl HostShellProvider {
    /// The provider for surface `id`, pinging `wake`, pasting from `selection`. Built on the
    /// host's thread, which is how it knows that thread.
    pub fn new(id: SurfaceId, wake: Ping, selection: SharedSelection) -> HostShellProvider {
        HostShellProvider {
            id,
            wake,
            host_thread: std::thread::current().id(),
            redraw: AtomicBool::new(false),
            arrived: AtomicBool::new(false),
            cursor: Mutex::new(None),
            cursor_changed: AtomicBool::new(false),
            ime: Mutex::new(ImeRequest::default()),
            selection,
            copied: Mutex::new(None),
        }
    }

    /// The surface this provider belongs to (for routing per-surface state such as
    /// `FrameStats` back to the surface that owns it).
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// Whether a redraw was requested, from any thread, since the last call; clears both
    /// flags. The flush's question.
    pub fn take_redraw(&self) -> RedrawRequest {
        let echoed = self.redraw.swap(false, Ordering::SeqCst);
        let arrived = self.arrived.swap(false, Ordering::SeqCst);
        requested(echoed || arrived)
    }

    /// Drop what the render just presented asked for itself (the host thread's flag), and
    /// report it. A request from another thread is left for the next flush.
    pub fn drop_echo(&self) -> RedrawRequest {
        requested(self.redraw.swap(false, Ordering::SeqCst))
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

    /// Text the document copied since the last call, for the host to claim the selection with.
    pub fn take_copied(&self) -> Option<String> {
        self.copied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
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

fn requested(flag: bool) -> RedrawRequest {
    if flag {
        RedrawRequest::Requested
    } else {
        RedrawRequest::NotRequested
    }
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
        let flag = match Origin::of(self.host_thread, std::thread::current().id()) {
            Origin::HostThread => &self.redraw,
            Origin::Elsewhere => &self.arrived,
        };
        flag.store(true, Ordering::SeqCst);
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
    /// The selection's text as last published; a paste never waits on another client.
    fn get_clipboard_text(&self) -> Result<String, ClipboardError> {
        self.selection.text().ok_or(ClipboardError)
    }

    /// Queue `text` for the flush, which claims the selection with it.
    fn set_clipboard_text(&self, text: String) -> Result<(), ClipboardError> {
        *self.copied.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);
        self.wake.ping();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn provider() -> (HostShellProvider, calloop::ping::PingSource) {
        let (ping, source) = calloop::ping::make_ping().expect("eventfd ping");
        let provider = HostShellProvider::new(SurfaceId(1), ping, SharedSelection::default());
        (provider, source)
    }

    #[test]
    fn origin_is_the_host_thread_only_on_the_host_thread() {
        let host = std::thread::current().id();
        let other = std::thread::spawn(|| std::thread::current().id())
            .join()
            .expect("thread");
        assert_eq!(Origin::of(host, host), Origin::HostThread);
        assert_eq!(Origin::of(host, other), Origin::Elsewhere);
    }

    #[test]
    fn the_present_drops_the_renders_own_request() {
        let (provider, _source) = provider();
        provider.request_redraw();
        assert_eq!(provider.drop_echo(), RedrawRequest::Requested);
        assert_eq!(provider.take_redraw(), RedrawRequest::NotRequested);
    }

    #[test]
    fn a_load_completing_on_another_thread_survives_the_present() {
        let (provider, _source) = provider();
        let provider = Arc::new(provider);
        let loader = Arc::clone(&provider);
        std::thread::spawn(move || loader.request_redraw())
            .join()
            .expect("loader thread");
        assert_eq!(provider.drop_echo(), RedrawRequest::NotRequested);
        assert_eq!(provider.take_redraw(), RedrawRequest::Requested);
        assert_eq!(
            provider.take_redraw(),
            RedrawRequest::NotRequested,
            "taken once"
        );
    }
}
