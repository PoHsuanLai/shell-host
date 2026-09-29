//! Where things paint: an element's transformed rectangle and a popup document's content size.

use blitz_dom::{BaseDocument, Document as _, ElementData, NodeId};

use super::SurfaceDocument;
use crate::dom::hover_sync::Bounds;
use crate::dom::painted::{Affine2, Placed, Point2, painted_bounds};
use crate::geometry::enclosing_rect;
use crate::region::{ElementId, Rect};
use crate::scale::Size;

impl SurfaceDocument {
    /// Where the element `id` names paints, in surface-local logical pixels: its layout box
    /// (`get_client_bounding_rect`) carried through its own and its ancestors' CSS transforms
    /// (`dom::painted`), rounded outward (`geometry::enclosing_rect`); `None`
    /// before layout or when absent. What `RegionSpec::Element(s)` and `PopupAnchor::Element`
    /// resolve against, once per rendered frame, so a region follows a transform as it
    /// animates.
    pub fn element_rect(&self, id: ElementId) -> Option<Rect> {
        let inner = self.doc.inner();
        let node_id = inner.get_element_by_id(id.as_str())?;
        let laid_out = inner.get_client_bounding_rect(node_id)?;
        let bounds = painted_bounds(
            Bounds {
                x: laid_out.x,
                y: laid_out.y,
                width: laid_out.width,
                height: laid_out.height,
            },
            &transform_chain(&inner, node_id),
        );
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
    pub fn content_size(&self) -> Option<Size> {
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

/// The transformed boxes from `id` up its layout parents (the chain Blitz's paint composes),
/// innermost first, each at its untransformed border-box origin in client coordinates: the
/// origin `get_client_bounding_rect` gives, less the box's own scroll offset, which Blitz's
/// `unrounded_absolute_position` subtracts at every level including the box itself.
fn transform_chain(doc: &BaseDocument, id: NodeId) -> Vec<Placed> {
    let scale = doc.viewport().scale_f64();
    std::iter::successors(Some(id), |&at| doc.get_node(at)?.layout_parent.get())
        .filter_map(|at| {
            let node = doc.get_node(at)?;
            // `Node::transform` panics on a text or comment node; only elements carry one.
            if !node.is_element() {
                return None;
            }
            let transform = node.transform().as_deref()?.as_coeffs();
            let rect = doc.get_client_bounding_rect(at)?;
            let scroll = node.scroll_offset();
            Some(Placed {
                origin: Point2 {
                    x: rect.x + scroll.x,
                    y: rect.y + scroll.y,
                },
                transform: Affine2::from_device(transform, scale),
            })
        })
        .collect()
}

/// Whether `data` names a tag that `SurfaceDocument::content_size` should skip: never a
/// document's own content, always `display:none` by Blitz's default stylesheet.
fn is_content_element(data: &ElementData) -> bool {
    !matches!(
        &*data.name.local,
        "style" | "script" | "title" | "link" | "meta" | "base" | "template"
    )
}
