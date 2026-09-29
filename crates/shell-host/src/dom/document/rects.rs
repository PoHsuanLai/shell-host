//! Where things paint: an element's transformed rectangle and a popup document's content size.

use blitz_dom::Document as _;
use blitz_kit::hit::is_content_element;
use blitz_kit::paint_rect::painted_rect;

use super::SurfaceDocument;
use crate::geometry::enclosing_rect;
use crate::region::{ElementId, Rect};
use crate::scale::Size;

impl SurfaceDocument {
    /// Where the element `id` names paints, in surface-local logical pixels: its layout box
    /// (`get_client_bounding_rect`) carried through its own and its ancestors' CSS transforms
    /// (`blitz_kit::paint_rect`), rounded outward (`geometry::enclosing_rect`); `None`
    /// before layout or when absent. What `RegionSpec::Element(s)` and `PopupAnchor::Element`
    /// resolve against, once per rendered frame, so a region follows a transform as it
    /// animates.
    pub(crate) fn element_rect(&self, id: ElementId) -> Option<Rect> {
        let inner = self.doc.inner();
        let node_id = inner.get_element_by_id(id.as_str())?;
        let bounds = painted_rect(&inner, node_id)?;
        Some(enclosing_rect(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ))
    }

    /// A popup document's content size, in logical px, for `PopupSize::FitContent`
    /// (`popup::refit`). Neither `<body>` nor `dioxus-native-dom`'s own `<main id="main">` (the
    /// fixed mount point every `DioxusDocument` renders its `VirtualDom` into,
    /// `dioxus_native_dom::DioxusDocument::new`) is it: a block's default `width:auto` fills
    /// its container, so both stretch to the surface's width regardless of content, which is
    /// exactly the size `FitContent` needs to *not* read (`<body>`'s `height:100%`, from this
    /// crate's own `UA_STYLESHEET`, does the same on the other axis). The measured element is
    /// `#main`'s first content child: `<style>`, `<script>`, `<title>`, `<link>`, `<meta>`,
    /// `<base>` and `<template>` are skipped (Blitz's own default stylesheet already hides them,
    /// `display:none`, so their box carries no useful size, and a document's own `<style>`
    /// tends to render as the first child of its root). `None` before the first layout, when
    /// `#main` is missing (a caller contract: `dioxus-native-dom` always creates it), or when it
    /// has no such child. The size is then widened to the far edges of every absolutely or fixed
    /// positioned element beneath `#main` (`positioned_extents`), so a menu or popover drawn as
    /// an overlay, which is out of flow and gives its parent no size, is measured too.
    pub(crate) fn content_size(&self) -> Option<Size> {
        let inner = self.doc.inner();
        let main = inner.get_element_by_id("main")?;
        let children = &inner.get_node(main)?.children;
        let content = children.iter().copied().find(|&child| {
            inner
                .get_node(child)
                .and_then(|node| node.element_data())
                .is_some_and(is_content_element)
        })?;
        let bounds = inner.get_client_bounding_rect(content)?;
        let (right, bottom) = children
            .iter()
            .flat_map(|&child| positioned_extents(&inner, child))
            .fold((bounds.width, bounds.height), |(w, h), (r, b)| {
                (w.max(r), h.max(b))
            });
        let rect = enclosing_rect(0.0, 0.0, right, bottom);
        Some(Size {
            width: rect.width,
            height: rect.height,
        })
    }
}

/// The far edges `(right, bottom)`, from the surface origin, of the layout box of every
/// absolutely (or fixed) positioned element in `id`'s subtree, `id` included. Such a box is out
/// of flow, so it adds nothing to its parent's size; measuring only in-flow content sees a
/// menu drawn as an overlay as 1x1. In-flow boxes are left out on purpose: `<body>`'s margin
/// offsets them, and their parent's own size already contains them. The layout box, not the
/// transformed one, so an appear animation's scale does not move the measurement.
fn positioned_extents(doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId) -> Vec<(f64, f64)> {
    let Some(node) = doc.get_node(id).filter(|node| node.is_element()) else {
        return Vec::new();
    };
    let own = node
        .primary_styles()
        .is_some_and(|style| style.get_box().position.is_absolutely_positioned())
        .then(|| doc.get_client_bounding_rect(id))
        .flatten()
        .map(|rect| (rect.x + rect.width, rect.y + rect.height));
    own.into_iter()
        .chain(
            node.children
                .iter()
                .flat_map(|&child| positioned_extents(doc, child)),
        )
        .collect()
}
