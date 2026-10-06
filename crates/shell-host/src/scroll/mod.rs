//! Host-side scroll physics for every Blitz document (design/11-BEHAVIOUR-scroll): the host
//! owns scrolling. Wheel, touchpad, palmrest and keyboard input reach this engine instead of
//! Blitz's default scroll action; the engine writes raw offsets each frame and paints its own
//! overlay thumb.
//!
//! The physics and the document side are `blitz_kit::scroll` (shared with every Blitz host).
//! Here, pure and table-tested: `config`, `indicator`, `source`, `suppress`. Touching Blitz or a
//! scene: `surface` (through `blitz_kit::scroll::driver`), `paint`, `watch` (the `use_scroll` hook).

pub(crate) mod config;
pub(crate) mod indicator;
pub(crate) mod paint;
pub(crate) mod source;
pub(crate) mod suppress;
pub(crate) mod surface;
pub(crate) mod tuning;
pub(crate) mod watch;

#[cfg(test)]
mod tests;
