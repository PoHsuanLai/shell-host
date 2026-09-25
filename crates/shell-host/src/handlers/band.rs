//! The pointer handler's half of a client-decorated window (`csd`): which zone the pointer is
//! in, the document's coordinates, the cursor to show, and the resize a press in the band
//! starts. Split from `pointer` so the event dispatch stays one screen long.

use blitz_traits::events::UiEvent;
use cursor_icon::CursorIcon;
use wayland_client::protocol::wl_pointer::WlPointer;

use crate::csd::{Zone, cursor_for, to_document, zone_at};
use crate::ctx::begin_resize;
use crate::dom::shell_provider::CursorRequest;
use crate::id::SurfaceId;
use crate::input::pointer::{self, PointerPos};
use crate::role::Role;
use crate::state::HostState;
use crate::toplevel::ResizeEdge;

impl HostState {
    /// The zone surface-local `pos` is in on surface `id` (always the document without a band).
    pub(crate) fn zone_of(&self, id: SurfaceId, pos: PointerPos) -> Zone {
        self.surfaces.get(&id).map_or(Zone::Document, |surface| {
            zone_at(surface.logical, surface.inset, pos)
        })
    }

    /// Surface-local `pos` in surface `id`'s document coordinates.
    pub(crate) fn document_pos(&self, id: SurfaceId, pos: PointerPos) -> PointerPos {
        self.surfaces.get(&id).map_or(pos, |surface| {
            to_document(pos, surface.inset, surface.scale)
        })
    }

    /// A motion to `pos` on surface `id`: re-decide the zone unless a button is held (a drag
    /// keeps the zone it started in), then hand the move to the document, or tell it the
    /// pointer left when the pointer has crossed into the band.
    pub(crate) fn pointer_moved(&mut self, id: SurfaceId, pos: PointerPos) {
        let held = self.input.buttons != blitz_traits::events::MouseEventButtons::None;
        let zone = if held {
            self.input.zone
        } else {
            self.zone_of(id, pos)
        };
        let was = std::mem::replace(&mut self.input.zone, zone);
        match zone {
            Zone::Document => {
                let at = self.document_pos(id, pos);
                let event = pointer::move_event(at, self.input.buttons, self.input.modifiers);
                self.deliver(id, [event]);
            }
            Zone::Edge(_) if was == Zone::Document => {
                if let Some(surface) = self.surfaces.get_mut(&id) {
                    surface.document.pointer_left(self.input.modifiers);
                }
                self.invalidate(id);
            }
            Zone::Edge(_) => {}
        }
    }

    /// Hand `events` to surface `id`'s document, record the cursor it asks for, and invalidate.
    pub(crate) fn deliver(&mut self, id: SurfaceId, events: impl IntoIterator<Item = UiEvent>) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        for event in events {
            surface.document.handle_event(event);
        }
        if let CursorRequest::Changed(icon) = surface.shell.take_cursor() {
            // `None` is Blitz's "nothing hovered" as often as CSS `cursor: none`; showing the
            // default is the safe reading of both.
            surface.doc_cursor = Some(icon.unwrap_or(CursorIcon::Default));
        }
        self.invalidate(id);
    }

    /// Show the cursor for the pointer's zone on surface `id`: the band's resize cursor, or
    /// the document's own. Needs the `enter` serial, so only for the focused surface.
    pub(crate) fn show_cursor(&mut self, pointer: &WlPointer, id: SurfaceId) {
        let Some((focus, serial)) = self.input.pointer_focus else {
            return;
        };
        if focus != id {
            return;
        }
        let icon = match self.input.zone {
            Zone::Edge(edge) => Some(cursor_for(edge)),
            Zone::Document => self
                .surfaces
                .get(&id)
                .map_or(Some(CursorIcon::Default), |surface| surface.doc_cursor),
        };
        self.cursor.set(pointer, serial, icon);
    }

    /// Drop the held buttons and the press. At an `enter` or a `leave` no button of ours can
    /// be down: a leave with a button held means the compositor took the pointer (a move or
    /// resize grab), and the release then goes to the compositor, never to us; keeping the
    /// buttons would pin the zone and let a later `begin_move` reuse a dead serial.
    pub(crate) fn forget_buttons(&mut self) {
        self.input.buttons = blitz_traits::events::MouseEventButtons::None;
        self.input.grab.update(|facts| facts.press = None);
    }

    /// A press in surface `id`'s band: resize from `edge` with the press just recorded.
    pub(crate) fn resize_from_band(&self, id: SurfaceId, edge: ResizeEdge) {
        let Some(surface) = self.surfaces.get(&id) else {
            return;
        };
        if let Role::Toplevel { window, .. } = &surface.role
            && let Err(refused) = begin_resize(self, id, window, edge)
        {
            eprintln!("shell-host: band resize of {id:?} refused: {refused}");
        }
    }
}
