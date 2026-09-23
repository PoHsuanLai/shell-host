//! What kind of surface a `HostSurface` is, with the sctk object that owns its `wl_surface`.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::window::Window;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::id::SurfaceId;
use crate::layer::{KeyboardMode, LayerConfig};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::toplevel::{Activation, ToplevelConfig};

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
        activation: Activation,
    },
}

impl Role {
    /// The `wl_surface` the role owns.
    pub fn wl_surface(&self) -> &WlSurface {
        todo!()
    }

    /// The parent, for a popup.
    pub fn parent(&self) -> Option<SurfaceId> {
        todo!()
    }
}
