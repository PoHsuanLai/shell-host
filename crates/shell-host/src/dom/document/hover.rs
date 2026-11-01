//! Hover: the shift a resolve causes and the repair that puts Blitz's dispatch back in step.

use blitz_dom::{Document as _, NodeId};
use blitz_kit::hit::element_of;
use blitz_kit::hover::{LastMove, Shift, repair};

use super::SurfaceDocument;

impl SurfaceDocument {
    /// Feed one pointer move, and say whether Blitz's own state changed with it: the hover moved
    /// to another node. A move inside one element changes nothing a repaint could show; what an
    /// app's handlers change repaints through their own wake (`HostCtx::poll_woken`), and what
    /// Blitz restyles through its own redraw request (`HostShellProvider::take_redraw`).
    pub(crate) fn handle_move(&mut self, event: blitz_traits::events::UiEvent) -> bool {
        let before = self.doc.inner().get_hover_node_id();
        self.handle_event(event);
        self.doc.inner().get_hover_node_id() != before
    }

    /// The pointer left the surface: the leave move (`input::pointer::leave_event`) first, so
    /// Blitz's own hover change dispatches `mouseout`/`mouseleave` to the hovered chain and a
    /// component can see the hover end; then `clear_hover()` as a backstop for anything the
    /// move left hovered. `true` if something was hovered before.
    pub(crate) fn pointer_left(&mut self, mods: keyboard_types::Modifiers) -> bool {
        let hovered = self.doc.inner().get_hover_node_id().is_some();
        self.doc
            .handle_ui_event(crate::input::pointer::leave_event(mods));
        self.clear_hover();
        hovered
    }

    /// `clear_hover()`, and forget the pointer; `true` if something was hovered.
    pub(crate) fn clear_hover(&mut self) -> bool {
        self.last_move = LastMove::Unknown;
        self.doc.inner_mut().clear_hover()
    }

    /// The hovered element (a hovered text node counts as its element).
    pub(super) fn hovered(&self) -> Option<NodeId> {
        let inner = self.doc.inner();
        element_of(&inner, inner.get_hover_node_id()?)
    }

    /// Blitz's `refresh_hover` moved the hover without dispatching (its TODO): `blitz_kit::hover`
    /// puts the old hover back and feeds the last move again through this document's driver, so
    /// the leave/enter diff reaches the components. At most once per resolve.
    pub(super) fn sync_hover(&mut self, shift: Shift) {
        repair(&mut self.doc, &self.last_move, shift);
    }
}
