//! Hover: the shift a resolve causes and the repair that puts Blitz's dispatch back in step.

use blitz_dom::{Document as _, NodeId};
use blitz_kit::hit::element_of;
use blitz_kit::hover::{LastMove, Shift, repair};

use super::{Polled, SurfaceDocument};

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

    /// Blitz's `refresh_hover` moved the hover without dispatching (its TODO): `blitz_kit::hover`
    /// puts the old hover back and feeds the last move again through this document's driver, so
    /// the leave/enter diff reaches the components. At most once per resolve.
    pub(super) fn sync_hover(&mut self, shift: Shift<NodeId>) {
        repair(&mut self.doc, &self.last_move, shift);
    }
}
