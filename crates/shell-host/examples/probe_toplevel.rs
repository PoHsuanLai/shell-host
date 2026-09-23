//! The acceptance probe (`scripts/spike-accept.sh demo`): an ordinary xdg toplevel, not a
//! shell-host surface, that asks to be maximized (or `--fullscreen`), paints a checkerboard of
//! `--cell` physical px (default 2) at the preferred fractional scale, and prints every configure
//! as a JSON line. Items a and b read the work area and the pixels behind the bar from it; item
//! c covers the dock with it. It stands in for "some other app's window", so it talks to the
//! compositor directly (sctk, `wl_shm`) and exits on `quit` or after 60 s.

use std::io::{Read, Write};
use std::os::fd::AsFd;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    self, WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("probe_toplevel: {e}");
        std::process::exit(1);
    }
}

/// The buffer size for a logical size at a scale in 120ths (round half up).
fn buffer_size(logical: (u32, u32), scale120: u32) -> (u32, u32) {
    let f = |v: u32| ((u64::from(v) * u64::from(scale120) + 60) / 120) as u32;
    (f(logical.0).max(1), f(logical.1).max(1))
}

struct Probe {
    window: Window,
    viewport: Option<WpViewport>,
    pool: SlotPool,
    shm: Shm,
    registry_state: RegistryState,
    output_state: OutputState,
    logical: (u32, u32),
    scale120: u32,
    cell: u32,
    configured: bool,
    exit: bool,
}

/// Checkerboard pixel (XRGB8888, little endian bytes) at buffer coords.
fn checker(x: u32, y: u32, cell: u32) -> [u8; 4] {
    if ((x / cell) + (y / cell)).is_multiple_of(2) {
        [0, 0, 0, 255]
    } else {
        [255, 255, 255, 255]
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, queue) = registry_queue_init::<Probe>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let xdg = XdgShell::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let fractional: Option<WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
    let surface = compositor.create_surface(&qh);
    let _fs = fractional
        .as_ref()
        .map(|m| m.get_fractional_scale(&surface, &qh, ()));
    let viewport = viewporter
        .as_ref()
        .map(|v| v.get_viewport(&surface, &qh, ()));
    let window = xdg.create_window(surface, WindowDecorations::RequestClient, &qh);
    window.set_title("probe_toplevel");
    window.set_app_id("shell-host.probe");
    if args.iter().any(|a| a == "--fullscreen") {
        window.set_fullscreen(None);
    } else {
        window.set_maximized();
    }
    window.commit();
    let cell = args
        .iter()
        .find_map(|a| a.strip_prefix("--cell="))
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let mut probe = Probe {
        window,
        viewport,
        pool: SlotPool::new(256 * 256 * 4, &shm).map_err(|e| e.to_string())?,
        shm,
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        logical: (256, 256),
        scale120: 120,
        cell,
        configured: false,
        exit: false,
    };
    let mut event_loop: calloop::EventLoop<Probe> =
        calloop::EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| e.to_string())?;
    let stdin = std::fs::File::from(
        std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|e| e.to_string())?,
    );
    handle
        .insert_source(
            calloop::generic::Generic::new(stdin, calloop::Interest::READ, calloop::Mode::Level),
            |_, file, probe: &mut Probe| {
                let mut buf = [0u8; 256];
                let n = (&**file).read(&mut buf).unwrap_or(0);
                if n == 0 || String::from_utf8_lossy(&buf[..n]).contains("quit") {
                    probe.exit = true;
                }
                Ok(calloop::PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(
            calloop::timer::Timer::from_duration(std::time::Duration::from_secs(60)),
            |_, _, probe| {
                probe.exit = true;
                calloop::timer::TimeoutAction::Drop
            },
        )
        .map_err(|e| e.to_string())?;
    while !probe.exit {
        event_loop
            .dispatch(None, &mut probe)
            .map_err(|e| e.to_string())?;
    }
    drop(probe);
    let _ = conn.flush();
    Ok(())
}

impl Probe {
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (w, h) = buffer_size(self.logical, self.scale120);
        let stride = w as i32 * 4;
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(w as i32, h as i32, stride, wl_shm::Format::Xrgb8888)
        else {
            return;
        };
        for (i, px) in canvas.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            px.copy_from_slice(&checker(x, y, self.cell));
        }
        let surface = self.window.wl_surface();
        match &self.viewport {
            Some(vp) => vp.set_destination(self.logical.0 as i32, self.logical.1 as i32),
            None => surface.set_buffer_scale(1),
        }
        surface.damage_buffer(0, 0, w as i32, h as i32);
        let _ = buffer.attach_to(surface);
        let _ = qh;
        surface.commit();
    }
}

impl WindowHandler for Probe {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &Window,
        cfg: WindowConfigure,
        _: u32,
    ) {
        let w = cfg.new_size.0.map_or(256, |v| v.get());
        let h = cfg.new_size.1.map_or(256, |v| v.get());
        self.logical = (w, h);
        self.configured = true;
        let line = serde_json::json!({
            "event": "configure",
            "width": w,
            "height": h,
            "sized": cfg.new_size.1.is_some(),
            "maximized": cfg.is_maximized(),
            "fullscreen": cfg.is_fullscreen(),
            "bounds": cfg.suggested_bounds,
            "scale120": self.scale120,
        });
        println!("{line}");
        let _ = std::io::stdout().flush();
        self.draw(qh);
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for Probe {
    fn event(
        probe: &mut Probe,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Probe>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            println!(
                "{}",
                serde_json::json!({"event": "preferred_scale", "scale120": scale})
            );
            let _ = std::io::stdout().flush();
            probe.scale120 = scale;
        }
    }
}

impl CompositorHandler for Probe {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}
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

impl OutputHandler for Probe {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Probe {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Probe {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_compositor!(Probe);
delegate_output!(Probe);
delegate_shm!(Probe);
delegate_xdg_shell!(Probe);
delegate_xdg_window!(Probe);
delegate_registry!(Probe);
delegate_noop!(Probe: ignore WpFractionalScaleManagerV1);
delegate_noop!(Probe: ignore WpViewporter);
delegate_noop!(Probe: ignore WpViewport);
