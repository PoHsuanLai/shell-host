//! Turning a [`RegionSpec`] or a [`PopupAnchor`] into concrete geometry, after layout.
//!
//! Pure: the caller looks up element rectangles in the laid-out document and passes a lookup
//! function; building the `wl_region` from the result, and committing it to the compositor, are
//! the caller's effects (input regions via `rounded_strips`; blur regions the same way, through
//! `BackgroundEffect::set_blur_region`, double-buffered with the next commit).
//! [`Tracked`] is the other half of "the caller's effect": it remembers what was last sent, so a
//! surface whose element rectangle has not moved does not recommit on every frame (design/10
//! §10.3.12).

mod anchor;
mod spec;
mod tracked;

pub use anchor::resolve_popup_anchor;
pub use spec::{Resolved, resolve};
pub use tracked::{Change, Tracked};
