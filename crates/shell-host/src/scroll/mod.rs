//! Host-side scroll physics for every Blitz document (design/11-BEHAVIOUR-scroll): the host
//! owns scrolling. Wheel, touchpad, palmrest and keyboard input reach this engine instead of
//! Blitz's default scroll action; the engine writes raw offsets each frame and paints its own
//! overlay thumb.
//!
//! Pure and table-tested: `config`, `geom`, `momentum`, `velocity`, `rubber`, `smooth`, `keys`,
//! `latch`, `engine`, `animate`, `indicator`, `cmd`, `source`, `suppress`. Touching Blitz or
//! a scene: `surface` (through `dom::scroll`), `paint`, `watch` (the `use_scroll` hook).

pub(crate) mod animate;
pub mod cmd;
pub mod config;
pub mod engine;
pub mod geom;
pub mod indicator;
pub mod keys;
pub mod latch;
pub mod momentum;
pub mod paint;
pub mod rubber;
pub mod smooth;
pub mod source;
pub mod suppress;
pub mod surface;
pub mod tuning;
pub mod velocity;
pub mod watch;

#[cfg(test)]
mod tests;
