//! Host-side scroll physics for every Blitz document (design/11-BEHAVIOUR-scroll): the host
//! owns scrolling. Wheel, touchpad, palmrest and keyboard input reach this engine instead of
//! Blitz's default scroll action; the engine writes raw offsets each frame and paints its own
//! overlay thumb.
//!
//! Pure and table-tested: `config`, `geom`, `momentum`, `velocity`, `rubber`, `smooth`, `keys`,
//! `latch`, `engine`, `animate`, `indicator`, `cmd`, `source`, `suppress`. Touching Blitz or
//! a scene: `surface` (through `dom::scroll`), `paint`, `watch` (the `use_scroll` hook).

pub(crate) mod animate;
pub(crate) mod cmd;
pub(crate) mod config;
pub(crate) mod engine;
pub(crate) mod geom;
pub(crate) mod indicator;
pub(crate) mod keys;
pub(crate) mod latch;
pub(crate) mod momentum;
pub(crate) mod paint;
pub(crate) mod rubber;
pub(crate) mod smooth;
pub(crate) mod source;
pub(crate) mod suppress;
pub(crate) mod surface;
pub(crate) mod tuning;
pub(crate) mod velocity;
pub(crate) mod watch;

#[cfg(test)]
mod tests;
