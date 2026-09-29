//! How a drag ended, as its caller sees it, and the mapping from the source's last events.

use serde::{Deserialize, Serialize};
use wayland_client::protocol::wl_data_device_manager::DndAction;

/// How a drag ended (`SurfaceHandle::start_drag`'s answer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum DragOutcome {
    /// A target took the payload and finished with it (`wl_data_source.dnd_finished`). On
    /// `Move` the source deletes its own copy.
    Dropped { action: DragAction },
    /// Released over nothing that accepted it, or the compositor ended the drag
    /// (`wl_data_source.cancelled`).
    Cancelled,
    /// The drag never started.
    Unsupported { why: DragRefused },
}

/// What the target did with a dropped payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragAction {
    Copy,
    Move,
}

impl DragAction {
    /// The action a `wl_data_source.action` event chose; `None` for none, or for `ask` (the
    /// target settles an ask with a later `action` before it finishes).
    pub fn from_wire(action: DndAction) -> Option<DragAction> {
        if action == DndAction::Copy {
            Some(DragAction::Copy)
        } else if action == DndAction::Move {
            Some(DragAction::Move)
        } else {
            None
        }
    }
}

/// Why a drag did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum DragRefused {
    /// No pointer button is held: a drag starts from a press that is still down.
    #[error("no pointer button is held")]
    NoPress,
    /// The held button was pressed on another surface.
    #[error("the held button was pressed on another surface")]
    PressedElsewhere,
    /// The compositor offers no `wl_data_device_manager`, or the seat has no data device yet.
    #[error("no data device")]
    NoDataDevice,
    /// The compositor's `wl_data_device_manager` is older than v3, so the source would never
    /// learn how the drop ended.
    #[error("wl_data_device_manager v3 is needed")]
    OldProtocol,
    /// Another drag from this client has not ended yet.
    #[error("a drag is already in progress")]
    InProgress,
    /// The surface has closed.
    #[error("the surface is gone")]
    NoSurface,
}

/// The source's last event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DragEnd {
    /// `dnd_finished`: the target read what it wanted and is done.
    Finished,
    /// `cancelled`: no target took it, or the target gave up after the drop.
    Cancelled,
}

/// The outcome of a drag that ended with `end`, the last action the compositor chose being
/// `chosen`. A finish with no action chosen is a copy: the source must not delete anything
/// it was not told to move.
pub(crate) fn outcome(end: DragEnd, chosen: Option<DragAction>) -> DragOutcome {
    match end {
        DragEnd::Finished => DragOutcome::Dropped {
            action: chosen.unwrap_or(DragAction::Copy),
        },
        DragEnd::Cancelled => DragOutcome::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_end_and_the_chosen_action_make_the_outcome() {
        let dropped = |action| DragOutcome::Dropped { action };
        let cases = [
            (
                DragEnd::Finished,
                Some(DragAction::Copy),
                dropped(DragAction::Copy),
            ),
            (
                DragEnd::Finished,
                Some(DragAction::Move),
                dropped(DragAction::Move),
            ),
            (DragEnd::Finished, None, dropped(DragAction::Copy)),
            (
                DragEnd::Cancelled,
                Some(DragAction::Move),
                DragOutcome::Cancelled,
            ),
            (DragEnd::Cancelled, None, DragOutcome::Cancelled),
        ];
        for (end, chosen, want) in cases {
            assert_eq!(outcome(end, chosen), want, "{end:?} {chosen:?}");
        }
    }

    #[test]
    fn only_a_single_copy_or_move_is_an_action() {
        let cases = [
            (DndAction::Copy, Some(DragAction::Copy)),
            (DndAction::Move, Some(DragAction::Move)),
            (DndAction::Ask, None),
            (DndAction::empty(), None),
            (DndAction::Copy | DndAction::Move, None),
        ];
        for (wire, want) in cases {
            assert_eq!(DragAction::from_wire(wire), want, "{wire:?}");
        }
    }

    #[test]
    fn outcomes_serialize_adjacently_tagged() {
        let cases = [
            (
                DragOutcome::Dropped {
                    action: DragAction::Copy,
                },
                r#"{"kind":"dropped","v":{"action":"copy"}}"#,
            ),
            (DragOutcome::Cancelled, r#"{"kind":"cancelled"}"#),
            (
                DragOutcome::Unsupported {
                    why: DragRefused::NoPress,
                },
                r#"{"kind":"unsupported","v":{"why":"no_press"}}"#,
            ),
        ];
        for (outcome, json) in cases {
            assert_eq!(serde_json::to_string(&outcome).ok().as_deref(), Some(json));
        }
    }
}
