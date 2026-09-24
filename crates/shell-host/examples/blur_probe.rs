//! A plain `wl_shm` client that asks for compositor blur (`ext-background-effect-v1`) with none
//! of shell-host in it, to tell a compositor's blur bug from ours (FINDINGS F29). It maps either
//! a 32 px top layer surface (`--role=layer`, default) or a 400x300 xdg toplevel
//! (`--role=toplevel`), fills it with the demo bar's tint (magenta, alpha 0.55, premultiplied
//! ARGB8888; `--format=xrgb` for an opaque XRGB8888 buffer), and sets a blur region:
//! `--region=whole|rect|strips|none` (the whole surface, one 300x32 rect, or the strips of a
//! 12 px rounded rect), `--order=before|after` (with the first buffer's commit, or on a later
//! commit after the first frame callback). Exits on stdin EOF or `quit`.

use std::io::Read;
use std::os::fd::AsFd;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::{
    self, ExtBackgroundEffectSurfaceV1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    Whole,
    Rect,
    Strips,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Argb,
    Xrgb,
}

enum Role {
    Layer(LayerSurface),
    Toplevel(Window),
}

impl Role {
    fn wl_surface(&self) -> &WlSurface {
        match self {
            Role::Layer(layer) => layer.wl_surface(),
            Role::Toplevel(window) => window.wl_surface(),
        }
    }
}

struct Probe {
    role: Role,
    compositor: CompositorState,
    effect: ExtBackgroundEffectSurfaceV1,
    pool: SlotPool,
    shm: Shm,
    registry_state: RegistryState,
    output_state: OutputState,
    size: (u32, u32),
    region: RegionKind,
    order: Order,
    format: Format,
    frames: u32,
    exit: bool,
}

fn arg<'a>(args: &'a [String], name: &str, default: &'a str) -> &'a str {
    args.iter()
        .find_map(|a| a.strip_prefix(name))
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("blur_probe: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let region = match arg(args, "--region=", "whole") {
        "whole" => RegionKind::Whole,
        "rect" => RegionKind::Rect,
        "strips" => RegionKind::Strips,
        "none" => RegionKind::None,
        other => return Err(format!("--region={other}")),
    };
    let order = match arg(args, "--order=", "before") {
        "before" => Order::Before,
        "after" => Order::After,
        other => return Err(format!("--order={other}")),
    };
    let format = match arg(args, "--format=", "argb") {
        "argb" => Format::Argb,
        "xrgb" => Format::Xrgb,
        other => return Err(format!("--format={other}")),
    };
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, queue) = registry_queue_init::<Probe>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let manager: ExtBackgroundEffectManagerV1 =
        globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let surface = compositor.create_surface(&qh);
    let effect = manager.get_background_effect(&surface, &qh, ());
    let (role, size) = match arg(args, "--role=", "layer") {
        "layer" => {
            let shell = LayerShell::bind(&globals, &qh).map_err(|e| e.to_string())?;
            let layer =
                shell.create_layer_surface(&qh, surface, Layer::Top, Some("blur_probe"), None);
            layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
            layer.set_size(0, 32);
            layer.commit();
            (Role::Layer(layer), (0, 32))
        }
        "toplevel" => {
            let xdg = XdgShell::bind(&globals, &qh).map_err(|e| e.to_string())?;
            let window = xdg.create_window(surface, WindowDecorations::RequestClient, &qh);
            window.set_title("blur_probe");
            window.commit();
            (Role::Toplevel(window), (400, 300))
        }
        other => return Err(format!("--role={other}")),
    };
    let mut probe = Probe {
        role,
        compositor,
        effect,
        pool: SlotPool::new(256 * 256 * 4, &shm).map_err(|e| e.to_string())?,
        shm,
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        size,
        region,
        order,
        format,
        frames: 0,
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
    while !probe.exit {
        event_loop
            .dispatch(None, &mut probe)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl Probe {
    fn set_region(&self) {
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        let rects: Vec<(i32, i32, i32, i32)> = match self.region {
            RegionKind::Whole => vec![(0, 0, w, h)],
            RegionKind::Rect => vec![(0, 0, 300.min(w), h)],
            RegionKind::Strips => vec![
                (4, 0, w - 8, 1),
                (2, 1, w - 4, 2),
                (1, 3, w - 2, 1),
                (0, 4, w, h - 8),
                (1, h - 4, w - 2, 1),
                (2, h - 3, w - 4, 2),
                (4, h - 1, w - 8, 1),
            ],
            RegionKind::None => vec![],
        };
        if rects.is_empty() {
            return;
        }
        let Ok(region) = Region::new(&self.compositor) else {
            return;
        };
        for (x, y, rw, rh) in rects {
            region.add(x, y, rw, rh);
        }
        self.effect.set_blur_region(Some(region.wl_region()));
        println!("blur region set ({:?}, {:?})", self.region, self.order);
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (w, h) = self.size;
        let (format, pixel) = match self.format {
            // rgba(255, 0, 128, 0.55) premultiplied, little-endian BGRA bytes.
            Format::Argb => (wl_shm::Format::Argb8888, [70u8, 0, 140, 140]),
            Format::Xrgb => (wl_shm::Format::Xrgb8888, [128u8, 0, 255, 255]),
        };
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(w as i32, h as i32, w as i32 * 4, format)
        else {
            return;
        };
        for px in canvas.as_chunks_mut::<4>().0.iter_mut() {
            px.copy_from_slice(&pixel);
        }
        let surface = self.role.wl_surface().clone();
        if self.frames == 0 && self.order == Order::Before {
            self.set_region();
        }
        surface.damage_buffer(0, 0, w as i32, h as i32);
        let _ = buffer.attach_to(&surface);
        surface.frame(qh, surface.clone());
        surface.commit();
    }
}

impl LayerShellHandler for Probe {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        cfg: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.size = (cfg.new_size.0.max(1), cfg.new_size.1.max(1));
        println!("configure {}x{}", self.size.0, self.size.1);
        self.draw(qh);
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
        self.size = (
            cfg.new_size.0.map_or(400, |v| v.get()),
            cfg.new_size.1.map_or(300, |v| v.get()),
        );
        println!("configure {}x{}", self.size.0, self.size.1);
        self.draw(qh);
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
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        self.frames += 1;
        println!("frame callback {}", self.frames);
        if self.frames == 1 && self.order == Order::After {
            self.set_region();
            surface.commit();
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

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Probe {
    fn event(
        _: &mut Probe,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Probe>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            println!("capabilities {flags:?}");
        }
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for Probe {
    fn event(
        _: &mut Probe,
        _: &ExtBackgroundEffectSurfaceV1,
        _: ext_background_effect_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Probe>,
    ) {
    }
}

delegate_compositor!(Probe);
delegate_output!(Probe);
delegate_shm!(Probe);
delegate_layer!(Probe);
delegate_xdg_shell!(Probe);
delegate_xdg_window!(Probe);
delegate_registry!(Probe);
