//! Hover: the shift a resolve causes and the repair that puts Blitz's dispatch back in step.

use blitz_dom::{BaseDocument, Document as _, NodeId};
use blitz_traits::events::UiEvent;

use super::{Polled, SurfaceDocument};
use crate::dom::hover_sync::{self, Bounds, HoverSync, LastMove, Shift};
use crate::input::pointer::PointerPos;

/// Whether a resolve moved the hover: the restyle it needs lands on a later frame, and the
/// resolve's own redraw request is dropped as the render's echo (`ctx::paint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverShift {
    Moved,
    Kept,
}

impl SurfaceDocument {
    /// The pointer left the surface: the leave move (`input::pointer::leave_event`) first, so
    /// Blitz's own hover change dispatches `mouseout`/`mouseleave` to the hovered chain and a
    /// component can see the hover end; then `clear_hover()` as a backstop for anything the
    /// move left hovered. `Polled::Changed` if something was hovered before.
    pub fn pointer_left(&mut self, mods: keyboard_types::Modifiers) -> Polled {
        let hovered = self.doc.inner().get_hover_node_id().is_some();
        self.doc
            .handle_ui_event(crate::input::pointer::leave_event(mods));
        self.clear_hover();
        if hovered {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
    }

    /// `clear_hover()`, and forget the pointer; `Polled::Changed` if something was hovered.
    pub fn clear_hover(&mut self) -> Polled {
        self.last_move = LastMove::Unknown;
        if self.doc.inner_mut().clear_hover() {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
    }

    /// The hovered element (a hovered text node counts as its element).
    pub(super) fn hovered(&self) -> Option<NodeId> {
        let inner = self.doc.inner();
        element_of(&inner, inner.get_hover_node_id()?)
    }

    /// Blitz's `refresh_hover` moved the hover without dispatching (its TODO): put the old
    /// hover back without dispatch, then feed the last move again, so the driver dispatches the
    /// leave/enter diff itself. At most once per resolve.
    pub(super) fn sync_hover(&mut self, shift: Shift<NodeId>) {
        let sync = {
            let inner = self.doc.inner();
            hover_sync::decide(
                shift,
                &self.last_move,
                |id| bounds_of(&inner, id),
                |at| element_at(&inner, at),
            )
        };
        match sync {
            HoverSync::Unchanged => return,
            HoverSync::Restore(at) => {
                self.doc.inner_mut().set_hover_to(at.x as f32, at.y as f32);
            }
            HoverSync::Clear => {
                self.doc.inner_mut().clear_hover();
            }
        }
        if let LastMove::At(event) = &self.last_move {
            let again = UiEvent::PointerMove(event.clone());
            let again = crate::input::pointer::scrolled(again, self.viewport_scroll());
            self.doc.handle_ui_event(again);
        }
    }
}

/// `id` if it is an element, else its nearest element ancestor: the node whose listeners a
/// hover change reaches.
fn element_of(doc: &BaseDocument, id: NodeId) -> Option<NodeId> {
    let node = doc.get_node(id)?;
    if node.is_element() {
        Some(id)
    } else {
        element_of(doc, node.parent?)
    }
}

/// The element Blitz would hover at `at`: the hit, lifted out of anonymous boxes as
/// `set_hover_to` lifts it, then to its element.
fn element_at(doc: &BaseDocument, at: PointerPos) -> Option<NodeId> {
    let hit = doc.hit(at.x as f32, at.y as f32)?;
    element_of(doc, doc.nearest_non_anonymous_ancestor(hit.node_id)?)
}

/// `id`'s current border box, in the page coordinates Blitz hit-tests (client plus the
/// viewport's scroll).
fn bounds_of(doc: &BaseDocument, id: NodeId) -> Option<Bounds> {
    let rect = doc.get_client_bounding_rect(id)?;
    let scroll = doc.viewport_scroll();
    Some(Bounds {
        x: rect.x + scroll.x,
        y: rect.y + scroll.y,
        width: rect.width,
        height: rect.height,
    })
}
