//! The overlay thumb (design/11 §11.3.12, §11.5.6): its geometry from a scroller's rect and
//! scroll metrics, and its show / hold / fade / expand timeline. Pure; `scroll::paint` draws
//! what this computes.

mod fade;
mod geometry;

pub(crate) use fade::{Indicator, Wake};
pub(crate) use geometry::{Metrics, ThumbPaint, in_zone};
