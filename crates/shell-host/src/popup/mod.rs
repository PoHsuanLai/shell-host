//! xdg_popup positioning: the popup vocabulary, the pure translation from a `PopupConfig` to
//! the values an `xdg_positioner` is configured with, and the wire mapping.

mod config;
mod positioner;
mod wire;

pub use config::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
    PopupToggle,
};
pub use positioner::offset_rect;
pub(crate) use positioner::{PositionerParams, positioner_params, refit};
pub(crate) use wire::{to_wire_anchor, to_wire_constraint_adjustment, to_wire_gravity};

#[cfg(test)]
mod tests;
