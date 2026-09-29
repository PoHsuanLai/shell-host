//! The Wayland-side state every handler runs on, and the one list of delegate macros in the
//! crate. Handler trait impls live in `handlers/*`; this file only wires the protocol objects
//! to them.

use std::collections::{BTreeMap, VecDeque};

use blitz_traits::shell::ColorScheme;
use smithay_client_toolkit::activation::ActivationState;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::session_lock::SessionLockState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::{
    delegate_activation, delegate_compositor, delegate_data_device, delegate_keyboard,
    delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_session_lock, delegate_shm, delegate_xdg_popup, delegate_xdg_shell,
    delegate_xdg_window, registry_handlers,
};
use wayland_client::{Connection, QueueHandle, delegate_dispatch};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use std::time::{Duration, Instant};

use blitz_traits::events::KeyState;

use blitz_traits::events::MouseEventButtons;
use keyboard_types::Modifiers;
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use wayland_client::globals::GlobalList;

use crate::blur_health::{
    BlurHealth, CompositorFacts, CompositorKind, RenderVendor, blur_health, compositor_kind,
    output_backend,
};
use crate::clipboard::Clipboard;
use crate::clock::HostClock;
use crate::csd::Zone;
use crate::cursor::Cursor;
use crate::dom::fonts::SharedFonts;
use crate::error::{HostError, Result};
use crate::event::ShellEvent;
use crate::frame::{FrameInput, TimerCmd};
use crate::gpu::context::GpuContext;
use crate::grab::GrabRecord;
use crate::handle::RequestQueue;
use crate::handlers::activation::ActivationRequest;
use crate::handlers::background_effect::{BackgroundEffect, Support};
use crate::handlers::drag_source::Dragging;
use crate::handlers::fractional::Fractional;
use crate::handlers::render_device::RenderDevice;
use crate::handlers::seat::SeatInput;
use crate::handlers::text_input::TextInputManager;
use crate::id::{SurfaceId, SurfaceIds};
use crate::ime::{Ime, ImeFocus, PendingIme};
use crate::input::focus::PointerFocus;
use crate::input::key::{Repeat, key_event};
use crate::input::palmrest::Palmrest;
use crate::input::pointer::PointerPos;
use crate::lock::active::{ActiveLock, LockIds};
use crate::prepaint::Prepaint;
use crate::scroll::suppress::Suppress;
use crate::scroll::tuning::Tuning;
use crate::surface::HostSurface;
use crate::timers::TimerRequest;
use crate::wake::WokenSet;

/// `wl_keyboard.repeat_info` values until the compositor sends its own (X's defaults).
const DEFAULT_REPEAT_DELAY: Duration = Duration::from_millis(600);
const DEFAULT_REPEAT_INTERVAL: Duration = Duration::from_millis(40);

/// Everything the Wayland handlers read and write. Field order is drop order: surfaces (and
/// with them every wgpu surface) first, the connection last.
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
    /// The compositor's main render device, for `blur_health`.
    pub(crate) render_device: Option<RenderDevice>,
    /// cosmic-comp or not, from the globals at bind time.
    pub(crate) compositor_kind: CompositorKind,
    pub(crate) text_input: Option<TextInputManager>,
    pub(crate) cursor: Cursor,
    pub(crate) ime: Ime,
    pub(crate) clipboard: Clipboard,
    /// The drag out of one of our surfaces, while one is under way.
    pub(crate) drag: Dragging,
    /// `ext_session_lock_manager_v1`; `lock` fails with `LockError::Unsupported` without it.
    pub(crate) session_lock: SessionLockState,
    /// The session lock in force, from its request until it ends.
    pub(crate) lock: Option<ActiveLock>,
    pub(crate) lock_ids: LockIds,
    /// `SharedFonts::system()` alone until `Host::connect` registers `HostConfig::fonts` into
    /// it, right after `bind`; every document built after that point (`state.fonts.for_document()`
    /// at `ctx/spawn.rs`) sees the shell's bundled faces.
    pub(crate) fonts: SharedFonts,
    pub(crate) clock: HostClock,
    pub(crate) ids: SurfaceIds,
    pub(crate) color_scheme: ColorScheme,
    /// `HostConfig::prepaint`, handed to every document the host builds.
    pub(crate) prepaint: Prepaint,
    /// `HostConfig::first_mouse_window_ms`, read by the pointer handler.
    pub(crate) first_mouse_window: Duration,
    /// Requests from documents' `SurfaceHandle`s.
    pub(crate) requests: RequestQueue,
    /// Surfaces whose Dioxus tasks are ready.
    pub(crate) woken: WokenSet,
    /// Events for the shell's closure, raised by the handlers.
    pub(crate) events: VecDeque<ShellEvent>,
    /// Timers the handlers want armed or cancelled.
    pub(crate) timers: Vec<TimerRequest>,
    /// The scroll configuration and physics every surface uses (`HostCtx::set_scroll_config`).
    pub(crate) scroll: Tuning,
    /// The double-scroll window palmrest's fingers open (design/11 §11.3.14).
    pub(crate) suppress: Suppress,
    /// The evdev code of a scroll key the engine took, so its repeats and release are too.
    pub(crate) scroll_key: Option<u32>,
    /// The palmrest socket, when the daemon runs.
    pub(crate) palmrest: Palmrest,
    pub(crate) qh: QueueHandle<HostState>,
    pub(crate) conn: Connection,
}

impl HostState {
    /// Bind every global the host uses on `globals`. `wl_compositor`, `wl_shm` and
    /// `xdg_wm_base` are required; everything else is optional and only loses its feature.
    pub(crate) fn bind(
        conn: Connection,
        globals: &GlobalList,
        qh: QueueHandle<HostState>,
        requests: RequestQueue,
        first_mouse_window: Duration,
    ) -> Result<HostState> {
        let missing = |global| move |_| HostError::MissingGlobal { global };
        let compositor = CompositorState::bind(globals, &qh).map_err(missing("wl_compositor"))?;
        let shm = Shm::bind(globals, &qh).map_err(missing("wl_shm"))?;
        let xdg_shell = XdgShell::bind(globals, &qh).map_err(missing("xdg_wm_base"))?;
        Ok(HostState {
            surfaces: BTreeMap::new(),
            gpu: None,
            registry: RegistryState::new(globals),
            outputs: OutputState::new(globals, &qh),
            compositor,
            shm,
            xdg_shell,
            layer_shell: LayerShell::bind(globals, &qh).ok(),
            seat: SeatState::new(globals, &qh),
            input: SeatInput {
                seat: None,
                pointer: None,
                keyboard: None,
                pointer_focus: PointerFocus::Nowhere,
                position: PointerPos::OUTSIDE,
                buttons: MouseEventButtons::None,
                keyboard_focus: None,
                focused_at: None,
                modifiers: Modifiers::empty(),
                held_modifiers: crate::modifiers::Modifiers::default(),
                modality: crate::live::Live::new(crate::modality::InputModality::default()),
                last_serial: 0,
                grab: GrabRecord::default(),
                zone: Zone::Document,
                data_device: None,
                repeating: None,
                repeat_delay: DEFAULT_REPEAT_DELAY,
                repeat_interval: DEFAULT_REPEAT_INTERVAL,
            },
            data_device: DataDeviceManagerState::bind(globals, &qh).ok(),
            activation: ActivationState::bind(globals, &qh).ok(),
            fractional: Fractional::bind(globals, &qh).ok(),
            background_effect: BackgroundEffect::bind(globals, &qh).ok(),
            render_device: RenderDevice::bind(globals, &qh).ok(),
            compositor_kind: globals.contents().with_list(|list| {
                compositor_kind(list.iter().map(|global| global.interface.as_str()))
            }),
            text_input: TextInputManager::bind(globals, &qh).ok(),
            cursor: Cursor {
                manager: CursorShapeManager::bind(globals, &qh).ok(),
                device: None,
                current: None,
            },
            ime: Ime {
                pending: PendingIme::default(),
                focus: ImeFocus::Disabled,
                serial: 0,
            },
            clipboard: Clipboard::default(),
            drag: Dragging::default(),
            session_lock: SessionLockState::new(globals, &qh),
            lock: None,
            lock_ids: LockIds::default(),
            fonts: SharedFonts::system(),
            clock: HostClock::starting_at(Instant::now()),
            ids: SurfaceIds::new(),
            color_scheme: ColorScheme::Light,
            prepaint: Prepaint::None,
            first_mouse_window,
            requests,
            woken: WokenSet::default(),
            events: VecDeque::new(),
            timers: Vec::new(),
            scroll: Tuning::default(),
            suppress: Suppress::default(),
            scroll_key: None,
            palmrest: Palmrest::from_env(),
            qh,
            conn,
        })
    }

    /// Feed `input` to surface `id`'s frame machine and queue the timer change it asks for.
    /// Unknown ids are ignored: the surface closed while the input was in flight.
    pub(crate) fn frame_input(&mut self, id: SurfaceId, input: FrameInput) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        let action = surface.step(input);
        if action.timer != TimerCmd::Keep {
            self.timers.push(TimerRequest::Watchdog {
                id,
                cmd: action.timer,
            });
        }
    }

    /// Whether the compositor can blur behind a surface at all: it advertises blur, and it is
    /// not a compositor known to lose blurred surfaces.
    pub(crate) fn blur_supported(&self) -> Support {
        let advertised = self
            .background_effect
            .as_ref()
            .map_or(Support::Unsupported, |effect| effect.capabilities().blur);
        match (advertised, self.blur_health()) {
            (Support::Supported, BlurHealth::Renders) => Support::Supported,
            (Support::Supported, BlurHealth::Breaks) | (Support::Unsupported, _) => {
                Support::Unsupported
            }
        }
    }

    /// Whether advertised blur would render, from what is known about the compositor.
    pub(crate) fn blur_health(&self) -> BlurHealth {
        let names: Vec<String> = self
            .outputs
            .outputs()
            .filter_map(|output| self.outputs.info(&output)?.name)
            .collect();
        blur_health(CompositorFacts {
            kind: self.compositor_kind,
            backend: output_backend(names.iter().map(String::as_str)),
            vendor: self
                .render_device
                .as_ref()
                .map_or(RenderVendor::Unknown, RenderDevice::vendor),
        })
    }

    /// One tick of the key-repeat timer the context arms for `TimerRequest::RepeatStart`:
    /// the held key again, as a `Repeat::Repeated` press, to the focused surface.
    pub(crate) fn key_repeat(&mut self) {
        let (Some(id), Some(held)) = (self.input.keyboard_focus, self.input.repeating.as_ref())
        else {
            return;
        };
        let raw = held.raw_code;
        let event = key_event(
            held,
            self.input.modifiers,
            KeyState::Pressed,
            Repeat::Repeated,
            self.composing(),
        );
        self.key_down(id, event, raw);
    }

    /// The adapter label every stats line carries; empty on the CPU backend.
    pub(crate) fn adapter_label(&self) -> String {
        self.gpu
            .as_ref()
            .map(|gpu| gpu.adapter().label())
            .unwrap_or_default()
    }
}

impl Drop for HostState {
    fn drop(&mut self) {
        // Every wgpu surface must go before its wl_surface and before the connection, even on
        // a panic; then flush so nothing lingers on screen.
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

impl AsMut<RenderDevice> for HostState {
    fn as_mut(&mut self) -> &mut RenderDevice {
        // Invariant: dmabuf feedback objects exist only when the global was bound.
        self.render_device
            .as_mut()
            .expect("dmabuf feedback events imply a bound manager")
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
delegate_session_lock!(HostState);

delegate_dispatch!(HostState: [WpFractionalScaleManagerV1: ()] => Fractional);
delegate_dispatch!(HostState: [WpFractionalScaleV1: ()] => Fractional);
delegate_dispatch!(HostState: [WpViewporter: ()] => Fractional);
delegate_dispatch!(HostState: [WpViewport: ()] => Fractional);
delegate_dispatch!(HostState: [ExtBackgroundEffectManagerV1: ()] => BackgroundEffect);
delegate_dispatch!(HostState: [ExtBackgroundEffectSurfaceV1: ()] => BackgroundEffect);
delegate_dispatch!(HostState: [ZwpLinuxDmabufV1: ()] => RenderDevice);
delegate_dispatch!(HostState: [ZwpLinuxDmabufFeedbackV1: ()] => RenderDevice);
// zwp_text_input_v3 is dispatched by `handlers::text_input` directly on `HostState`: sctk has no
// helper to delegate to.
