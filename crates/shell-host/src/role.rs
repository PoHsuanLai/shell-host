//! What kind of surface a `HostSurface` is, with the sctk object that owns its `wl_surface`.

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::window::Window;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::id::SurfaceId;
use crate::layer::{KeyboardMode, LayerConfig};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::toplevel::{ToplevelConfig, ToplevelState};

/// A surface's role.
#[derive(Debug)]
pub enum Role {
    Layer {
        surface: LayerSurface,
        config: LayerConfig,
        /// The keyboard mode actually in force: `config.keyboard`, or `OnDemand` while a
        /// grabbing popup is open from a `KeyboardMode::None` layer.
        keyboard: KeyboardMode,
    },
    Popup {
        popup: Popup,
        parent: SurfaceId,
        config: PopupConfig,
        placement: PopupPlacement,
    },
    Toplevel {
        window: Window,
        config: ToplevelConfig,
        /// As of the last configure (`activated` drives the first-mouse filter).
        state: ToplevelState,
    },
}

/// Which role a surface has, without its objects: what a `SurfaceHandle` knows of its surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoleKind {
    Layer,
    Popup,
    Toplevel,
}

impl Role {
    /// This role's kind.
    pub fn kind(&self) -> RoleKind {
        match self {
            Role::Layer { .. } => RoleKind::Layer,
            Role::Popup { .. } => RoleKind::Popup,
            Role::Toplevel { .. } => RoleKind::Toplevel,
        }
    }

    /// The `wl_surface` the role owns.
    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Role::Layer { surface, .. } => surface.wl_surface(),
            Role::Popup { popup, .. } => popup.wl_surface(),
            Role::Toplevel { window, .. } => window.wl_surface(),
        }
    }

    /// The parent, for a popup.
    pub fn parent(&self) -> Option<SurfaceId> {
        match self {
            Role::Popup { parent, .. } => Some(*parent),
            Role::Layer { .. } | Role::Toplevel { .. } => None,
        }
    }
}
