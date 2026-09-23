//! The Wayland-side state every handler runs on, and the one list of delegate macros in the
//! crate. Handler trait impls live in `handlers/*`; this file only wires the protocol objects
//! to them.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::collections::{BTreeMap, VecDeque};

use blitz_traits::shell::ColorScheme;
use smithay_client_toolkit::activation::ActivationState;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::{
    delegate_activation, delegate_compositor, delegate_data_device, delegate_keyboard,
    delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_popup, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::{Connection, QueueHandle, delegate_dispatch};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use crate::clipboard::Clipboard;
use crate::clock::HostClock;
use crate::cursor::Cursor;
use crate::dom::fonts::SharedFonts;
use crate::event::ShellEvent;
use crate::gpu::context::GpuContext;
use crate::handle::RequestQueue;
use crate::handlers::activation::ActivationRequest;
use crate::handlers::background_effect::BackgroundEffect;
use crate::handlers::fractional::Fractional;
use crate::handlers::seat::SeatInput;
use crate::handlers::text_input::TextInputManager;
use crate::id::{SurfaceId, SurfaceIds};
use crate::ime::Ime;
use crate::surface::HostSurface;
use crate::timers::TimerRequest;
use crate::wake::WokenSet;

/// Everything the Wayland handlers read and write. Field order is drop order: surfaces (and
/// with them every wgpu surface) first, the connection last (SPIKE.md S1 finding 6).
#[derive(Debug)]
pub struct HostState {
    pub(crate) surfaces: BTreeMap<SurfaceId, HostSurface>,
    pub(crate) gpu: Option<GpuContext>,
    pub(crate) registry: RegistryState,
    pub(crate) outputs: OutputState,
    pub(crate) compositor: CompositorState,
    pub(crate) shm: Shm,
    pub(crate) xdg_shell: XdgShell,
    /// `None` on a compositor without layer-shell: `spawn_layer` then fails with `Unsupported`.
    pub(crate) layer_shell: Option<LayerShell>,
    pub(crate) seat: SeatState,
    pub(crate) input: SeatInput,
    pub(crate) data_device: Option<DataDeviceManagerState>,
    pub(crate) activation: Option<ActivationState>,
    pub(crate) fractional: Option<Fractional>,
    pub(crate) background_effect: Option<BackgroundEffect>,
    pub(crate) text_input: Option<TextInputManager>,
    pub(crate) cursor: Cursor,
    pub(crate) ime: Ime,
    pub(crate) clipboard: Clipboard,
    pub(crate) fonts: SharedFonts,
    pub(crate) clock: HostClock,
    pub(crate) ids: SurfaceIds,
    pub(crate) color_scheme: ColorScheme,
    /// Requests from documents' `SurfaceHandle`s.
    pub(crate) requests: RequestQueue,
    /// Surfaces whose Dioxus tasks are ready.
    pub(crate) woken: WokenSet,
    /// Events for the shell's closure, raised by the handlers.
    pub(crate) events: VecDeque<ShellEvent>,
    /// Timers the handlers want armed or cancelled.
    pub(crate) timers: Vec<TimerRequest>,
    pub(crate) qh: QueueHandle<HostState>,
    pub(crate) conn: Connection,
}

impl Drop for HostState {
    fn drop(&mut self) {
        // Every wgpu surface must go before its wl_surface and before the connection, even on
        // a panic; then flush so nothing lingers on screen (SPIKE.md S1 finding 6).
        self.surfaces.clear();
        let _ = self.conn.flush();
    }
}

impl ProvidesRegistryState for HostState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl AsMut<Fractional> for HostState {
    fn as_mut(&mut self) -> &mut Fractional {
        // Invariant: fractional-scale objects exist only when the global was bound.
        self.fractional
            .as_mut()
            .expect("fractional-scale events imply a bound manager")
    }
}

impl AsMut<BackgroundEffect> for HostState {
    fn as_mut(&mut self) -> &mut BackgroundEffect {
        // Invariant: background-effect objects exist only when the global was bound.
        self.background_effect
            .as_mut()
            .expect("background-effect events imply a bound manager")
    }
}

delegate_registry!(HostState);
delegate_compositor!(HostState);
delegate_output!(HostState);
delegate_shm!(HostState);
delegate_layer!(HostState);
delegate_xdg_shell!(HostState);
delegate_xdg_window!(HostState);
delegate_xdg_popup!(HostState);
delegate_seat!(HostState);
// Also routes wp_cursor_shape_manager_v1 and its devices to sctk's CursorShapeManager.
delegate_pointer!(HostState);
delegate_keyboard!(HostState);
delegate_data_device!(HostState);
delegate_activation!(HostState, ActivationRequest);

delegate_dispatch!(HostState: [WpFractionalScaleManagerV1: ()] => Fractional);
delegate_dispatch!(HostState: [WpFractionalScaleV1: ()] => Fractional);
delegate_dispatch!(HostState: [WpViewporter: ()] => Fractional);
delegate_dispatch!(HostState: [WpViewport: ()] => Fractional);
delegate_dispatch!(HostState: [ExtBackgroundEffectManagerV1: ()] => BackgroundEffect);
delegate_dispatch!(HostState: [ExtBackgroundEffectSurfaceV1: ()] => BackgroundEffect);
// zwp_text_input_v3 is dispatched by `handlers::text_input` directly on `HostState`: sctk has no
// helper to delegate to.
