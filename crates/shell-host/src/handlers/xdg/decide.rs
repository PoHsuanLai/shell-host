//! The decisions behind the xdg configure handlers, as values a table can assert on.

use std::num::NonZeroU32;

use crate::csd::{Inset, inset_for, surface_of, window_geometry};
use crate::layer::KeyboardMode;
use crate::region::{Px, Rect};
use crate::scale::{MAX_SURFACE_EXTENT, Size, wire_extent};
use crate::toplevel::{Decorations, ToplevelState};

/// What a toplevel configure leaves the surface looking like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ToplevelLayout {
    /// The resize band the new state allows.
    pub(super) inset: Inset,
    /// The surface around the frame.
    pub(super) surface: Size,
    /// Where the frame sits in the surface, when the host tells the compositor: a
    /// client-decorated window sets its window geometry to the frame. `None` for a
    /// server-decorated window, whose surface is its frame already.
    pub(super) geometry: Option<Rect>,
}

/// The layout a toplevel configure produces: the frame size (the configure's, or `frame_now` on
/// an axis it leaves to us), the band `next` allows, and the frame's place in the surface.
pub(super) fn toplevel_layout(
    frame_now: Size,
    decorations: Decorations,
    next: ToplevelState,
    wire: (Option<NonZeroU32>, Option<NonZeroU32>),
) -> ToplevelLayout {
    let inset = inset_for(decorations, next);
    let frame = toplevel_configured_size(frame_now, wire);
    let surface = surface_of(frame, inset);
    let geometry = match decorations {
        Decorations::Client(_) => Some(window_geometry(surface, inset)),
        Decorations::Server => None,
    };
    ToplevelLayout {
        inset,
        surface,
        geometry,
    }
}

/// The frame size to record from an `xdg_toplevel.configure`: the compositor's value on an
/// axis it constrains, or `current` (the last-known frame, initially `ToplevelConfig::size`)
/// on an axis it leaves to the client (`new_size`'s `None`). Configure sizes are window
/// geometry, so they never include the resize band.
pub(super) fn toplevel_configured_size(
    current: Size,
    wire: (Option<NonZeroU32>, Option<NonZeroU32>),
) -> Size {
    let axis = |got: Option<NonZeroU32>, fallback: Px| -> Px {
        got.map_or(fallback, |v| wire_extent(v.get()))
    };
    Size {
        width: axis(wire.0, current.width),
        height: axis(wire.1, current.height),
    }
}

/// The logical size of a popup from its configure. `xdg_popup.configure` always carries a
/// definite size; a non-positive width or height (which the protocol does not forbid outright)
/// is clamped to one pixel so a degenerate configure never leaves the surface unrenderable, and
/// one beyond [`MAX_SURFACE_EXTENT`] is kept to it.
pub(super) fn popup_size(width: Px, height: Px) -> Size {
    Size {
        width: width.clamp(1, MAX_SURFACE_EXTENT),
        height: height.clamp(1, MAX_SURFACE_EXTENT),
    }
}

/// Whether a layer's keyboard mode drops back to `KeyboardMode::None` after one of its popups
/// closed. A layer spawned with `KeyboardMode::None` that now holds another mode (lifted to
/// `OnDemand` for a grabbing popup) drops back once no grabbing popup of it is left open
/// (`grabs_open`). A layer spawned with another mode, or already back at `None`, keeps what it
/// has.
pub(super) fn keyboard_restore(
    configured: KeyboardMode,
    current: KeyboardMode,
    grabs_open: bool,
) -> bool {
    !grabs_open && configured == KeyboardMode::None && current != KeyboardMode::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toplevel::{Fullscreen, Maximize, ResizeMargin};

    const CURRENT: Size = Size {
        width: 640,
        height: 480,
    };
    type Wire = (Option<NonZeroU32>, Option<NonZeroU32>);

    fn nz(v: u32) -> Option<NonZeroU32> {
        NonZeroU32::new(v)
    }

    #[test]
    fn toplevel_configured_size_falls_back_per_axis() {
        let cases: &[(&str, Wire, Size)] = &[
            (
                "both axes left to the client keep the current size",
                (None, None),
                CURRENT,
            ),
            (
                "the compositor constrains both axes",
                (nz(800), nz(600)),
                Size {
                    width: 800,
                    height: 600,
                },
            ),
            (
                "only one axis is constrained",
                (nz(800), None),
                Size {
                    width: 800,
                    height: 480,
                },
            ),
        ];
        for (name, wire, want) in cases {
            assert_eq!(toplevel_configured_size(CURRENT, *wire), *want, "{name}");
        }
    }

    #[test]
    fn a_toplevel_configure_lays_out_the_frame_inside_its_band() {
        let floating = ToplevelState::default();
        let maximized = ToplevelState {
            maximized: Maximize::Set,
            ..floating
        };
        let fullscreen = ToplevelState {
            fullscreen: Fullscreen::Set,
            ..floating
        };
        let frame = Size {
            width: 800,
            height: 600,
        };
        let cases: &[(&str, Decorations, ToplevelState, ToplevelLayout)] = &[
            (
                "floating client window: the band surrounds the frame",
                Decorations::CLIENT,
                floating,
                ToplevelLayout {
                    inset: Inset(7),
                    surface: Size {
                        width: 814,
                        height: 614,
                    },
                    geometry: Some(Rect {
                        x: 7,
                        y: 7,
                        width: 800,
                        height: 600,
                    }),
                },
            ),
            (
                "custom margin widens the band",
                Decorations::Client(ResizeMargin(12)),
                floating,
                ToplevelLayout {
                    inset: Inset(12),
                    surface: Size {
                        width: 824,
                        height: 624,
                    },
                    geometry: Some(Rect {
                        x: 12,
                        y: 12,
                        width: 800,
                        height: 600,
                    }),
                },
            ),
            (
                "maximized client window: no band, the geometry is the surface",
                Decorations::CLIENT,
                maximized,
                ToplevelLayout {
                    inset: Inset::NONE,
                    surface: frame,
                    geometry: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 800,
                        height: 600,
                    }),
                },
            ),
            (
                "fullscreen client window: no band",
                Decorations::CLIENT,
                fullscreen,
                ToplevelLayout {
                    inset: Inset::NONE,
                    surface: frame,
                    geometry: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 800,
                        height: 600,
                    }),
                },
            ),
            (
                "server-decorated window: nothing to tell the compositor",
                Decorations::Server,
                floating,
                ToplevelLayout {
                    inset: Inset::NONE,
                    surface: frame,
                    geometry: None,
                },
            ),
        ];
        for (name, decorations, state, want) in cases {
            let got = toplevel_layout(CURRENT, *decorations, *state, (nz(800), nz(600)));
            assert_eq!(got, *want, "{name}");
        }
    }

    #[test]
    fn a_popup_configure_is_at_least_one_pixel_each_way() {
        let cases: &[(&str, (Px, Px), Size)] = &[
            (
                "a definite size passes through",
                (200, 40),
                Size {
                    width: 200,
                    height: 40,
                },
            ),
            (
                "zero width is clamped",
                (0, 40),
                Size {
                    width: 1,
                    height: 40,
                },
            ),
            (
                "negative height is clamped",
                (200, -3),
                Size {
                    width: 200,
                    height: 1,
                },
            ),
        ];
        for (name, (width, height), want) in cases {
            assert_eq!(popup_size(*width, *height), *want, "{name}");
        }
    }

    #[test]
    fn a_lifted_layer_drops_back_only_when_no_grabbing_popup_is_left() {
        use KeyboardMode::{Exclusive, None as NoKeys, OnDemand};
        let cases: &[(&str, KeyboardMode, KeyboardMode, bool, bool)] = &[
            (
                "lifted, no popup left: restore",
                NoKeys,
                OnDemand,
                false,
                true,
            ),
            (
                "lifted, another grabbing popup open: keep",
                NoKeys,
                OnDemand,
                true,
                false,
            ),
            (
                "set to exclusive since spawning: restore too",
                NoKeys,
                Exclusive,
                false,
                true,
            ),
            ("already back at none: keep", NoKeys, NoKeys, false, false),
            ("spawned on demand: keep", OnDemand, OnDemand, false, false),
            (
                "spawned exclusive: keep",
                Exclusive,
                Exclusive,
                false,
                false,
            ),
        ];
        for (name, configured, current, grabs_open, want) in cases {
            assert_eq!(
                keyboard_restore(*configured, *current, *grabs_open),
                *want,
                "{name}"
            );
        }
    }
}
