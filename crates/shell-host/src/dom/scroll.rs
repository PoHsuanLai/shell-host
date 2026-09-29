//! The scroll engine's view of a Blitz document: which scrollers are under a point, their
//! geometry, the raw offset writes, and the markers (`data-wheel`, `data-overscroll`,
//! `data-keys`). The only place the engine touches Blitz (design/11 §11.3.1, §11.7).
//!
//! Offsets are written raw: `set_viewport_scroll` for the viewport and `scroll_offset_mut` for
//! an element. Neither clamps, redraws, dispatches `scroll` or shows Blitz's scrollbars, and
//! nothing in `BaseDocument::resolve` clamps them back. Blitz's own scroll paths
//! (`scroll_chain_by` from a forwarded wheel, `scroll_to`, a fling) clamp and would undo a
//! stretch, so the host never forwards a wheel except to a `data-wheel="capture"` element.

use blitz_dom::{BaseDocument, Node, NodeId};

use crate::region::ElementId;
use crate::scroll::geom::{Axis, Elastic, Geom, Px, Scroller};
use crate::scroll::indicator::Area;
use crate::scroll::latch::Candidate;

/// Whether a wheel over a point goes to the engine or, raw, to a capturing element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelRoute {
    Engine,
    /// An element on the chain carries `data-wheel="capture"`.
    Capture,
}

/// What the keyboard focus means for scroll keys (design/11 §11.3.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFocus {
    /// Nothing focused (or the root or body, where Blitz parks focus): keys scroll the default
    /// target (the scroller under the pointer).
    Free,
    /// The focus sits in this scroller and nothing on its path handles keys: keys scroll it.
    Scroller(Scroller),
    /// Keys are the document's: an element on the focus path has a Dioxus key listener or
    /// `data-keys="capture"`, the focus is an editable or a form control, or nothing around the
    /// focus scrolls on the key's axis.
    Document,
}

/// The value of attribute `name` on `node`, if it is an element carrying it.
fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.element_data()?
        .attrs
        .iter()
        .find(|a| *a.name.local == *name)
        .map(|a| a.value.as_str())
}

/// `node` and its ancestors, innermost first.
fn ancestors(doc: &BaseDocument, id: NodeId) -> impl Iterator<Item = &Node> {
    std::iter::successors(doc.get_node(id), |node| {
        node.parent.and_then(|p| doc.get_node(p))
    })
}

/// The element under document point `(x, y)` (page coordinates: surface-local plus the
/// viewport's scroll), lifted out of text and anonymous boxes.
fn element_under(doc: &BaseDocument, x: f64, y: f64) -> Option<NodeId> {
    let hit = doc.hit(x as f32, y as f32)?;
    let id = doc.nearest_non_anonymous_ancestor(hit.node_id)?;
    ancestors(doc, id).find(|n| n.is_element()).map(|n| n.id)
}

/// Page coordinates of surface-local `(x, y)`.
pub fn page_point(doc: &BaseDocument, x: f64, y: f64) -> (f64, f64) {
    let scroll = doc.viewport_scroll();
    (x + scroll.x, y + scroll.y)
}

/// Whether `node`'s computed `overflow-x`/`-y` lets a user scroll it on `axis`.
fn scrolls(doc: &BaseDocument, node: &Node, axis: Axis) -> bool {
    let property = match axis {
        Axis::X => "overflow-x",
        Axis::Y => "overflow-y",
    };
    matches!(
        doc.resolved_style_value(node.id, property).as_str(),
        "auto" | "scroll"
    )
}

fn is_root(doc: &BaseDocument, id: NodeId) -> bool {
    doc.try_root_element().is_some_and(|root| root.id == id)
}

/// The scrollers on `axis` from the element under surface-local `(x, y)` outward, innermost
/// first, ending with the viewport when it can scroll or stretch on `axis`.
pub fn chain_at(doc: &BaseDocument, x: f64, y: f64, axis: Axis) -> Vec<Candidate> {
    let (px, py) = page_point(doc, x, y);
    let mut chain: Vec<Candidate> = element_under(doc, px, py)
        .into_iter()
        .flat_map(|id| ancestors(doc, id))
        .filter(|node| node.is_element() && !is_root(doc, node.id) && scrolls(doc, node, axis))
        .filter_map(|node| {
            let scroller = Scroller::Node(node.id.as_u64());
            geom(doc, scroller, axis).map(|geom| Candidate { scroller, geom })
        })
        .collect();
    if let Some(geom) = geom(doc, Scroller::Viewport, axis)
        && (geom.max.0 > 0.0 || geom.elastic == Elastic::Elastic)
    {
        chain.push(Candidate {
            scroller: Scroller::Viewport,
            geom,
        });
    }
    chain
}

/// Whether `node` opted into the rubber band (`data-overscroll="band"`).
fn banded(node: &Node) -> bool {
    attr(node, "data-overscroll") == Some("band")
}

/// An elastic axis (design/11 §11.3.7): opted in, and vertical or with content to scroll.
fn elastic(opted: bool, axis: Axis, max: f64) -> Elastic {
    match (opted, axis) {
        (true, Axis::Y) => Elastic::Elastic,
        (true, Axis::X) if max > 0.0 => Elastic::Elastic,
        _ => Elastic::Rigid,
    }
}

/// `scroller`'s geometry on `axis`; `None` for a node that is gone.
pub fn geom(doc: &BaseDocument, scroller: Scroller, axis: Axis) -> Option<Geom> {
    let pick = |x: f64, y: f64| match axis {
        Axis::X => x,
        Axis::Y => y,
    };
    match scroller {
        Scroller::Viewport => {
            let root = doc.try_root_element()?;
            let layout = root.final_layout();
            let (vw, vh) = doc.viewport().logical_size();
            let content_w = f64::from(layout.size.width.max(layout.scrollable_overflow_rect.right));
            let content_h = f64::from(
                layout
                    .size
                    .height
                    .max(layout.scrollable_overflow_rect.bottom),
            );
            let max = pick(content_w - f64::from(vw), content_h - f64::from(vh)).max(0.0);
            let scroll = doc.viewport_scroll();
            let opted = banded(root) || body(doc).is_some_and(banded);
            Some(Geom {
                offset: Px(pick(scroll.x, scroll.y)),
                max: Px(max),
                viewport: Px(pick(f64::from(vw), f64::from(vh))),
                elastic: elastic(opted, axis, max),
            })
        }
        Scroller::Node(id) => {
            let node = doc.get_node(NodeId::from_u64(id))?;
            let layout = node.final_layout();
            let max = match scrolls(doc, node, axis) {
                true => pick(
                    f64::from(layout.scroll_width()),
                    f64::from(layout.scroll_height()),
                ),
                false => 0.0,
            }
            .max(0.0);
            let offset = node.scroll_offset();
            Some(Geom {
                offset: Px(pick(offset.x, offset.y)),
                max: Px(max),
                viewport: Px(pick(
                    f64::from(layout.size.width),
                    f64::from(layout.size.height),
                )),
                elastic: elastic(banded(node), axis, max),
            })
        }
    }
}

fn body(doc: &BaseDocument) -> Option<&Node> {
    let root = doc.try_root_element()?;
    root.children
        .iter()
        .filter_map(|&id| doc.get_node(id))
        .find(|n| n.element_data().is_some_and(|e| &*e.name.local == "body"))
}

/// Write `raw` as `scroller`'s offset on `axis`, leaving the other axis alone.
pub fn write(doc: &mut BaseDocument, scroller: Scroller, axis: Axis, raw: f64) {
    let set = |point: &mut blitz_dom::Point<f64>| match axis {
        Axis::X => point.x = raw,
        Axis::Y => point.y = raw,
    };
    match scroller {
        Scroller::Viewport => {
            let mut scroll = doc.viewport_scroll();
            set(&mut scroll);
            doc.set_viewport_scroll(scroll);
        }
        Scroller::Node(id) => {
            if let Some(node) = doc.get_node_mut(NodeId::from_u64(id)) {
                set(node.scroll_offset_mut());
            }
        }
    }
}

/// Where a wheel at surface-local `(x, y)` goes.
pub fn wheel_route(doc: &BaseDocument, x: f64, y: f64) -> WheelRoute {
    let (px, py) = page_point(doc, x, y);
    let captured = element_under(doc, px, py)
        .into_iter()
        .flat_map(|id| ancestors(doc, id))
        .any(|node| attr(node, "data-wheel") == Some("capture"));
    match captured {
        true => WheelRoute::Capture,
        false => WheelRoute::Engine,
    }
}

/// The Dioxus key listeners: dioxus-native-dom records a listener as an attribute named for
/// the event (`keydown`) whose value is this placeholder (`MutationWriter::
/// create_event_listener`); Blitz cannot report a handler's `prevent_default`, so a listener on
/// the focus path is taken as handling keys.
const KEY_LISTENERS: [&str; 3] = ["keydown", "keyup", "keypress"];
const LISTENER_PLACEHOLDER: &str = "<rust func>";

/// Whether `node` carries a Dioxus key listener.
fn listens_for_keys(node: &Node) -> bool {
    KEY_LISTENERS
        .iter()
        .any(|name| attr(node, name) == Some(LISTENER_PLACEHOLDER))
}

/// What the focused element means for a scroll key on `axis`.
pub fn key_focus(doc: &BaseDocument, axis: Axis) -> KeyFocus {
    let Some(focus) = doc.get_focussed_node_id() else {
        return KeyFocus::Free;
    };
    // Blitz focuses the root element when nothing else is: that is "nothing focused" (the
    // body counts the same), unless a key listener sits there.
    let unfocused = is_root(doc, focus) || body(doc).is_some_and(|b| b.id == focus);
    if unfocused {
        return match ancestors(doc, focus).any(listens_for_keys) {
            true => KeyFocus::Document,
            false => KeyFocus::Free,
        };
    }
    let handled = ancestors(doc, focus).enumerate().any(|(depth, node)| {
        listens_for_keys(node)
            || attr(node, "data-keys") == Some("capture")
            || (depth == 0 && owns_keys(node))
    });
    if handled {
        return KeyFocus::Document;
    }
    let scroller = ancestors(doc, focus)
        .find(|n| n.is_element() && !is_root(doc, n.id) && scrolls(doc, n, axis))
        .map(|n| Scroller::Node(n.id.as_u64()))
        .or_else(|| {
            geom(doc, Scroller::Viewport, axis)
                .filter(|g| g.max.0 > 0.0)
                .map(|_| Scroller::Viewport)
        });
    scroller.map_or(KeyFocus::Document, KeyFocus::Scroller)
}

/// Whether a focused `node` takes arrow and space keys itself: text inputs, form controls,
/// buttons (Space presses them) and `contenteditable`.
fn owns_keys(node: &Node) -> bool {
    let Some(element) = node.element_data() else {
        return false;
    };
    element.text_input_data().is_some()
        || matches!(
            &*element.name.local,
            "input" | "textarea" | "select" | "button"
        )
        || attr(node, "contenteditable").is_some_and(|v| v != "false")
}

/// The scroller the element with `id` names, if it exists.
pub fn scroller_by_id(doc: &BaseDocument, id: &ElementId) -> Option<Scroller> {
    let node = doc.get_element_by_id(id.as_str())?;
    Some(match is_root(doc, node) {
        true => Scroller::Viewport,
        false => Scroller::Node(node.as_u64()),
    })
}

/// The nearest ancestor of the element with `id` that scrolls on `axis` (the viewport if none),
/// and the element's own border box in surface-local coordinates.
pub fn container_of(doc: &BaseDocument, id: &ElementId, axis: Axis) -> Option<(Scroller, Area)> {
    let node = doc.get_element_by_id(id.as_str())?;
    let area = area(doc, Scroller::Node(node.as_u64()))?;
    let container = ancestors(doc, node)
        .skip(1)
        .find(|n| n.is_element() && !is_root(doc, n.id) && scrolls(doc, n, axis))
        .map_or(Scroller::Viewport, |n| Scroller::Node(n.id.as_u64()));
    Some((container, area))
}

/// `scroller`'s border box in surface-local coordinates (the viewport: the whole surface).
/// Blitz's client rect of an element subtracts the element's own scroll offset along with its
/// ancestors' (`unrounded_absolute_position`), which moves a scrolled container's box by its
/// own scroll; that is added back here.
pub fn area(doc: &BaseDocument, scroller: Scroller) -> Option<Area> {
    match scroller {
        Scroller::Viewport => {
            let (w, h) = doc.viewport().logical_size();
            Some(Area {
                x: 0.0,
                y: 0.0,
                width: f64::from(w),
                height: f64::from(h),
            })
        }
        Scroller::Node(id) => {
            let id = NodeId::from_u64(id);
            let rect = doc.get_client_bounding_rect(id)?;
            let own = doc.get_node(id)?.scroll_offset();
            Some(Area {
                x: rect.x + own.x,
                y: rect.y + own.y,
                width: rect.width,
                height: rect.height,
            })
        }
    }
}
