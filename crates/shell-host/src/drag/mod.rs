//! Dragging out of a surface (`wl_data_source` + `wl_data_device.start_drag`): what a drag
//! carries, the mime types and bytes it offers, the press serial it starts from, the bytes a
//! pipe still owes, and how its end maps to an outcome. Pure; `ctx::drag` and
//! `handlers::drag_source` are the Wayland side.

pub(crate) mod offer;
pub(crate) mod outcome;
pub(crate) mod pipe;
pub(crate) mod serial;
pub(crate) mod spec;
