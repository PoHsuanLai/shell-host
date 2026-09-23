//! xdg toplevels: every app window (design/20-SURFACES.md, "All apps are xdg toplevels through
//! shell-host").

use smithay_client_toolkit::shell::xdg::window::WindowDecorations;

use crate::scale::Size;

/// Everything `HostCtx::spawn_toplevel` needs besides the root component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToplevelConfig {
    /// The `xdg_toplevel.set_app_id` value; also the debug-command name of the window.
    pub app_id: String,
    pub title: String,
    /// The size to open at, in logical pixels, until the compositor configures another.
    pub size: Size,
    /// The smallest size the window accepts; `None` for no limit.
    pub min_size: Option<Size>,
    /// The largest size the window accepts; `None` for no limit.
    pub max_size: Option<Size>,
    pub decorations: Decorations,
}

/// Who draws the window frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decorations {
    /// The document draws its own titlebar and traffic lights (ds windows).
    Client,
    /// Ask the compositor for server-side decorations, falling back to none.
    Server,
}

/// Whether a toplevel is the active window, from its last configure. Drives the first-mouse
/// filter (design/13 §13.3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activation {
    Active,
    Inactive,
}

/// The sctk request for `decorations`.
pub fn to_wire_decorations(decorations: Decorations) -> WindowDecorations {
    match decorations {
        Decorations::Client => WindowDecorations::RequestClient,
        Decorations::Server => WindowDecorations::RequestServer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorations_map_to_sctk_requests() {
        let cases = [
            (Decorations::Client, WindowDecorations::RequestClient),
            (Decorations::Server, WindowDecorations::RequestServer),
        ];
        for (ours, wire) in cases {
            assert_eq!(to_wire_decorations(ours), wire, "{ours:?}");
        }
    }
}
