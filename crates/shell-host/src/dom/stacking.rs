//! Re-hoisting z-indexed boxes after layout (FINDINGS F69).
//!
//! Blitz's `resolve` gathers each stacking context's z-indexed descendants in
//! `flush_styles_to_layout`, adding up their ancestors' `final_layout().location` and scroll
//! offsets, and only then runs `resolve_layout`. So the positions it records are the previous
//! layout's: a box whose ancestors moved in this resolve (a tile just inserted, whose location
//! was still zero) is painted and hit-tested where they used to be, until some later resolve
//! happens to run. An idle surface never runs one, so the stale frame stays on screen, while a
//! debug capture (which resolves again) draws it right. Gathering again after the layout reads
//! the fresh positions; the walk rebuilds only paint order and the hoisted lists, never style or
//! layout, so it is idempotent and cheap next to the layout itself.

use blitz_dom::{BaseDocument, NodeId};

/// Recompute every stacking context's hoisted children from `doc`'s current layout. A document
/// with no root element yet (nothing to hoist) is left alone.
pub(crate) fn rehoist(doc: &mut BaseDocument) {
    if let Some(root) = root_element(doc) {
        doc.flush_styles_to_layout(root);
    }
}

/// The document node's first element child (`<html>`), the root Blitz's own resolve walks from;
/// `BaseDocument::root_element` panics when there is none.
fn root_element(doc: &BaseDocument) -> Option<NodeId> {
    doc.root_node()
        .children
        .iter()
        .copied()
        .find(|&id| doc.get_node(id).is_some_and(|node| node.is_element()))
}
