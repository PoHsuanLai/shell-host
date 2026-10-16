//! Our drag's `wl_data_source` events (`handlers::data_device` routes them here when the
//! source is the drag's): a target's `send` becomes a pipe write for the loop, `action`
//! records the compositor's choice, and `dnd_finished` or `cancelled` ends the drag and answers
//! `SurfaceHandle::start_drag`.

use smithay_client_toolkit::data_device_manager::WritePipe;
use smithay_client_toolkit::data_device_manager::data_source::DragSource;
use tokio::sync::oneshot;
use wayland_client::protocol::wl_data_device_manager::DndAction;

use crate::drag::offer::{Offered, bytes_for};
use crate::drag::outcome::{DragAction, DragEnd, DragOutcome, outcome};
use crate::handlers::pipe_writes::Queued;
use crate::id::SurfaceId;
use crate::state::HostState;

/// The seat's drag, if one is under way.
#[derive(Debug, Default)]
pub(crate) struct Dragging {
    pub(crate) active: Option<ActiveDrag>,
}

/// A drag the compositor is running.
#[derive(Debug)]
pub(crate) struct ActiveDrag {
    /// Kept alive until the drag ends: dropping it destroys the source, which cancels the drag.
    pub(crate) source: DragSource,
    pub(crate) offered: Vec<Offered>,
    pub(crate) reply: oneshot::Sender<DragOutcome>,
    /// The compositor's latest `action` choice.
    pub(crate) chosen: Option<DragAction>,
    /// The icon surface, closed with the drag.
    pub(crate) icon: Option<SurfaceId>,
}

impl HostState {
    /// A target asked for the payload as `mime`: queue the write. A type we never offered gets
    /// the pipe closed at once (an empty read), which is all a source can say.
    pub(crate) fn drag_send(&mut self, mime: &str, pipe: WritePipe) {
        let bytes = self
            .drag
            .active
            .as_ref()
            .and_then(|drag| bytes_for(&drag.offered, mime));
        match bytes {
            Some(bytes) => {
                if self.pipe_writes.push(pipe, bytes) == Queued::Refused {
                    eprintln!("shell-host: too many drag pipes open; one closed unanswered");
                }
            }
            None => drop(pipe),
        }
    }

    /// The compositor chose `action` (it may choose again as the pointer moves).
    pub(crate) fn drag_action(&mut self, action: DndAction) {
        if let Some(drag) = self.drag.active.as_mut() {
            drag.chosen = DragAction::from_wire(action);
        }
    }

    /// The drag is over: answer the caller, destroy the source and close the icon.
    pub(crate) fn drag_end(&mut self, end: DragEnd) {
        let Some(drag) = self.drag.active.take() else {
            return;
        };
        // The caller may have stopped waiting; nobody is left to tell then.
        let _ = drag.reply.send(outcome(end, drag.chosen));
        if let Some(icon) = drag.icon {
            self.remove_surface(icon);
        }
    }
}
