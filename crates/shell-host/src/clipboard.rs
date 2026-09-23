//! The clipboard (`wl_data_device`): copying serves our own text from a cache; pasting reads the
//! selection offer through a pipe, with a timeout so a stuck source cannot freeze the loop.

use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use smithay_client_toolkit::data_device_manager::ReadPipe;
use smithay_client_toolkit::data_device_manager::data_offer::SelectionOffer;
use smithay_client_toolkit::data_device_manager::data_source::CopyPasteSource;
use wayland_client::Connection;

use crate::error::{HostError, Result};

/// How long a paste waits for the selection owner to write.
pub const PASTE_TIMEOUT: Duration = Duration::from_millis(200);

/// The MIME types a text paste accepts, most preferred first.
pub const TEXT_MIME: &[&str] = &["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// The seat's clipboard.
#[derive(Debug, Default)]
pub struct Clipboard {
    /// The text we last copied, while we own the selection.
    pub(crate) own: Option<String>,
    /// The compositor's current selection offer, when someone else owns it.
    pub(crate) offer: Option<SelectionOffer>,
    /// The connection, so [`Clipboard::paste`] can flush a `wl_data_offer.receive` request
    /// before blocking on its pipe (sending a request only buffers it client-side; nothing
    /// reaches the compositor until a flush). Set once, the first time a handler sees a
    /// `Connection` (`handlers::seat::HostState::new_seat`); `None` only before that.
    pub(crate) conn: Option<Connection>,
    /// The data source backing our own selection, kept alive for as long as we hold it:
    /// `CopyPasteSource`'s own `Drop` sends `wl_data_source.destroy`, so dropping this the
    /// moment it is created (rather than storing it here) would hand the selection back before
    /// anyone could read it. Set by `handlers::data_device`'s claim helper, which is the only
    /// code with the queue handle and serial `Clipboard::copy` itself cannot take.
    pub(crate) source: Option<CopyPasteSource>,
}

impl Clipboard {
    /// Take ownership of the selection with `text`: updates the cache `paste` reads and that
    /// `handlers::data_device`'s `DataSourceHandler::send_request` serves to other clients.
    /// The wire request (`wl_data_device.set_selection`, which needs a queue handle and the
    /// serial of a recent input event, neither of which this method has) is made by whatever
    /// drains `Request::CopyText` and then calls the claim helper in `handlers::data_device`.
    pub fn copy(&mut self, text: String) {
        self.own = Some(text);
    }

    /// The clipboard text: our own copy if we own the selection, else the offer read with
    /// [`PASTE_TIMEOUT`].
    pub fn paste(&self) -> Result<String> {
        if let Some(text) = &self.own {
            return Ok(text.clone());
        }
        let offer = self.offer.as_ref().ok_or_else(|| {
            HostError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "no clipboard offer",
            ))
        })?;
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
        // The request above only queued `wl_data_offer.receive`; flush it now, or the source
        // never learns to write and every paste times out.
        if let Some(conn) = &self.conn {
            let _ = conn.flush();
        }
        read_within(pipe, PASTE_TIMEOUT)
    }
}

/// Read all of `pipe` on a helper thread, giving up after `timeout`. A dedicated thread (rather
/// than a raw poll on the fd) needs nothing beyond `std`, and `paste()` is already a blocking
/// call made from inside Blitz's own event handling, not from the calloop loop.
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
