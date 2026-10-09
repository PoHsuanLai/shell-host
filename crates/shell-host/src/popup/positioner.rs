//! The pure translation from a `PopupConfig` plus a resolved anchor rect and content size into
//! the values an `xdg_positioner` is configured with, and the refit of a `FitContent` popup.
//!
//! Building and committing the actual `xdg_positioner` object is an effect (a Wayland request
//! against a live connection) and belongs to the caller; this module only computes what to send.

use super::config::{PopupAdjust, PopupEdge};
use super::config::{PopupConfig, PopupOffset, PopupSize};
use crate::placement::PopupPlacement;
use crate::region::Rect;
use crate::scale::Size;

/// The resolved values an `xdg_positioner` should be configured with: `set_size`,
/// `set_anchor_rect`, `set_anchor`, `set_gravity` and `set_constraint_adjustment`, all in one
/// value so a caller can log or test them without touching the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PositionerParams {
    pub size: Size,
    pub anchor_rect: Rect,
    pub anchor: PopupEdge,
    pub gravity: PopupEdge,
    pub adjust: PopupAdjust,
}

/// Combine a `PopupConfig` with a resolved anchor rect and a resolved content size into the
/// values to hand the `xdg_positioner`. `size` is the popup's natural size (already measured,
/// for `PopupSize::FitContent`, or already fixed, for `PopupSize::Fixed`); `FitContent` only
/// clamps it to its `max`. The config's `offset` translates the anchor rectangle.
pub(crate) fn positioner_params(
    config: &PopupConfig,
    anchor: Rect,
    size: Size,
) -> PositionerParams {
    let size = match config.size {
        PopupSize::Fixed => size,
        PopupSize::FitContent { max } => Size {
            width: size.width.min(max),
            height: size.height.min(max),
        },
    };
    PositionerParams {
        size,
        anchor_rect: offset_rect(anchor, config.offset),
        anchor: config.edge,
        gravity: config.gravity,
        adjust: config.adjust,
    }
}

/// `rect` moved by `offset`; its size is unchanged.
pub fn offset_rect(rect: Rect, offset: PopupOffset) -> Rect {
    Rect {
        x: rect.x + offset.dx,
        y: rect.y + offset.dy,
        ..rect
    }
}

/// Whether a `FitContent` popup's placement should change now that its document's `content`
/// size is known, and what to (`SurfaceOp::Reposition`, applied by `ctx::paint::render` right
/// after the layout that produced `content`). `None` for `PopupSize::Fixed` (nothing to fit),
/// or when the clamped content size already matches `current.size` — the check that stops a
/// settled popup from repositioning every frame.
pub(crate) fn refit(
    config: &PopupConfig,
    current: PopupPlacement,
    content: Size,
) -> Option<PopupPlacement> {
    let PopupSize::FitContent { max } = config.size else {
        return None;
    };
    let fitted = Size {
        width: content.width.min(max),
        height: content.height.min(max),
    };
    if fitted == current.size {
        None
    } else {
        Some(PopupPlacement {
            size: fitted,
            ..current
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::popup::config::{PopupAnchor, PopupGrab};

    fn config(
        edge: PopupEdge,
        gravity: PopupEdge,
        adjust: PopupAdjust,
        size: PopupSize,
    ) -> PopupConfig {
        PopupConfig {
            anchor: PopupAnchor::Rect,
            size,
            edge,
            gravity,
            adjust,
            grab: PopupGrab::Grabbed,
            offset: PopupOffset::ZERO,
        }
    }

    fn placement(size: Size) -> PopupPlacement {
        PopupPlacement {
            anchor_rect: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            size,
        }
    }

    // Table-tested: `refit` only ever changes a `FitContent` popup whose clamped content size
    // differs from its current placement.
    const REFIT_CASES: &[(&str, PopupSize, Size, Size, Option<Size>)] = &[
        (
            "Fixed never refits, however large the content",
            PopupSize::Fixed,
            Size {
                width: 100,
                height: 40,
            },
            Size {
                width: 999,
                height: 999,
            },
            None,
        ),
        (
            "content already matches the current placement: no change",
            PopupSize::FitContent { max: 200 },
            Size {
                width: 100,
                height: 40,
            },
            Size {
                width: 100,
                height: 40,
            },
            None,
        ),
        (
            "content grew: refit to the new size",
            PopupSize::FitContent { max: 200 },
            Size {
                width: 100,
                height: 40,
            },
            Size {
                width: 150,
                height: 60,
            },
            Some(Size {
                width: 150,
                height: 60,
            }),
        ),
        (
            "content past max: refit, clamped on each axis",
            PopupSize::FitContent { max: 200 },
            Size {
                width: 100,
                height: 40,
            },
            Size {
                width: 999,
                height: 999,
            },
            Some(Size {
                width: 200,
                height: 200,
            }),
        ),
    ];

    #[test]
    fn refit_only_changes_a_fit_content_popup_whose_measured_size_differs() {
        for &(name, popup_size, current_size, content, expected_size) in REFIT_CASES {
            let cfg = config(
                PopupEdge::Bottom,
                PopupEdge::Bottom,
                PopupAdjust::NONE,
                popup_size,
            );
            let current = placement(current_size);
            let got = refit(&cfg, current, content);
            let want = expected_size.map(|size| PopupPlacement { size, ..current });
            assert_eq!(got, want, "{name}");
        }
    }
}
