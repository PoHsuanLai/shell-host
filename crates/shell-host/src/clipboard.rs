//! The clipboard (`wl_data_device`): copying serves our own text from a cache; pasting reads the
//! selection offer through a pipe, with a timeout so a stuck source cannot freeze the loop.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::time::Duration;

use smithay_client_toolkit::data_device_manager::data_offer::SelectionOffer;

use crate::error::Result;

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
}

impl Clipboard {
    /// Take ownership of the selection with `text` (`wl_data_device.set_selection`, needs the
    /// serial of a recent input event).
    pub fn copy(&mut self, text: String) {
        todo!()
    }

    /// The clipboard text: our own copy if we own the selection, else the offer read with
    /// [`PASTE_TIMEOUT`].
    pub fn paste(&self) -> Result<String> {
        todo!()
    }
}
