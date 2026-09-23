//! The host's one `FontContext`: every document gets a clone, so the source cache is shared and
//! a face registered once is available on every surface.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use blitz_dom::FontContext;

/// The shared font context.
#[derive(Clone)]
pub struct SharedFonts {
    ctx: FontContext,
}

impl SharedFonts {
    /// System fonts (fontconfig, including CJK fallback) plus Blitz's bullet glyph.
    pub fn system() -> SharedFonts {
        todo!()
    }

    /// Register a bundled face (quire's fonts, `ds::fonts`) for every document.
    pub fn register(&mut self, bytes: Vec<u8>) {
        todo!()
    }

    /// A clone for a new document's `DocumentConfig::font_ctx`.
    pub fn for_document(&self) -> FontContext {
        todo!()
    }
}

impl std::fmt::Debug for SharedFonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedFonts").finish_non_exhaustive()
    }
}
