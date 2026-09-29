//! xdg_popup positioning: the popup vocabulary, the pure translation from a `PopupConfig` to
//! the values an `xdg_positioner` is configured with, and the wire mapping.

mod config;
mod positioner;
mod wire;

pub use config::{
    Adjust, Edge, Grab, Gravity, Offset, PopupAnchor, PopupConfig, PopupSize, Toggle,
};
pub use positioner::{PositionerParams, offset_rect, positioner_params, refit};
pub use wire::{to_wire_anchor, to_wire_constraint_adjustment, to_wire_gravity};
