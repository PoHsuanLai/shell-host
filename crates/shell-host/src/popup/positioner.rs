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
pub struct PositionerParams {
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
pub fn positioner_params(config: &PopupConfig, anchor: Rect, size: Size) -> PositionerParams {
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
pub fn refit(
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
    use crate::region::ElementId;

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

    #[test]
    fn positioner_params_ignores_which_kind_of_anchor_the_config_names() {
        // `config.anchor` says *what* the anchor rect was resolved from (an explicit rect or a
        // named element); `positioner_params` only ever sees the already
        // resolved `Rect`, so a `PopupAnchor::Element` config must behave exactly like a
        // `PopupAnchor::Rect` one here.
        let anchor = Rect {
            x: 10,
            y: 4,
            width: 80,
            height: 24,
        };
        let size = Size {
            width: 160,
            height: 200,
        };
        let mut by_rect = config(
            PopupEdge::Bottom,
            PopupEdge::Bottom,
            PopupAdjust::NONE,
            PopupSize::Fixed,
        );
        let mut by_element = by_rect.clone();
        by_element.anchor = PopupAnchor::Element(ElementId::new("dock-pill"));
        by_rect.anchor = PopupAnchor::Rect;
        assert_eq!(
            positioner_params(&by_rect, anchor, size),
            positioner_params(&by_element, anchor, size)
        );
    }

    #[test]
    fn fixed_size_passes_through_unchanged() {
        let cfg = config(
            PopupEdge::Bottom,
            PopupEdge::Bottom,
            PopupAdjust::NONE,
            PopupSize::Fixed,
        );
        let anchor = Rect {
            x: 10,
            y: 4,
            width: 80,
            height: 24,
        };
        let size = Size {
            width: 160,
            height: 200,
        };
        let params = positioner_params(&cfg, anchor, size);
        assert_eq!(params.size, size);
        assert_eq!(params.anchor_rect, anchor);
    }

    #[test]
    fn fit_content_clamps_to_max_on_each_axis() {
        let cfg = config(
            PopupEdge::Bottom,
            PopupEdge::Bottom,
            PopupAdjust::NONE,
            PopupSize::FitContent { max: 100 },
        );
        let anchor = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let params = positioner_params(
            &cfg,
            anchor,
            Size {
                width: 250,
                height: 40,
            },
        );
        assert_eq!(
            params.size,
            Size {
                width: 100,
                height: 40
            }
        );
    }

    #[test]
    fn fit_content_never_grows_a_size_already_under_max() {
        let cfg = config(
            PopupEdge::Bottom,
            PopupEdge::Bottom,
            PopupAdjust::NONE,
            PopupSize::FitContent { max: 500 },
        );
        let anchor = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let params = positioner_params(
            &cfg,
            anchor,
            Size {
                width: 30,
                height: 20,
            },
        );
        assert_eq!(
            params.size,
            Size {
                width: 30,
                height: 20
            }
        );
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
