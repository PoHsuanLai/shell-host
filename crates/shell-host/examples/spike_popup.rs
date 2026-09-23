//! M0 spike item (e): an `xdg_popup` child of a layer surface, its own nested popup, and the
//! blur + fractional-scale protocols bound on the same bar surface, so all three pieces this
//! agent owns can be proven against one bare-shm client.
//!
//! A 32px top bar has one "button". `stdin` commands drive it (see the plan's spike-example
//! convention: examples are stdin-scripted, not mouse-driven, so every run is reproducible):
//!
//! - `click <x> <y>` — coordinates are relative to whichever surface is currently on top (the
//!   bar if nothing is open, popup 1 if only it is open, popup 2 if it is). A click inside the
//!   button (bar) or the submenu row (popup 1) opens the next popup; anything else dismisses
//!   the current top popup. sctk 0.20's `xdg_popup` has no way for this client to synthesize a
//!   real compositor-detected "outside click" (that needs real input this stdin harness cannot
//!   send), so dismissal here is the plan's own documented fallback for this item: the client
//!   destroys the popup itself and logs it as such. A genuine compositor-sent `popup_done` (for
//!   example, from a real mouse click during interactive use) is also wired up and logged
//!   distinctly.
//! - `capture <path>` — takes a full-screen screenshot with `spectacle` and logs the path.
//! - `quit` — exits.
//!
//! Every event this item cares about is logged to stdout with an `EVENT` prefix so a driving
//! shell script (or a human) can grep the run.

use std::io::BufRead;
use std::process::Command;

use calloop::EventLoop;
use calloop::channel;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, delegate_xdg_popup, delegate_xdg_shell,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle, delegate_dispatch};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use shell_host::handlers::background_effect::BackgroundEffect;
use shell_host::handlers::fractional::Fractional;
use shell_host::popup::{
    Adjust, Edge, Grab, PopupAnchor, PopupConfig, PopupSize, PositionerParams, positioner_params,
    to_wire_anchor, to_wire_constraint_adjustment, to_wire_gravity,
};
use shell_host::region::Rect;
use shell_host::scale::{self, Scale};

const BAR_HEIGHT: i32 = 32;
const BUTTON: Rect = Rect {
    x: 20,
    y: 4,
    width: 80,
    height: 24,
};
const SUBMENU: Rect = Rect {
    x: 16,
    y: 44,
    width: 148,
    height: 28,
};
const POPUP1_SIZE: (i32, i32) = (180, 110);
const POPUP2_SIZE: (i32, i32) = (160, 90);

// ARGB8888, little-endian byte order (B, G, R, A), matching sctk's own examples.
const BAR_BG: [u8; 4] = [0x33, 0x33, 0x33, 0xE0];
const BUTTON_COLOR: [u8; 4] = [0x40, 0xA0, 0xE0, 0xFF];
const POPUP1_BG: [u8; 4] = [0x20, 0x60, 0x20, 0xF0];
const SUBMENU_COLOR: [u8; 4] = [0x40, 0xD0, 0x40, 0xFF];
const POPUP2_BG: [u8; 4] = [0x60, 0x20, 0x60, 0xF0];

fn main() {
    let conn =
        Connection::connect_to_env().expect("no Wayland connection: is WAYLAND_DISPLAY set?");
    let (globals, event_queue) = registry_queue_init(&conn).expect("registry");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell is not available");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg-shell is not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm is not available");
    let background_effect =
        BackgroundEffect::bind(&globals, &qh).expect("ext-background-effect-v1 is not available");
    let mut fractional = Fractional::bind(&globals, &qh)
        .expect("wp-fractional-scale-v1 or wp-viewporter is not available");

    let surface = compositor.create_surface(&qh);
    let bar = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Top,
        Some("shell-host-spike-popup"),
        None,
    );
    bar.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
    bar.set_size(0, BAR_HEIGHT as u32);
    bar.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    fractional.watch(&qh, bar.wl_surface());
    bar.commit();

    let pool = SlotPool::new(4 * 1024 * 1024, &shm).expect("failed to create the shm pool");

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        compositor,
        xdg_shell,
        background_effect,
        fractional,
        pool,
        qh: qh.clone(),
        bar,
        bar_size: (256, BAR_HEIGHT),
        bar_scale: Scale::ONE,
        bar_first_configure: true,
        popups: Vec::new(),
        keyboard: None,
        pointer: None,
        keyboard_focus: None,
        exit: false,
    };

    let mut event_loop: EventLoop<App> =
        EventLoop::try_new().expect("failed to create the event loop");
    let handle = event_loop.handle();
    WaylandSource::new(conn, event_queue)
        .insert(handle.clone())
        .expect("failed to insert the Wayland source");

    let (sender, receiver) = channel::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    handle
        .insert_source(receiver, |event, _, app: &mut App| {
            if let channel::Event::Msg(line) = event {
                app.handle_command(&line);
            }
        })
        .expect("failed to insert the stdin source");

    let signal = event_loop.get_signal();
    println!("READY: bar mapped, waiting for stdin commands (click x y | capture path | quit)");
    event_loop
        .run(None, &mut app, |app| {
            // `wp_fractional_scale_v1.preferred_scale` lands in the Wayland dispatch above;
            // pick up any change here, once per loop iteration, rather than threading a
            // callback into `handlers::fractional`.
            app.update_preferred_scale();
            if app.exit {
                signal.stop();
            }
        })
        .expect("event loop error");
}

/// One popup in the (at most two deep) stack this example opens.
struct PopupLevel {
    popup: Popup,
    color: [u8; 4],
    size: (i32, i32),
    /// Where in this popup's own surface a click opens the next popup, if any.
    hot_rect: Option<Rect>,
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    background_effect: BackgroundEffect,
    fractional: Fractional,
    pool: SlotPool,
    qh: QueueHandle<App>,

    bar: LayerSurface,
    bar_size: (i32, i32),
    bar_scale: Scale,
    bar_first_configure: bool,

    popups: Vec<PopupLevel>,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard_focus: Option<wl_surface::WlSurface>,

    exit: bool,
}

impl App {
    fn handle_command(&mut self, line: &str) {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("click") => {
                let (Some(x), Some(y)) = (
                    parts.next().and_then(|s| s.parse().ok()),
                    parts.next().and_then(|s| s.parse().ok()),
                ) else {
                    println!("ERROR: usage: click <x> <y>");
                    return;
                };
                self.handle_click(x, y);
            }
            Some("capture") => {
                let Some(path) = parts.next() else {
                    println!("ERROR: usage: capture <path>");
                    return;
                };
                self.capture(path);
            }
            Some("quit") => self.exit = true,
            _ => println!("ERROR: unknown command {line:?}"),
        }
    }

    fn handle_click(&mut self, x: i32, y: i32) {
        let point = Rect {
            x,
            y,
            width: 1,
            height: 1,
        };
        match self.popups.len() {
            0 => {
                if rect_contains(BUTTON, point) {
                    self.open_popup1();
                } else {
                    println!("EVENT ClickIgnored surface=bar x={x} y={y} (outside button)");
                }
            }
            1 => {
                let hot = self.popups[0].hot_rect;
                if hot.is_some_and(|r| rect_contains(r, point)) {
                    self.open_popup2();
                } else {
                    self.dismiss_top("client");
                }
            }
            _ => self.dismiss_top("client"),
        }
    }

    fn open_popup1(&mut self) {
        let config = PopupConfig {
            anchor: PopupAnchor::Rect,
            size: PopupSize::Fixed,
            edge: Edge::Bottom,
            gravity: Edge::Bottom,
            adjust: Adjust::SLIDE_AND_FLIP,
            grab: Grab::Ungrabbed,
        };
        let params = positioner_params(
            &config,
            BUTTON,
            scale::Size {
                width: POPUP1_SIZE.0,
                height: POPUP1_SIZE.1,
            },
        );
        let qh = self.qh.clone();
        let positioner = build_positioner(&self.xdg_shell, params);
        let surface = self.compositor.create_surface(&qh);
        let popup = Popup::from_surface(None, &positioner, &qh, surface, &self.xdg_shell)
            .expect("failed to create the popup surface");
        self.bar.get_popup(popup.xdg_popup());
        popup.wl_surface().commit();
        log_positioner("popup1", &params);
        self.popups.push(PopupLevel {
            popup,
            color: POPUP1_BG,
            size: POPUP1_SIZE,
            hot_rect: Some(SUBMENU),
        });
    }

    fn open_popup2(&mut self) {
        let config = PopupConfig {
            anchor: PopupAnchor::Rect,
            size: PopupSize::Fixed,
            edge: Edge::Right,
            gravity: Edge::Right,
            adjust: Adjust::SLIDE_AND_FLIP,
            grab: Grab::Ungrabbed,
        };
        let params = positioner_params(
            &config,
            SUBMENU,
            scale::Size {
                width: POPUP2_SIZE.0,
                height: POPUP2_SIZE.1,
            },
        );
        let qh = self.qh.clone();
        let positioner = build_positioner(&self.xdg_shell, params);
        let parent_xdg_surface = self.popups[0].popup.xdg_surface().clone();
        let surface = self.compositor.create_surface(&qh);
        let popup = Popup::from_surface(
            Some(&parent_xdg_surface),
            &positioner,
            &qh,
            surface,
            &self.xdg_shell,
        )
        .expect("failed to create the nested popup surface");
        popup.wl_surface().commit();
        log_positioner("popup2 (submenu, nested in popup1)", &params);
        self.popups.push(PopupLevel {
            popup,
            color: POPUP2_BG,
            size: POPUP2_SIZE,
            hot_rect: None,
        });
    }

    fn dismiss_top(&mut self, reason: &str) {
        let Some(level) = self.popups.pop() else {
            return;
        };
        drop(level.popup); // sends xdg_popup.destroy / xdg_surface.destroy on Drop
        println!(
            "EVENT PopupDismissed reason={reason} remaining_open={}",
            self.popups.len()
        );
    }

    fn capture(&mut self, path: &str) {
        let status = Command::new("spectacle")
            .args(["-b", "-n", "-o", path])
            .status();
        match status {
            Ok(s) if s.success() => println!("EVENT Captured path={path}"),
            Ok(s) => println!("ERROR: spectacle exited with {s}"),
            Err(e) => println!("ERROR: failed to run spectacle: {e}"),
        }
    }

    fn repaint_bar(&mut self) {
        let (logical_w, logical_h) = self.bar_size;
        let physical_w = scale::logical_to_physical(logical_w, self.bar_scale);
        let physical_h = scale::logical_to_physical(logical_h, self.bar_scale);
        self.fractional
            .set_destination(self.bar.wl_surface(), logical_w, logical_h);
        let button = scale_rect(BUTTON, self.bar_scale);
        paint(
            &mut self.pool,
            self.bar.wl_surface(),
            physical_w,
            physical_h,
            BAR_BG,
            &[(button, BUTTON_COLOR)],
        );
        println!(
            "EVENT BarPainted logical=({logical_w},{logical_h}) scale={} buffer=({physical_w},{physical_h})",
            self.bar_scale.0
        );
    }

    fn paint_popup(&mut self, popup: &Popup) {
        let Some(level) = self.popups.iter().find(|l| &l.popup == popup) else {
            return;
        };
        let (w, h) = level.size;
        let highlight = level.hot_rect.map(|r| (r, SUBMENU_COLOR));
        let highlights: &[(Rect, [u8; 4])] = highlight.as_slice_of_one();
        paint(
            &mut self.pool,
            popup.wl_surface(),
            w,
            h,
            level.color,
            highlights,
        );
        println!(
            "EVENT PopupPainted size=({w},{h}) color={:02x?} hot_rect={:?}",
            level.color, level.hot_rect
        );
    }

    fn update_preferred_scale(&mut self) {
        let Some(scale) = self.fractional.preferred_scale(self.bar.wl_surface()) else {
            return;
        };
        if scale == self.bar_scale {
            return;
        }
        self.bar_scale = scale;
        println!(
            "EVENT PreferredScale surface=bar scale={} ({:.3}x)",
            scale.0,
            scale.as_f64()
        );
        if !self.bar_first_configure {
            self.repaint_bar();
        }
    }
}

/// A one-or-zero-element slice, so `paint`'s `highlights` parameter does not need an
/// allocation for the common case of a single highlight rect.
trait AsSliceOfOne<T> {
    fn as_slice_of_one(&self) -> &[T];
}
impl<T> AsSliceOfOne<T> for Option<T> {
    fn as_slice_of_one(&self) -> &[T] {
        self.as_ref().map_or(&[], std::slice::from_ref)
    }
}

fn rect_contains(rect: Rect, point: Rect) -> bool {
    point.x >= rect.x
        && point.x < rect.x + rect.width
        && point.y >= rect.y
        && point.y < rect.y + rect.height
}

fn scale_rect(rect: Rect, scale: Scale) -> Rect {
    Rect {
        x: scale::logical_to_physical(rect.x, scale),
        y: scale::logical_to_physical(rect.y, scale),
        width: scale::logical_to_physical(rect.width, scale),
        height: scale::logical_to_physical(rect.height, scale),
    }
}

fn build_positioner(xdg_shell: &XdgShell, params: PositionerParams) -> XdgPositioner {
    let positioner = XdgPositioner::new(xdg_shell).expect("xdg_wm_base is bound");
    positioner.set_size(params.size.width, params.size.height);
    positioner.set_anchor_rect(
        params.anchor_rect.x,
        params.anchor_rect.y,
        params.anchor_rect.width,
        params.anchor_rect.height,
    );
    positioner.set_anchor(to_wire_anchor(params.anchor));
    positioner.set_gravity(to_wire_gravity(params.gravity));
    positioner.set_constraint_adjustment(to_wire_constraint_adjustment(params.adjust));
    positioner
}

fn log_positioner(name: &str, params: &PositionerParams) {
    println!(
        "EVENT PositionerParams popup={name} size=({},{}) anchor_rect=({},{},{},{}) anchor={:?} gravity={:?}",
        params.size.width,
        params.size.height,
        params.anchor_rect.x,
        params.anchor_rect.y,
        params.anchor_rect.width,
        params.anchor_rect.height,
        params.anchor,
        params.gravity,
    );
}

fn paint(
    pool: &mut SlotPool,
    surface: &wl_surface::WlSurface,
    width: i32,
    height: i32,
    base: [u8; 4],
    highlights: &[(Rect, [u8; 4])],
) {
    let width = width.max(1);
    let height = height.max(1);
    let stride = width * 4;
    let (buffer, canvas) = pool
        .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        .expect("create buffer");
    for chunk in canvas.as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&base);
    }
    for (rect, color) in highlights {
        let x0 = rect.x.max(0);
        let y0 = rect.y.max(0);
        let x1 = (rect.x + rect.width).min(width);
        let y1 = (rect.y + rect.height).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let idx = ((y * width + x) * 4) as usize;
                canvas[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
    surface.damage_buffer(0, 0, width, height);
    buffer.attach_to(surface).expect("buffer attach");
    surface.commit();
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let width = if configure.new_size.0 == 0 {
            self.bar_size.0
        } else {
            configure.new_size.0 as i32
        };
        self.bar_size = (width, BAR_HEIGHT);

        let manager_caps = self.background_effect.capabilities();
        println!(
            "EVENT BackgroundEffectCapabilities blur={:?}",
            manager_caps.blur
        );
        let region = Region::new(&self.compositor).expect("wl_compositor bound");
        region.add(0, 0, width, BAR_HEIGHT);
        self.background_effect
            .set_blur_region(qh, self.bar.wl_surface(), Some(region.wl_region()));

        self.repaint_bar();
        self.bar_first_configure = false;
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        println!(
            "EVENT PopupConfigure position=({},{}) size=({},{}) serial={} kind={:?}",
            config.position.0,
            config.position.1,
            config.width,
            config.height,
            config.serial,
            config.kind
        );
        if let Some(level) = self.popups.iter_mut().find(|l| &l.popup == popup) {
            level.size = (config.width.max(1), config.height.max(1));
        }
        self.paint_popup(popup);
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        println!("EVENT PopupDone (compositor-initiated)");
        self.popups.retain(|l| &l.popup != popup);
        println!(
            "EVENT PopupDismissed reason=compositor remaining_open={}",
            self.popups.len()
        );
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
        if capability == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            p.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.keyboard_focus = Some(surface.clone());
        println!(
            "EVENT KeyboardEnter surface={:?}",
            surface_name(self, surface)
        );
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.keyboard_focus = None;
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let arrow = match event.keysym {
            Keysym::Left => Some("Left"),
            Keysym::Right => Some("Right"),
            Keysym::Up => Some("Up"),
            Keysym::Down => Some("Down"),
            _ => None,
        };
        if let Some(arrow) = arrow {
            let focus = self.keyboard_focus.as_ref().map(|s| surface_name(self, s));
            println!("EVENT ArrowKey key={arrow} focus={focus:?}");
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    println!(
                        "EVENT PointerEnter surface={:?} @{:?}",
                        surface_name(self, &event.surface),
                        event.position
                    );
                }
                PointerEventKind::Press { button, .. } => {
                    println!(
                        "EVENT PointerPress button={button:#x} surface={:?} @{:?}",
                        surface_name(self, &event.surface),
                        event.position
                    );
                }
                _ => {}
            }
        }
    }
}

/// A short, human-readable name for one of our own surfaces, for logging.
fn surface_name(app: &App, surface: &wl_surface::WlSurface) -> String {
    if surface == app.bar.wl_surface() {
        return "bar".to_string();
    }
    for (i, level) in app.popups.iter().enumerate() {
        if surface == level.popup.wl_surface() {
            return format!("popup{}", i + 1);
        }
    }
    "unknown".to_string()
}

// `delegate_xdg_shell!` also wires up the toplevel-decoration global, which requires
// `WindowHandler`, even though this example never creates a `Window`.
impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl AsMut<BackgroundEffect> for App {
    fn as_mut(&mut self) -> &mut BackgroundEffect {
        &mut self.background_effect
    }
}

impl AsMut<Fractional> for App {
    fn as_mut(&mut self) -> &mut Fractional {
        &mut self.fractional
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_xdg_shell!(App);
delegate_xdg_popup!(App);
delegate_registry!(App);

delegate_dispatch!(App: [ExtBackgroundEffectManagerV1: ()] => BackgroundEffect);
delegate_dispatch!(App: [ExtBackgroundEffectSurfaceV1: ()] => BackgroundEffect);
delegate_dispatch!(App: [WpFractionalScaleManagerV1: ()] => Fractional);
delegate_dispatch!(App: [WpFractionalScaleV1: ()] => Fractional);
delegate_dispatch!(App: [WpViewporter: ()] => Fractional);
delegate_dispatch!(App: [WpViewport: ()] => Fractional);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
