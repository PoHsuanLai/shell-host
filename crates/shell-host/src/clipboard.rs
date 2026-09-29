//! The clipboard (`wl_data_device`): copying serves our own text from a cache; pasting answers
//! from the selection text read ahead of time.
//!
//! Blitz asks `ShellProvider::get_clipboard_text` synchronously, from inside its own event
//! handling on the main thread, which is also the thread that would have to serve the pipe if
//! the selection were ours. So nothing is read at paste time: every `wl_data_device.selection`
//! event starts a read of the new offer on a helper thread ([`PASTE_TIMEOUT`]), which publishes
//! the text into a [`SharedSelection`] every surface's shell provider holds. A paste in the few
//! milliseconds between a new selection and its read finishing sees the previous text.

use std::io;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use smithay_client_toolkit::data_device_manager::ReadPipe;
use smithay_client_toolkit::data_device_manager::data_offer::SelectionOffer;
use smithay_client_toolkit::data_device_manager::data_source::CopyPasteSource;
use wayland_client::Connection;

use crate::error::{HostError, Result};

/// How long reading a selection offer waits for its owner to write.
pub(crate) const PASTE_TIMEOUT: Duration = Duration::from_millis(200);

/// The MIME types a text paste accepts, most preferred first.
pub(crate) const TEXT_MIME: &[&str] = &["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// Which selection a read belongs to: bumped by every new selection, so a slow read of an old
/// offer cannot overwrite the text of a newer one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Generation(u64);

/// Whether a published read was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Publish {
    Kept,
    /// A newer selection began while this one was being read.
    Stale,
}

#[derive(Debug, Default)]
struct Published {
    generation: Generation,
    text: Option<String>,
}

/// The current selection's text, shared between the seat (which publishes it) and every
/// surface's shell provider (which reads it for a paste).
#[derive(Debug, Clone, Default)]
pub(crate) struct SharedSelection(Arc<Mutex<Published>>);

impl SharedSelection {
    /// A new selection replaced the old one: the generation a read of it must publish under.
    /// The previous text stays readable until that read publishes.
    pub(crate) fn begin(&self) -> Generation {
        let mut published = self.lock();
        published.generation = Generation(published.generation.0 + 1);
        published.generation
    }

    /// Publish the text read for `generation`; `None` when the selection holds no text (or the
    /// read failed). Ignored when a newer selection has begun since.
    pub(crate) fn publish(&self, generation: Generation, text: Option<String>) -> Publish {
        let mut published = self.lock();
        if published.generation != generation {
            return Publish::Stale;
        }
        published.text = text;
        Publish::Kept
    }

    /// The selection's text as last published.
    pub(crate) fn text(&self) -> Option<String> {
        self.lock().text.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Published> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The seat's clipboard.
#[derive(Debug, Default)]
pub(crate) struct Clipboard {
    /// The text we last copied, while we own the selection.
    pub(crate) own: Option<String>,
    /// The compositor's current selection offer.
    pub(crate) offer: Option<SelectionOffer>,
    /// The selection's text, as the shell providers read it.
    pub(crate) selection: SharedSelection,
    /// The connection, so [`Clipboard::follow`] can flush a `wl_data_offer.receive` request
    /// before its helper thread blocks on the pipe (sending a request only buffers it
    /// client-side). Set once, the first time a handler sees a `Connection`
    /// (`handlers::seat::HostState::new_seat`); `None` only before that.
    pub(crate) conn: Option<Connection>,
    /// The data source backing our own selection, kept alive for as long as we hold it:
    /// `CopyPasteSource`'s own `Drop` sends `wl_data_source.destroy`, so dropping this the
    /// moment it is created (rather than storing it here) would hand the selection back before
    /// anyone could read it. Set by `handlers::data_device`'s claim helper, which is the only
    /// code with the queue handle and serial `Clipboard::copy` itself cannot take.
    pub(crate) source: Option<CopyPasteSource>,
}

impl Clipboard {
    /// Take ownership of the selection with `text`: updates the cache that
    /// `handlers::data_device`'s `DataSourceHandler::send_request` serves to other clients, and
    /// publishes it at once, so a paste in this process sees it without a round trip. The wire
    /// request (`wl_data_device.set_selection`, which needs a queue handle and the serial of a
    /// recent input event) is made by `handlers::data_device::claim_selection`.
    pub(crate) fn copy(&mut self, text: String) {
        let generation = self.selection.begin();
        self.selection.publish(generation, Some(text.clone()));
        self.own = Some(text);
    }

    /// Follow a new selection (`wl_data_device.selection`): remember the offer and read its text
    /// on a helper thread, publishing it when the read finishes (or `None` when the offer has
    /// no text type, the read fails, or it times out). No offer means an empty clipboard.
    pub(crate) fn follow(&mut self, offer: Option<SelectionOffer>) {
        let generation = self.selection.begin();
        let pipe = offer
            .as_ref()
            .map(|offer| receive_text(offer, self.conn.as_ref()));
        self.offer = offer;
        match pipe {
            Some(Ok(pipe)) => {
                let selection = self.selection.clone();
                thread::spawn(move || {
                    let text = read_within(pipe, PASTE_TIMEOUT).ok();
                    selection.publish(generation, text);
                });
            }
            Some(Err(_)) | None => {
                self.selection.publish(generation, None);
            }
        }
    }
}

/// Ask `offer`'s owner to write its text into a pipe, and flush the request so it reaches the
/// compositor before anyone blocks on the pipe.
fn receive_text(offer: &SelectionOffer, conn: Option<&Connection>) -> Result<ReadPipe> {
    let mime = offer
        .with_mime_types(|types: &[String]| {
            TEXT_MIME
                .iter()
                .copied()
                .find(|want| types.iter().any(|t| t.as_str() == *want))
        })
        .ok_or_else(|| {
            HostError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "clipboard offer has no text mime type",
            ))
        })?;
    let pipe = offer
        .receive(mime.to_owned())
        .map_err(|err| HostError::Io(io::Error::other(err)))?;
    if let Some(conn) = conn {
        let _ = conn.flush();
    }
    Ok(pipe)
}

/// Read all of `pipe` on a second helper thread, giving up after `timeout`. A thread (rather
/// than a raw poll on the fd) needs nothing beyond `std`; on a timeout the reader is abandoned
/// and ends when the owner closes its end.
fn read_within(mut pipe: ReadPipe, timeout: Duration) -> Result<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        use io::Read;
        let mut buf = String::new();
        let result = pipe.read_to_string(&mut buf).map(|_| buf);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(err)) => Err(HostError::Io(err)),
        Err(_) => Err(HostError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "paste timed out waiting for the selection owner",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_read_is_what_a_paste_sees() {
        let seat = SharedSelection::default();
        let provider = seat.clone();
        assert_eq!(provider.text(), None, "nothing before the first selection");

        let first = seat.begin();
        assert_eq!(seat.publish(first, Some("one".into())), Publish::Kept);
        assert_eq!(provider.text().as_deref(), Some("one"));

        let second = seat.begin();
        assert_eq!(
            provider.text().as_deref(),
            Some("one"),
            "the old text stays until the new read lands"
        );
        assert_eq!(seat.publish(second, Some("two".into())), Publish::Kept);
        assert_eq!(provider.text().as_deref(), Some("two"));

        let third = seat.begin();
        assert_eq!(seat.publish(third, None), Publish::Kept, "an image, say");
        assert_eq!(provider.text(), None);
    }

    #[test]
    fn a_slow_read_of_an_old_selection_is_dropped() {
        let seat = SharedSelection::default();
        let old = seat.begin();
        let new = seat.begin();
        assert_eq!(seat.publish(new, Some("new".into())), Publish::Kept);
        assert_eq!(seat.publish(old, Some("old".into())), Publish::Stale);
        assert_eq!(seat.text().as_deref(), Some("new"));
    }

    #[test]
    fn copying_publishes_at_once() {
        let mut clipboard = Clipboard::default();
        let provider = clipboard.selection.clone();
        clipboard.copy("mine".into());
        assert_eq!(provider.text().as_deref(), Some("mine"));
    }
}
