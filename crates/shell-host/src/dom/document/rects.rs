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
    /// has no such child.
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
        let rect = enclosing_rect(0.0, 0.0, bounds.width, bounds.height);
        Some(Size {
            width: rect.width,
            height: rect.height,
        })
    }
}
