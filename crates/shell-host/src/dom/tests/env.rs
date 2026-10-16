//! The one document environment every test builds on: no Wayland connection behind it, the
//! shell provider only records what it is asked, the font context discovers whatever fonts this
//! machine has, and the net provider serves `data:` and `file:` only.

use std::sync::Arc;

use blitz_kit::fonts::SharedFonts;
use blitz_kit::net::LocalNet;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::Prepaint;
use crate::clipboard::SharedSelection;
use crate::dom::document::DocumentEnv;
use crate::dom::shell_provider::HostShellProvider;

/// A `DocumentEnv` for a `width` x `height` px viewport at scale 1.
pub(crate) fn test_env(width: u32, height: u32) -> DocumentEnv {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    DocumentEnv {
        shell: Arc::new(HostShellProvider::new(ping, SharedSelection::default())),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet::default()),
        viewport: Viewport::new(width, height, 1.0, ColorScheme::Light),
        prepaint: Prepaint::None,
    }
}
