//! What a popup asks for: its anchor, size, edge, gravity, adjustment, grab and offset.

use crate::region::{ElementId, Px};

/// Where a popup is anchored: an explicit rect, or a DOM element resolved after layout.
///
/// Not `Copy`: `ElementId` owns a `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupAnchor {
    Rect,
    Element(ElementId),
}

/// How a popup's size is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupSize {
    /// Use the size passed to `positioner_params` as-is.
    Fixed,
    /// Clamp the passed-in size to `max` on each axis (content sizes itself, up to a cap).
    FitContent { max: Px },
}

/// Which point of the anchor rectangle the popup is positioned relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupEdge {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    BottomLeft,
    TopRight,
    BottomRight,
}

/// A single-axis toggle. Used instead of `bool` so `PopupAdjust`'s fields read as intent
/// (`slide_x: PopupToggle::On`) rather than an unlabelled `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupToggle {
    #[default]
    Off,
    On,
}

/// The `xdg_positioner` constraint-adjustment flags: how the compositor may move or resize the
/// popup if its unadjusted position would be (partly) off-screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PopupAdjust {
    pub slide_x: PopupToggle,
    pub slide_y: PopupToggle,
    pub flip_x: PopupToggle,
    pub flip_y: PopupToggle,
    pub resize_x: PopupToggle,
    pub resize_y: PopupToggle,
}

impl PopupAdjust {
    /// No adjustment: the popup stays exactly where positioned, even off-screen.
    pub const NONE: PopupAdjust = PopupAdjust {
        slide_x: PopupToggle::Off,
        slide_y: PopupToggle::Off,
        flip_x: PopupToggle::Off,
        flip_y: PopupToggle::Off,
        resize_x: PopupToggle::Off,
        resize_y: PopupToggle::Off,
    };

    /// Slide on both axes, flip on both axes, and never resize: the common "menu" behaviour.
    pub const SLIDE_AND_FLIP: PopupAdjust = PopupAdjust {
        slide_x: PopupToggle::On,
        slide_y: PopupToggle::On,
        flip_x: PopupToggle::On,
        flip_y: PopupToggle::On,
        resize_x: PopupToggle::Off,
        resize_y: PopupToggle::Off,
    };
}

/// Whether a popup takes an exclusive input grab (dismissing it, and any popups nested inside
/// it, when the compositor sees a click outside the stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupGrab {
    Ungrabbed,
    Grabbed,
}

/// A shift of the popup from where its anchor edge and gravity put it, in the parent's logical
/// pixels: design/13 §13.3.2 places a bar menu at (-8, +6) from its title button. Applied to
/// the anchor rectangle before it is sent, so it moves with the anchor and survives a flip
/// or a reposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PopupOffset {
    pub dx: Px,
    pub dy: Px,
}

impl PopupOffset {
    /// No shift: the popup sits exactly where its anchor edge and gravity put it.
    pub const ZERO: PopupOffset = PopupOffset { dx: 0, dy: 0 };
}

/// How a popup should be positioned and behave, independent of any particular anchor rect or
/// content size (those are resolved separately, after layout, and passed to
/// `positioner_params`).
///
/// Not `Copy`: `anchor` may own an `ElementId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupConfig {
    pub anchor: PopupAnchor,
    pub size: PopupSize,
    pub edge: PopupEdge,
    pub gravity: PopupEdge,
    pub adjust: PopupAdjust,
    pub grab: PopupGrab,
    /// Where the popup sits relative to its anchor; `PopupOffset::ZERO` for none.
    pub offset: PopupOffset,
}
