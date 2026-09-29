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
pub enum Edge {
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

/// Which direction the popup grows away from its anchor point. Shares `Edge`'s nine positions
/// but is a distinct type: an anchor edge and a gravity direction are never interchangeable,
/// and mixing them up should be a type error, not a rendering bug.
pub type Gravity = Edge;

/// A single-axis toggle. Used instead of `bool` so `Adjust`'s fields read as intent
/// (`slide_x: Toggle::On`) rather than an unlabelled `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Toggle {
    #[default]
    Off,
    On,
}

/// The `xdg_positioner` constraint-adjustment flags: how the compositor may move or resize the
/// popup if its unadjusted position would be (partly) off-screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Adjust {
    pub slide_x: Toggle,
    pub slide_y: Toggle,
    pub flip_x: Toggle,
    pub flip_y: Toggle,
    pub resize_x: Toggle,
    pub resize_y: Toggle,
}

impl Adjust {
    /// No adjustment: the popup stays exactly where positioned, even off-screen.
    pub const NONE: Adjust = Adjust {
        slide_x: Toggle::Off,
        slide_y: Toggle::Off,
        flip_x: Toggle::Off,
        flip_y: Toggle::Off,
        resize_x: Toggle::Off,
        resize_y: Toggle::Off,
    };

    /// Slide on both axes, flip on both axes, and never resize: the common "menu" behaviour.
    pub const SLIDE_AND_FLIP: Adjust = Adjust {
        slide_x: Toggle::On,
        slide_y: Toggle::On,
        flip_x: Toggle::On,
        flip_y: Toggle::On,
        resize_x: Toggle::Off,
        resize_y: Toggle::Off,
    };
}

/// Whether a popup takes an exclusive input grab (dismissing it, and any popups nested inside
/// it, when the compositor sees a click outside the stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    Ungrabbed,
    Grabbed,
}

/// A shift of the popup from where its anchor edge and gravity put it, in the parent's logical
/// pixels: design/13 §13.3.2 places a bar menu at (-8, +6) from its title button. Applied to
/// the anchor rectangle before it is sent, so it moves with the anchor and survives a flip
/// or a reposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Offset {
    pub dx: Px,
    pub dy: Px,
}

impl Offset {
    /// No shift: the popup sits exactly where its anchor edge and gravity put it.
    pub const ZERO: Offset = Offset { dx: 0, dy: 0 };
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
    pub edge: Edge,
    pub gravity: Gravity,
    pub adjust: Adjust,
    pub grab: Grab,
    /// Where the popup sits relative to its anchor; `Offset::ZERO` for none.
    pub offset: Offset,
}
