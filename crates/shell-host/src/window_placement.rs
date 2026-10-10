//! Where a person can ask a window to go (the zoom button's menu), and whether the compositor
//! can do it.
//!
//! xdg-shell has no such request; `quire_window_v1.request_placement` does, so the choices are
//! offered only on a compositor that advertises it (`handlers::quire_window`).
//!
//! See FINDINGS F50, p2-z-sh.

/// A placement a menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowPlacement {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    /// The whole usable area, still a window.
    Fill,
    /// Centred on its output at its own size.
    Centre,
    /// The size and place before the last placement.
    Restore,
}

/// Whether the compositor takes placement requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlacementSupport {
    /// It advertises `quire_window_manager_v1`: `SurfaceHandle::request_placement` acts.
    Offered,
    /// It does not: the request is dropped, so a menu should not show the choices.
    Absent,
}
