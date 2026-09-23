//! The host's one `FontContext`: every document gets a clone, so the source cache is shared and
//! a face registered once is available on every surface.

use std::sync::Arc;

use blitz_dom::FontContext;
use parley::fontique::Blob;

/// The shared font context.
#[derive(Clone)]
pub struct SharedFonts {
    ctx: FontContext,
}

impl SharedFonts {
    /// System fonts (fontconfig, including CJK fallback) plus Blitz's bullet glyph.
    pub fn system() -> SharedFonts {
        let mut ctx = FontContext::new();
        // Shares the backing store across every clone handed to `for_document`, so a font file
        // loaded for one surface is not re-read from disk for the next.
        ctx.source_cache.make_shared();
        ctx.collection.register_fonts(
            Blob::new(Arc::new(blitz_dom::BULLET_FONT) as Arc<dyn AsRef<[u8]> + Send + Sync>),
            None,
        );
        SharedFonts { ctx }
    }

    /// Register a bundled face (quire's fonts, `ds::fonts`) for every document.
    pub fn register(&mut self, bytes: Vec<u8>) {
        self.ctx.collection.register_fonts(
            Blob::new(Arc::new(bytes) as Arc<dyn AsRef<[u8]> + Send + Sync>),
            None,
        );
    }

    /// A clone for a new document's `DocumentConfig::font_ctx`.
    pub fn for_document(&self) -> FontContext {
        self.ctx.clone()
    }
}

impl std::fmt::Debug for SharedFonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedFonts").finish_non_exhaustive()
    }
}
