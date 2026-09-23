//! The spike host: sctk state, calloop loop, stdin commands, frame machine.

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blitz_dom::Document;
use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, Mode, PostAction};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_surface::WlSurface};
use wayland_client::{Connection, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use super::cmd::{self, Cmd, Target};
use super::doc::{self, Content};
use super::gpu::{self, DEVICES_CREATED, Gpu};
use super::protocols::{BlurSink, ScaleSink, buffer_size};
use super::render::{animating, present};
use super::stats::{Facts, Stats};
use super::surface::{Pace, SpikeSurface};

/// Command-line options.
#[derive(Debug, Clone)]
pub struct Options {
    pub surfaces: Vec<Content>,
    pub blur: bool,
    pub fractional: bool,
    pub layer: Layer,
    pub exclusive: i32,
    pub timeout: Duration,
}

/// `spike_bar [--dock|--with-dock] [--mode=transform|width] [--layer=top|overlay|bottom]
/// [--exclusive=N]`; env `SPIKE_BLUR=1`, `SPIKE_NO_FRACTIONAL=1`, `SPIKE_TIMEOUT=secs`.
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(&format!("{name}=")).map(str::to_owned))
    };
    let dock = match value("--mode").as_deref() {
        None | Some("transform") => Content::DockTransform,
        Some("width") => Content::DockWidth,
        Some(m) => return Err(format!("unknown --mode={m}")),
    };
    let surfaces = if flag("--with-dock") {
        vec![Content::Bar, dock]
    } else if flag("--dock") {
        vec![dock]
    } else {
        vec![Content::Bar]
    };
    let layer = match value("--layer").as_deref() {
        None | Some("top") => Layer::Top,
        Some("overlay") => Layer::Overlay,
        Some("bottom") => Layer::Bottom,
        Some(l) => return Err(format!("unknown --layer={l}")),
    };
    let exclusive = value("--exclusive")
        .map(|v| v.parse().map_err(|e| format!("--exclusive: {e}")))
        .transpose()?
        .unwrap_or(32);
    let env_on = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
    let timeout = std::env::var("SPIKE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    Ok(Options {
        surfaces,
        blur: env_on("SPIKE_BLUR"),
        fractional: !env_on("SPIKE_NO_FRACTIONAL"),
        layer,
        exclusive,
        timeout: Duration::from_secs(timeout),
    })
}

pub struct State {
    // Surfaces first: they must drop before the globals and the connection.
    pub surfaces: Vec<SpikeSurface>,
    pub gpu: Option<Gpu>,
    pub instance: wgpu::Instance,
    pub registry_state: RegistryState,
    pub output_state: OutputState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub fractional: Option<WpFractionalScaleManagerV1>,
    pub viewporter: Option<WpViewporter>,
    pub blur_mgr: Option<ExtBackgroundEffectManagerV1>,
    pub blur_caps: Option<u32>,
    pub conn: Connection,
    pub opts: Options,
    pub stdin_buf: String,
    pub exit: bool,
}

/// Runs the spike until `quit`, stdin EOF, the layer closing, or the timeout.
pub fn run(opts: Options) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let fractional = opts
        .fractional
        .then(|| globals.bind(&qh, 1..=1, ()).ok())
        .flatten();
    let viewporter = opts
        .fractional
        .then(|| globals.bind(&qh, 1..=1, ()).ok())
        .flatten();
    let blur_mgr = opts
        .blur
        .then(|| globals.bind(&qh, 1..=1, ()).ok())
        .flatten();
    eprintln!(
        "globals: fractional={} viewporter={} blur_mgr={}",
        fractional.is_some(),
        viewporter.is_some(),
        blur_mgr.is_some()
    );

    let mut event_loop: EventLoop<State> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| e.to_string())?;
    let stdin = std::fs::File::from(
        std::io::stdin()
            .lock()
            .as_fd_owned()
            .map_err(|e| e.to_string())?,
    );
    handle
        .insert_source(
            Generic::new(stdin, Interest::READ, Mode::Level),
            |_, file, state: &mut State| {
                let mut buf = [0u8; 4096];
                let n = (&**file).read(&mut buf).unwrap_or(0);
                if n == 0 {
                    state.exit = true;
                    return Ok(PostAction::Remove);
                }
                state
                    .stdin_buf
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(Timer::from_duration(opts.timeout), |_, _, state| {
            eprintln!("SPIKE_TIMEOUT reached, exiting");
            state.exit = true;
            TimeoutAction::Drop
        })
        .map_err(|e| e.to_string())?;

    let mut state = State {
        surfaces: Vec::new(),
        gpu: None,
        instance: gpu::instance(&conn),
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        fractional,
        viewporter,
        blur_mgr,
        blur_caps: None,
        conn: conn.clone(),
        opts: opts.clone(),
        stdin_buf: String::new(),
        exit: false,
    };
    for content in &opts.surfaces {
        state.spawn(*content, &qh);
    }
    while !state.exit {
        event_loop
            .dispatch(None, &mut state)
            .map_err(|e| e.to_string())?;
        state.run_commands(&qh);
        state.flush(&qh);
        if let Some(err) = state.conn.protocol_error() {
            state.surfaces.clear();
            return Err(format!("wayland protocol error: {err:?}"));
        }
    }
    state.teardown();
    Ok(())
}

trait AsFdOwned {
    fn as_fd_owned(&self) -> std::io::Result<std::os::fd::OwnedFd>;
}

impl<T: std::os::fd::AsFd> AsFdOwned for T {
    fn as_fd_owned(&self) -> std::io::Result<std::os::fd::OwnedFd> {
        self.as_fd().try_clone_to_owned()
    }
}

impl State {
    fn spawn(&mut self, content: Content, qh: &QueueHandle<Self>) {
        let (name, anchor, exclusive, layer) = match content {
            Content::Bar => (
                "bar",
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                self.opts.exclusive,
                self.opts.layer,
            ),
            _ => ("dock", Anchor::BOTTOM, 0, self.opts.layer),
        };
        let wl = self.compositor.create_surface(qh);
        let fractional = self
            .fractional
            .as_ref()
            .map(|m| m.get_fractional_scale(&wl, qh, wl.clone()));
        let viewport = self
            .viewporter
            .as_ref()
            .map(|v| v.get_viewport(&wl, qh, ()));
        let blur = self
            .blur_mgr
            .as_ref()
            .map(|m| m.get_background_effect(&wl, qh, ()));
        let surface = self.layer_shell.create_layer_surface(
            qh,
            wl,
            layer,
            Some(format!("spike-{name}")),
            None,
        );
        let size = doc::logical_size(content);
        surface.set_anchor(anchor);
        surface.set_size(size.0, size.1);
        surface.set_exclusive_zone(exclusive);
        surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        surface.commit();
        let redraw = Arc::new(AtomicBool::new(false));
        self.surfaces.push(SpikeSurface {
            wgpu: None,
            config: None,
            blur,
            fractional,
            viewport,
            layer: surface,
            name,
            content,
            doc: doc::build(content, redraw.clone()),
            scene: vello_hybrid::Scene::new(1, 1),
            redraw,
            logical: size,
            scale120: 120,
            buffer: size,
            configured: false,
            pace: Pace::Idle,
            clock: Instant::now(),
            stats: Stats::default(),
        });
    }

    fn index_of(&self, wl: &WlSurface) -> Option<usize> {
        self.surfaces.iter().position(|s| s.wl_surface() == wl)
    }

    fn target(&self, t: Target) -> Option<usize> {
        let want = match t {
            Target::Primary => return (!self.surfaces.is_empty()).then_some(0),
            Target::Bar => "bar",
            Target::Dock => "dock",
        };
        self.surfaces.iter().position(|s| s.name == want)
    }

    /// Applies a new logical size or scale: viewport, blur region, swapchain, doc viewport.
    fn apply_geometry(&mut self, i: usize) {
        let s = &mut self.surfaces[i];
        s.buffer = buffer_size(s.logical, s.scale120);
        match &s.viewport {
            Some(vp) => vp.set_destination(s.logical.0 as i32, s.logical.1 as i32),
            None => s
                .wl_surface()
                .set_buffer_scale((s.scale120 / 120).max(1) as i32),
        }
        if let (Some(blur), Ok(region)) = (&s.blur, Region::new(&self.compositor)) {
            region.add(0, 0, s.logical.0 as i32, s.logical.1 as i32);
            blur.set_blur_region(Some(region.wl_region()));
        }
        let scale = s.scale();
        s.doc
            .inner_mut()
            .set_viewport(doc::viewport(s.buffer, scale));
        if s.wgpu.is_none() {
            match super::raw_handle::create_surface(&self.instance, &self.conn, s.wl_surface()) {
                Ok(w) => s.wgpu = Some(w),
                Err(e) => {
                    eprintln!("[{}] create_surface: {e}", s.name);
                    self.exit = true;
                    return;
                }
            }
        }
        if self.gpu.is_none() {
            let Some(w) = &s.wgpu else { return };
            match Gpu::new(&self.instance, w) {
                Ok(g) => {
                    eprintln!("adapter: {}", g.adapter_name);
                    self.gpu = Some(g);
                }
                Err(e) => {
                    eprintln!("gpu: {e}");
                    self.exit = true;
                    return;
                }
            }
        }
        if let Some(gpu) = &self.gpu {
            s.configure_swapchain(gpu);
        }
        s.redraw.store(true, Ordering::SeqCst);
    }

    /// The frame machine's "something changed" edge.
    fn request(&mut self, i: usize, qh: &QueueHandle<Self>) {
        let s = &mut self.surfaces[i];
        if !s.configured {
            return;
        }
        match s.pace {
            Pace::Idle => {
                if let Some(gpu) = self.gpu.as_mut() {
                    present(gpu, s, qh);
                    s.redraw.store(false, Ordering::SeqCst);
                }
            }
            Pace::InFlight => s.pace = Pace::InFlightDirty,
            Pace::InFlightDirty => {}
        }
    }

    /// After every dispatch: flush Dioxus work, then feed redraw requests to the frame machine.
    fn flush(&mut self, qh: &QueueHandle<Self>) {
        for i in 0..self.surfaces.len() {
            if self.surfaces[i].doc.poll(None) {
                self.surfaces[i].redraw.store(true, Ordering::SeqCst);
            }
            if self.surfaces[i].redraw.swap(false, Ordering::SeqCst) {
                self.request(i, qh);
            }
        }
        let _ = self.conn.flush();
    }

    fn run_commands(&mut self, qh: &QueueHandle<Self>) {
        while let Some(pos) = self.stdin_buf.find('\n') {
            let line: String = self.stdin_buf.drain(..=pos).collect();
            if line.trim().is_empty() {
                continue;
            }
            match cmd::parse(&line) {
                Ok(c) => self.command(c, qh),
                Err(e) => eprintln!("{e}"),
            }
        }
    }

    fn command(&mut self, c: Cmd, _qh: &QueueHandle<Self>) {
        match c {
            Cmd::Hover(t, x, y) => self.events(t, doc::hover_events(x, y)),
            Cmd::Click(t, x, y) => self.events(t, doc::click_events(x, y)),
            Cmd::Leave(t) => {
                if let Some(i) = self.target(t) {
                    let s = &mut self.surfaces[i];
                    if s.doc.inner_mut().clear_hover() {
                        s.redraw.store(true, Ordering::SeqCst);
                    }
                }
            }
            Cmd::Stats => self.print_stats(),
            Cmd::Capture(t, path) => {
                let (Some(i), Some(gpu)) = (self.target(t), self.gpu.as_mut()) else {
                    return;
                };
                let r = super::capture::capture(gpu, &mut self.surfaces[i], &path);
                println!(
                    "{}",
                    serde_json::json!({"capture": path, "ok": r.is_ok(), "err": r.err()})
                );
                let _ = std::io::stdout().flush();
            }
            Cmd::Quit => self.exit = true,
        }
    }

    fn events(&mut self, t: Target, events: Vec<blitz_traits::events::UiEvent>) {
        let Some(i) = self.target(t) else { return };
        for e in events {
            self.surfaces[i].doc.handle_ui_event(e);
        }
        if std::env::var_os("SPIKE_DEBUG").is_some() {
            let s = &self.surfaces[i];
            eprintln!(
                "[{}] after events: redraw={} pace={:?} hover={:?}",
                s.name,
                s.redraw.load(Ordering::SeqCst),
                s.pace,
                s.doc.inner().get_hover_node_id()
            );
        }
    }

    fn print_stats(&mut self) {
        let adapter = self
            .gpu
            .as_ref()
            .map(|g| g.adapter_name.clone())
            .unwrap_or_default();
        let blur = match (&self.blur_mgr, self.blur_caps) {
            (None, _) if self.opts.blur => "unavailable".to_owned(),
            (None, _) => "off".to_owned(),
            (Some(_), caps) => format!("on caps={caps:?}"),
        };
        let mut out = std::io::stdout().lock();
        for s in &mut self.surfaces {
            let facts = Facts {
                surface: s.name,
                scale120: s.scale120,
                logical: s.logical,
                buffer: s.buffer,
                alpha_mode: s
                    .config
                    .as_ref()
                    .map(|c| format!("{:?}", c.alpha_mode))
                    .unwrap_or_default(),
                present_mode: s
                    .config
                    .as_ref()
                    .map(|c| format!("{:?}", c.present_mode))
                    .unwrap_or_default(),
                adapter: &adapter,
                devices_created: DEVICES_CREATED.load(Ordering::SeqCst),
                blur: &blur,
            };
            let _ = writeln!(out, "{}", s.stats.report(&facts));
        }
        let _ = out.flush();
    }

    /// Destroys every surface (wgpu swapchain first) and flushes, so nothing lingers on screen.
    pub fn teardown(&mut self) {
        self.surfaces.clear();
        let _ = self.conn.flush();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.teardown();
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        wl: &WlSurface,
        factor: i32,
    ) {
        // Integer fallback only: with fractional-scale the preferred_scale event wins.
        if self.fractional.is_some() {
            return;
        }
        if let Some(i) = self.index_of(wl) {
            self.surfaces[i].scale120 = factor.max(1) as u32 * 120;
            if self.surfaces[i].configured {
                self.apply_geometry(i);
                self.flush(qh);
            }
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, wl: &WlSurface, _: u32) {
        let Some(i) = self.index_of(wl) else { return };
        let s = &mut self.surfaces[i];
        s.stats.callbacks += 1;
        let again = s.pace == Pace::InFlightDirty || animating(s);
        s.pace = Pace::Idle;
        if again {
            self.request(i, qh);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        cfg: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(i) = self.index_of(layer.wl_surface()) else {
            return;
        };
        let s = &mut self.surfaces[i];
        let requested = doc::logical_size(s.content);
        let pick = |got: u32, want: u32| if got == 0 { want.max(1) } else { got };
        s.logical = (
            pick(cfg.new_size.0, requested.0),
            pick(cfg.new_size.1, requested.1),
        );
        eprintln!(
            "[{}] configure logical={:?} scale120={}",
            s.name, s.logical, s.scale120
        );
        s.configured = true;
        self.apply_geometry(i);
        self.flush(qh);
    }
}

impl ScaleSink for State {
    fn preferred_scale(&mut self, wl: &WlSurface, scale120: u32) {
        let Some(i) = self.index_of(wl) else { return };
        eprintln!("[{}] preferred_scale {scale120}", self.surfaces[i].name);
        if self.surfaces[i].scale120 != scale120 {
            self.surfaces[i].scale120 = scale120;
            if self.surfaces[i].configured {
                self.apply_geometry(i);
            }
        }
    }
}

impl BlurSink for State {
    fn blur_capabilities(&mut self, bits: u32) {
        eprintln!("ext_background_effect capabilities = {bits:#x}");
        self.blur_caps = Some(bits);
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_layer!(State);
delegate_registry!(State);
crate::delegate_spike_protocols!(State);
