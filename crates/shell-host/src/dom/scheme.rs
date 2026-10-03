//! The viewport's colour scheme follows the appearance the document's root resolved, and a
//! switch repaints the glyphs and text in the new ink.
//!
//! A root that carries the design system's appearance writes it as `data-theme` on the outermost
//! `.ds`. The host copies it to the viewport, so the canvas, the user-agent sheet,
//! `prefers-color-scheme` and the scroll thumb agree with the root.
//!
//! A colour-only restyle leaves the boxes as they are: Blitz bakes a text run's colour into its
//! inline layout and an `<svg>`'s `currentColor` into its parsed tree when the box is built, and a
//! `color` change builds neither again (after a switch to Dark the glyphs and text kept their
//! light-scheme colours). So a switch also turns incremental layout off, which makes every resolve
//! rebuild every box, and it stays off while a transition is still running (a box built
//! mid-transition bakes the colour it was built at, not the one the transition ends on). The first
//! `follow_root` after the last transition has ended turns it back on.
//!
//! This is the rule of quire's `ds_blitz::scheme::follow_root`, written against `blitz-dom`
//! because the host stands below the design system and may not depend on it (ARCHITECTURE
//! section 1).

use blitz_dom::{BaseDocument, LocalName};
use blitz_traits::shell::ColorScheme;

/// The scheme the outermost `.ds` resolved, if one is mounted.
pub(crate) fn resolved(doc: &BaseDocument) -> Option<ColorScheme> {
    // Selected by class: an unprefixed attribute selector never matches under Blitz.
    let root = doc.query_selector(".ds").ok().flatten()?;
    let theme = doc.get_node(root)?.attr(LocalName::from("data-theme"))?;
    Some(match theme {
        "dark" => ColorScheme::Dark,
        _ => ColorScheme::Light,
    })
}

/// Point the viewport at `scheme` and make the next resolves rebuild every box.
pub(crate) fn switch_to(doc: &mut BaseDocument, scheme: ColorScheme) {
    doc.viewport_mut().color_scheme = scheme;
    doc.set_incremental_layout(false);
}

/// Follow the root's scheme; the scheme it changed to, when it changed. Called once per frame,
/// before the resolve.
pub(crate) fn follow_root(doc: &mut BaseDocument) -> Option<ColorScheme> {
    let wanted = resolved(doc)?;
    if doc.viewport().color_scheme == wanted {
        if !doc.incremental_layout() && !doc.is_animating() {
            doc.set_incremental_layout(true);
        }
        return None;
    }
    switch_to(doc, wanted);
    Some(wanted)
}
