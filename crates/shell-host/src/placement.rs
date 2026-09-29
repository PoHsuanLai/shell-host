//! The two things a popup needs that `PopupConfig` does not carry: the anchor rectangle
//! (for `PopupAnchor::Rect`) and the content size (`PopupSize::Fixed`'s size, or
//! `FitContent`'s first guess before the popup's document is measured).
//!
//! See FINDINGS F4.

use crate::region::Rect;
use crate::scale::Size;

/// Where and how big a popup starts, in the parent surface's logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupPlacement {
    /// The anchor rectangle, surface-local to the parent. Used when the config's anchor is
    /// `PopupAnchor::Rect`; ignored for `PopupAnchor::Element`, whose rectangle is resolved
    /// from the parent's layout.
    pub anchor_rect: Rect,
    /// The popup's size: exact for `PopupSize::Fixed`; the initial size for
    /// `PopupSize::FitContent`, replaced (and repositioned) once the content is measured.
    pub size: Size,
}
