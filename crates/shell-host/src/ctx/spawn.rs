//! Creating surfaces: the role (layer surface, xdg toplevel, xdg popup) with its initial state,
//! the document with its `SurfaceHandle`, and the `HostSurface` that ties them together.
//!
//! A role is built unmapped (every request made, no commit) and mapped separately, because a
//! hidden surface keeps an unmapped role and maps it again when shown.

use std::sync::Arc;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgSurface};
use wayland_client::protocol::wl_surface::WlSurface;

use crate::blur::BlurSpec;
use crate::debug::{StatsWindow, SurfaceName};
use crate::dom::document::{DocumentEnv, SurfaceDocument};
use crate::dom::net::LocalNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::error::{HostError, Result};
use crate::frame::FrameState;
use crate::handle::SurfaceHandle;
use crate::id::{OutputId, SurfaceId};
use crate::layer::{
    Extent, KeyboardMode, LayerConfig, OutputTarget, to_wire_anchor, to_wire_exclusive,
    to_wire_keyboard, to_wire_layer, to_wire_size,
};
use crate::live_scale::LiveScale;
use crate::msg::Visibility;
use crate::placement::PopupPlacement;
use crate::popup::{
    Grab, PopupAnchor, PopupConfig, PositionerParams, positioner_params, to_wire_anchor as edge,
    to_wire_constraint_adjustment, to_wire_gravity,
};
use crate::region::{Px, RegionSpec};
use crate::role::Role;
use crate::root::Root;
use crate::scale::{Scale, Size};
use crate::state::HostState;
use crate::surface::{AppliedRegions, Configured, HostSurface};
use crate::tick::FrameTicks;
use crate::toplevel::{Activation, ToplevelConfig, to_wire_decorations};

/// Create and map layer surface `id`.
pub(super) fn layer(
    state: &mut HostState,
    id: SurfaceId,
    config: LayerConfig,
    root: Root,
) -> Result<()> {
    let name = SurfaceName(config.namespace.clone());
    let requested = requested_size(&config);
    let (blur, input) = (config.blur.clone(), config.input.clone());
    let role = layer_role(state, config)?;
    map(state, &role);
    let at = (requested, Scale::ONE);
    insert(state, id, role, name, at, (blur, input), root);
    Ok(())
}

/// Create and map toplevel `id`.
pub(super) fn toplevel(state: &mut HostState, id: SurfaceId, config: ToplevelConfig, root: Root) {
    let name = SurfaceName(config.app_id.clone());
    let size = config.size;
    let role = toplevel_role(state, config);
    map(state, &role);
    let regions = (BlurSpec::None, RegionSpec::Whole);
    insert(state, id, role, name, (size, Scale::ONE), regions, root);
}

/// Create and map popup `id` on `parent`. A grabbing popup lifts a `KeyboardMode::None` parent
/// layer to `OnDemand` until it closes.
pub(super) fn popup(
    state: &mut HostState,
    parent: SurfaceId,
    id: SurfaceId,
    config: PopupConfig,
    placement: PopupPlacement,
    root: Root,
) -> Result<()> {
    let parent_name = state
        .surfaces
        .get(&parent)
        .map(|surface| surface.name.0.clone())
        .ok_or(HostError::NoSurface(parent))?;
    let grab = config.grab;
    // A popup opens on its parent's output nearly always, so the parent's scale is the best
    // first guess: the popup's first render already reads it (`use_surface_scale`), and the
    // compositor's own preferred scale corrects it if it differs.
    let initial = state
        .surfaces
        .get(&parent)
        .map_or(Scale::ONE, |surface| surface.scale);
    let role = popup_role(state, parent, config, placement)?;
    if grab == Grab::Grabbed {
        lift_keyboard(state, parent);
    }
    map(state, &role);
    let name = SurfaceName(format!("{parent_name}/popup"));
    let regions = (BlurSpec::None, RegionSpec::Whole);
    let at = (placement.size, initial);
    insert(state, id, role, name, at, regions, root);
    Ok(())
}

/// A fresh, unmapped role with the same configuration as `old`, which is destroyed first.
pub(super) fn rebuild(state: &mut HostState, old: Role) -> Result<Role> {
    match old {
        Role::Layer {
            surface, config, ..
        } => {
            drop(surface);
            layer_role(state, config)
        }
        Role::Toplevel { window, config, .. } => {
            drop(window);
            Ok(toplevel_role(state, config))
        }
        Role::Popup {
            popup,
            parent,
            config,
            placement,
        } => {
            drop(popup);
            popup_role(state, parent, config, placement)
        }
    }
}

/// Map `role`: a popup's grab (which must precede its first commit), then the initial commit
/// the compositor answers with a configure.
pub(super) fn map(state: &HostState, role: &Role) {
    if let Role::Popup { popup, config, .. } = role
        && config.grab == Grab::Grabbed
        && let Some(seat) = &state.input.seat
    {
        popup.xdg_popup().grab(seat, state.input.last_serial);
    }
    role.wl_surface().commit();
}

fn layer_role(state: &mut HostState, config: LayerConfig) -> Result<Role> {
    let layer_shell = state.layer_shell.as_ref().ok_or(HostError::Unsupported {
        global: "zwlr_layer_shell_v1",
        feature: "layer surfaces",
    })?;
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    let output = match config.output {
        OutputTarget::CompositorPicks => None,
        OutputTarget::Output(OutputId(name)) => state.outputs.outputs().find(|output| {
            state
                .outputs
                .info(output)
                .is_some_and(|info| info.id == name)
        }),
    };
    let surface = layer_shell.create_layer_surface(
        &state.qh,
        wl,
        to_wire_layer(config.layer),
        Some(config.namespace.clone()),
        output.as_ref(),
    );
    let (width, height) = to_wire_size(config.size);
    let margin = config.margin;
    surface.set_anchor(to_wire_anchor(config.anchor));
    surface.set_size(width, height);
    surface.set_exclusive_zone(to_wire_exclusive(config.exclusive));
    surface.set_margin(margin.top, margin.right, margin.bottom, margin.left);
    surface.set_keyboard_interactivity(to_wire_keyboard(config.keyboard));
    Ok(Role::Layer {
        surface,
        keyboard: config.keyboard,
        config,
    })
}

fn toplevel_role(state: &mut HostState, config: ToplevelConfig) -> Role {
    let wl = new_wl_surface(state);
    let window =
        state
            .xdg_shell
            .create_window(wl, to_wire_decorations(config.decorations), &state.qh);
    window.set_title(config.title.clone());
    window.set_app_id(config.app_id.clone());
    window.set_min_size(config.min_size.map(wire_size));
    window.set_max_size(config.max_size.map(wire_size));
    Role::Toplevel {
        window,
        config,
        activation: Activation::Inactive,
    }
}

fn popup_role(
    state: &mut HostState,
    parent: SurfaceId,
    config: PopupConfig,
    placement: PopupPlacement,
) -> Result<Role> {
    let parent_surface = state
        .surfaces
        .get(&parent)
        .ok_or(HostError::NoSurface(parent))?;
    let anchor_rect = match &config.anchor {
        PopupAnchor::Rect => placement.anchor_rect,
        PopupAnchor::Element(element) => parent_surface
            .document
            .element_rect(ToOwned::to_owned(element))
            .unwrap_or(placement.anchor_rect),
    };
    let positioner = positioner(
        state,
        &positioner_params(&config, anchor_rect, placement.size),
    )?;
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    let parent_xdg = match &parent_surface.role {
        Role::Layer { .. } => None,
        Role::Popup { popup, .. } => Some(popup.xdg_surface()),
        Role::Toplevel { window, .. } => Some(window.xdg_surface()),
    };
    let popup = Popup::from_surface(parent_xdg, &positioner, &state.qh, wl, &state.xdg_shell)
        .map_err(|_| HostError::MissingGlobal {
            global: "xdg_wm_base",
        })?;
    if let Role::Layer { surface, .. } = &parent_surface.role {
        surface.get_popup(popup.xdg_popup());
    }
    Ok(Role::Popup {
        popup,
        parent,
        config,
        placement,
    })
}

/// An `xdg_positioner` set up with `params`. Sizes are clamped to 1 px, since a zero size is a
/// protocol error.
pub(super) fn positioner(state: &HostState, params: &PositionerParams) -> Result<XdgPositioner> {
    let positioner =
        XdgPositioner::new(&state.xdg_shell).map_err(|_| HostError::MissingGlobal {
            global: "xdg_wm_base",
        })?;
    let rect = params.anchor_rect;
    positioner.set_size(params.size.width.max(1), params.size.height.max(1));
    positioner.set_anchor_rect(rect.x, rect.y, rect.width.max(1), rect.height.max(1));
    positioner.set_anchor(edge(params.anchor));
    positioner.set_gravity(to_wire_gravity(params.gravity));
    positioner.set_constraint_adjustment(to_wire_constraint_adjustment(params.adjust));
    Ok(positioner)
}

/// Switch a `KeyboardMode::None` layer to `OnDemand`, so the grabbing popup opened from it can
/// take keyboard focus (plan "Opening a grabbing popup ...").
fn lift_keyboard(state: &mut HostState, parent: SurfaceId) {
    if let Some(HostSurface {
        role: Role::Layer {
            surface, keyboard, ..
        },
        ..
    }) = state.surfaces.get_mut(&parent)
        && *keyboard == KeyboardMode::None
    {
        *keyboard = KeyboardMode::OnDemand;
        surface.set_keyboard_interactivity(to_wire_keyboard(KeyboardMode::OnDemand));
        surface.commit();
    }
}

fn new_wl_surface(state: &mut HostState) -> WlSurface {
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    wl
}

/// Build the document (with this surface's `SurfaceHandle` as a root context) and record the
/// surface at `logical` size and first-guess `scale`. It is marked woken, so the first flush
/// polls it.
fn insert(
    state: &mut HostState,
    id: SurfaceId,
    role: Role,
    name: SurfaceName,
    (logical, scale): (Size, Scale),
    (blur, input): (BlurSpec, RegionSpec),
    root: Root,
) {
    let shell = Arc::new(HostShellProvider::new(
        id,
        state.requests.ping(),
        state.clipboard.selection.clone(),
    ));
    let ticks = FrameTicks::default();
    let live_scale = LiveScale::new(scale);
    let handle = SurfaceHandle::new(
        id,
        state.requests.clone(),
        state.ids.clone(),
        ticks.clone(),
        live_scale.clone(),
    );
    let env = DocumentEnv {
        shell: Arc::clone(&shell),
        fonts: state.fonts.for_document(),
        net: Arc::new(LocalNet),
        viewport: super::paint::viewport(logical, scale, state.color_scheme),
        prepaint: state.prepaint.clone(),
    };
    let document = surface_document(root, handle, env);
    state.surfaces.insert(
        id,
        HostSurface {
            presenter: None,
            role,
            document,
            shell,
            id,
            name,
            frame: FrameState::NEW,
            watchdog: None,
            configured: Configured::Pending,
            visibility: Visibility::Shown,
            logical,
            scale,
            blur,
            input,
            stats: StatsWindow::default(),
            geometry: None,
            regions: AppliedRegions::default(),
            ticks,
            live_scale,
        },
    );
    state.woken.mark(id);
}

/// Surface `root`'s document, with `handle` provided as a root context before the first build,
/// so `use_surface()` works in every component of the surface from its first render.
fn surface_document(root: Root, handle: SurfaceHandle, env: DocumentEnv) -> SurfaceDocument {
    SurfaceDocument::build(root.context(handle), env)
}

/// The size a layer asks for; a `Fill` axis is 0 until the compositor configures it.
fn requested_size(config: &LayerConfig) -> Size {
    let px = |extent| match extent {
        Extent::Fill => 0,
        Extent::Px(px) => Px::try_from(px).unwrap_or(Px::MAX),
    };
    Size {
        width: px(config.size.width),
        height: px(config.size.height),
    }
}

fn wire_size(size: Size) -> (u32, u32) {
    let px = |v: Px| u32::try_from(v).unwrap_or(0);
    (px(size.width), px(size.height))
}

#[cfg(test)]
mod tests {
    use blitz_dom::Document as _;
    use blitz_traits::shell::{ColorScheme, Viewport};
    use dioxus::prelude::*;

    use super::*;
    use crate::clipboard::SharedSelection;
    use crate::dom::document::Polled;
    use crate::dom::fonts::SharedFonts;
    use crate::handle::{RequestQueue, use_frame_tick, use_surface, use_surface_scale};
    use crate::id::SurfaceIds;
    use crate::msg::{Request, SurfaceOp};
    use crate::prepaint::{Prepaint, Prepainted};
    use crate::tick::{FrameTick, HostInstant};

    fn app() -> Element {
        let surface = use_surface();
        use_hook(|| surface.set_blur(BlurSpec::None));
        rsx! { div { "surface {surface.id().0}" } }
    }

    fn ticking() -> Element {
        let tick = use_frame_tick();
        let FrameTick { at, dt } = tick();
        rsx! { div { "at {at.0.as_millis()} dt {dt.as_millis()}" } }
    }

    fn text(document: &mut SurfaceDocument) -> String {
        let mut text = String::new();
        document.dioxus().inner().visit(|_, node| {
            if node.is_text_node() {
                text.push_str(&node.text_content());
            }
        });
        text
    }

    fn test_env(id: SurfaceId, ping: calloop::ping::Ping, prepaint: Prepaint) -> DocumentEnv {
        DocumentEnv {
            shell: Arc::new(HostShellProvider::new(id, ping, SharedSelection::default())),
            fonts: SharedFonts::system().for_document(),
            net: Arc::new(LocalNet),
            viewport: Viewport::new(200, 40, 1.0, ColorScheme::Light),
            prepaint,
        }
    }

    /// Counts its wakes, so a test polls a document only when the host would.
    #[derive(Default)]
    struct Wakes(std::sync::atomic::AtomicU32);

    impl std::task::Wake for Wakes {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl Wakes {
        fn take(&self) -> u32 {
            self.0.swap(0, std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// `use_frame_tick` re-renders its component with every tick the host publishes, through
    /// the document's own task and waker, and not before. The document is polled only when
    /// its waker fired, as the flush does, so a tick whose wake is lost fails here.
    #[test]
    fn use_frame_tick_renders_each_published_tick() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let requests = RequestQueue::new(ping.clone());
        let id = SurfaceId(7);
        let ticks = FrameTicks::default();
        let handle = SurfaceHandle::new(
            id,
            requests.clone(),
            SurfaceIds::new(),
            ticks.clone(),
            LiveScale::new(Scale::ONE),
        );
        let env = test_env(id, ping, Prepaint::None);
        let mut document = surface_document(Root::new(ticking), handle.clone(), env);
        let wakes = Arc::new(Wakes::default());
        let waker = std::task::Waker::from(Arc::clone(&wakes));
        let mut cx = std::task::Context::from_waker(&waker);
        document.poll(&mut cx);
        assert_eq!(text(&mut document), "at 0 dt 0");

        let drive = handle.drive_frames();
        assert!(
            matches!(requests.drain().as_slice(), [Request::Redraw(SurfaceId(7))]),
            "taking a drive asks for a frame"
        );
        let ms = |n| HostInstant(std::time::Duration::from_millis(n));
        let frames = [
            (100, "at 100 dt 0"),
            (116, "at 116 dt 16"),
            (133, "at 133 dt 17"),
            (150, "at 150 dt 17"),
        ];
        for (at, want) in frames {
            wakes.take();
            ticks.frame_due(crate::frame::Animation::Stopped, ms(at));
            assert!(wakes.take() > 0, "the tick at {at} ms woke the document");
            assert_eq!(document.poll(&mut cx), Polled::Changed, "{want}");
            assert_eq!(text(&mut document), want);
        }
        drop(drive);
        ticks.frame_due(crate::frame::Animation::Stopped, ms(166));
        assert_eq!(wakes.take(), 0, "no tick without a drive");
        assert_eq!(text(&mut document), "at 150 dt 17");
    }

    #[test]
    fn a_spawned_root_reads_its_surface_handle() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let requests = RequestQueue::new(ping.clone());
        let id = SurfaceId(42);
        let handle = SurfaceHandle::new(
            id,
            requests.clone(),
            SurfaceIds::new(),
            FrameTicks::default(),
            LiveScale::new(Scale::ONE),
        );
        let env = test_env(id, ping, Prepaint::None);

        let mut document = surface_document(Root::new(app), handle, env);

        let mut text = String::new();
        document.dioxus().inner().visit(|_, node| {
            if node.is_text_node() {
                text.push_str(&node.text_content());
            }
        });
        assert!(text.contains("surface 42"), "rendered {text:?}");
        let queued = requests.drain();
        assert!(
            matches!(
                queued.as_slice(),
                [Request::Apply {
                    id: SurfaceId(42),
                    op: SurfaceOp::SetBlur(BlurSpec::None)
                }]
            ),
            "the handle queues into the host: {queued:?}"
        );
    }

    fn scaled() -> Element {
        let scale = use_surface_scale();
        rsx! { div { "scale {scale().0}" } }
    }

    /// The scale is a root context before the first render, and each change the host
    /// publishes re-renders the reader through its own task and waker; a repeated scale wakes
    /// nothing, so it costs no frame.
    #[test]
    fn use_surface_scale_starts_at_the_published_scale_and_follows_each_change() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let id = SurfaceId(9);
        let live = LiveScale::new(Scale(150));
        let handle = SurfaceHandle::new(
            id,
            RequestQueue::new(ping.clone()),
            SurfaceIds::new(),
            FrameTicks::default(),
            live.clone(),
        );
        let env = test_env(id, ping, Prepaint::None);
        let mut document = surface_document(Root::new(scaled), handle.clone(), env);
        assert_eq!(
            text(&mut document),
            "scale 150",
            "the first render has the scale"
        );
        assert_eq!(handle.current_scale(), Scale(150));

        let wakes = Arc::new(Wakes::default());
        let waker = std::task::Waker::from(Arc::clone(&wakes));
        let mut cx = std::task::Context::from_waker(&waker);
        document.poll(&mut cx);
        wakes.take();
        live.publish(Scale(150));
        assert_eq!(wakes.take(), 0, "an unchanged scale wakes nothing");
        for wire in [180, 210, 120] {
            live.publish(Scale(wire));
            assert!(wakes.take() > 0, "publishing {wire} woke the document");
            assert_eq!(document.poll(&mut cx), Polled::Changed);
            assert_eq!(text(&mut document), format!("scale {wire}"));
        }
        assert_eq!(
            document.poll(&mut cx),
            Polled::Unchanged,
            "then it is quiet"
        );
    }

    /// The hook runs exactly once per `resolve`, after layout, at the viewport's scale (what
    /// a device-grid snap reads), and never on a build or a poll.
    #[test]
    fn resolve_runs_the_prepaint_hook_once_after_layout() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let id = SurfaceId(11);
        let seen = Arc::new(std::sync::Mutex::new(Vec::<(f64, f32)>::new()));
        let record = Arc::clone(&seen);
        let hook = Prepaint::hook(move |doc| {
            let height = doc.root_element().final_layout().size.height;
            let entry = (doc.viewport().scale_f64(), height);
            record.lock().expect("unpoisoned").push(entry);
        });
        let env = DocumentEnv {
            viewport: Viewport::new(300, 60, 1.5, ColorScheme::Light),
            ..test_env(id, ping.clone(), hook)
        };
        let handle = SurfaceHandle::new(
            id,
            RequestQueue::new(ping),
            SurfaceIds::new(),
            FrameTicks::default(),
            LiveScale::new(Scale(180)),
        );
        let mut document = surface_document(Root::new(app), handle, env);
        let waker = std::task::Waker::noop();
        document.poll(&mut std::task::Context::from_waker(waker));
        let calls = || seen.lock().expect("unpoisoned").clone();
        assert!(calls().is_empty(), "building and polling never run it");

        assert_eq!(document.resolve(0.0), Prepainted::Ran);
        let first = calls();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, 1.5, "the hook sees the viewport's scale");
        assert!(first[0].1 > 0.0, "and a laid-out document: {first:?}");
        assert_eq!(document.resolve(0.1), Prepainted::Ran);
        assert_eq!(calls().len(), 2, "once per resolve");
    }

    #[test]
    fn without_a_hook_resolve_reports_none_ran() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let id = SurfaceId(12);
        let handle = SurfaceHandle::new(
            id,
            RequestQueue::new(ping.clone()),
            SurfaceIds::new(),
            FrameTicks::default(),
            LiveScale::new(Scale::ONE),
        );
        let env = test_env(id, ping, Prepaint::None);
        let mut document = surface_document(Root::new(app), handle, env);
        assert_eq!(document.resolve(0.0), Prepainted::NoHook);
    }
}
