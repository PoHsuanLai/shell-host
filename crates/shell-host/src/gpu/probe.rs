//! The first-present probe behind `adapter::validate`: a throwaway Wayland
//! connection with one 1x1 transparent surface, mapped as a layer surface where the compositor
//! offers layer-shell, presented once through the candidate adapter. A rejected buffer is a
//! fatal protocol error, and it kills this connection instead of the host's.
//!
//! See FINDINGS F16.

use std::sync::Mutex;

use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{
    self, ZwlrLayerShellV1,
};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, ZwlrLayerSurfaceV1,
};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};

use crate::error::{HostError, Result};
use crate::gpu::context::{SURFACE_FORMAT, enumerate, with_backends};
use crate::gpu::raw_handle::{self, WaylandDisplay};
use crate::gpu::swapchain::{choose_alpha, choose_present};
use blitz_kit::adapter::AdapterFacts;
use blitz_kit::adapter::block_on;

const NAMESPACE: &str = "shell-host-adapter-probe";

/// Where the probe surface is in its mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapping {
    Waiting,
    Configured,
    Closed,
}

#[derive(Debug)]
struct Probe {
    mapping: Mapping,
}

/// Present one frame through `adapter` on a throwaway connection; `AdapterRejected` when the
/// adapter cannot present there or the compositor kills the connection for it.
pub(crate) fn first_present(host_adapter: &wgpu::Adapter) -> Result<()> {
    let label = AdapterFacts::of(host_adapter).label();
    let conn = Connection::connect_to_env().map_err(|e| HostError::Connect(e.to_string()))?;
    let (globals, mut queue) =
        registry_queue_init::<Probe>(&conn).map_err(|e| HostError::Connect(e.to_string()))?;
    let qh = queue.handle();
    let compositor: WlCompositor =
        globals
            .bind(&qh, 4..=6, ())
            .map_err(|_| HostError::MissingGlobal {
                global: "wl_compositor",
            })?;
    let surface = compositor.create_surface(&qh, ());
    let region = compositor.create_region(&qh, ());
    surface.set_input_region(Some(&region));
    region.destroy();
    let layer = globals
        .bind::<ZwlrLayerShellV1, _, _>(&qh, 1..=4, ())
        .ok()
        .map(|shell| map_layer(&shell, &surface, &qh));

    let mut probe = Probe {
        mapping: match layer {
            Some(_) => Mapping::Waiting,
            // A surface without a role is never configured; its buffers are still imported.
            None => Mapping::Configured,
        },
    };
    surface.commit();
    // Kept alive with `conn` once the probe is done (`RETAINED`).
    let instance = probe_instance(&conn);
    let result = await_configure(&mut queue, &mut probe)
        .and_then(|()| same_adapter(&instance, host_adapter))
        .and_then(|adapter| present_once(&instance, &adapter, &conn, &surface))
        .and_then(|()| settle(&conn, &mut queue, &mut probe));

    if let Some(layer) = layer {
        layer.destroy();
    }
    surface.destroy();
    let _ = conn.flush();
    retain(instance, conn.clone());
    result.map_err(|reason| HostError::AdapterRejected {
        adapter: label,
        reason,
    })
}

/// Probe instances, and the connections whose displays they point at, kept for the life of the
/// process. Dropping a Vulkan instance that presented through NVIDIA 615.71 leaves the driver
/// unable to answer surface queries on the host's own instance: the next
/// `vkGetPhysicalDeviceSurfaceSupportKHR` jumps to a null pointer inside `libnvidia-glcore`
/// (reproduced on this machine; keeping only the connection alive does not help, keeping the
/// instance does). The cost is one idle instance and connection per validated candidate,
/// usually one, created once at startup.
static RETAINED: Mutex<Vec<(wgpu::Instance, Connection)>> = Mutex::new(Vec::new());

fn retain(instance: wgpu::Instance, conn: Connection) {
    // A poisoned lock only means another probe panicked while pushing; the list is still
    // a list of things to keep alive.
    let mut kept = RETAINED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.push((instance, conn));
}

/// An instance on the probe connection's display (wgpu refuses surfaces of another display).
fn probe_instance(conn: &Connection) -> wgpu::Instance {
    let desc = match WaylandDisplay::new(conn) {
        Some(display) => wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display)),
        None => wgpu::InstanceDescriptor::new_without_display_handle(),
    };
    wgpu::Instance::new(with_backends(desc))
}

/// The probe instance's adapter for the same device, driver and backend as `host_adapter`.
fn same_adapter(
    instance: &wgpu::Instance,
    host_adapter: &wgpu::Adapter,
) -> std::result::Result<wgpu::Adapter, String> {
    let wanted = host_adapter.get_info();
    enumerate(instance)
        .into_iter()
        .find(|a| a.get_info() == wanted)
        .ok_or_else(|| "the adapter is not enumerated on a second connection".into())
}

fn map_layer(
    shell: &ZwlrLayerShellV1,
    surface: &WlSurface,
    qh: &QueueHandle<Probe>,
) -> ZwlrLayerSurfaceV1 {
    let layer = shell.get_layer_surface(
        surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        NAMESPACE.to_owned(),
        qh,
        (),
    );
    layer.set_size(1, 1);
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    layer
}

/// The compositor answers the initial commit with a configure before the round trip's `done`.
fn await_configure(
    queue: &mut EventQueue<Probe>,
    probe: &mut Probe,
) -> std::result::Result<(), String> {
    for _ in 0..2 {
        if probe.mapping != Mapping::Waiting {
            break;
        }
        queue.roundtrip(probe).map_err(|e| e.to_string())?;
    }
    match probe.mapping {
        Mapping::Configured => Ok(()),
        Mapping::Waiting => Err("the probe surface was never configured".into()),
        Mapping::Closed => Err("the compositor closed the probe surface".into()),
    }
}

/// Configure a 1x1 swapchain on a throwaway device, clear it to transparent and present it.
fn present_once(
    instance: &wgpu::Instance,
    adapter: &wgpu::Adapter,
    conn: &Connection,
    surface: &WlSurface,
) -> std::result::Result<(), String> {
    let target = raw_handle::create_surface(instance, conn, surface).map_err(|e| e.to_string())?;
    if !adapter.is_surface_supported(&target) {
        return Err("the adapter cannot present to a Wayland surface".into());
    }
    let caps = target.get_capabilities(adapter);
    if !caps.formats.contains(&SURFACE_FORMAT) {
        return Err(format!(
            "{SURFACE_FORMAT:?} is not offered ({:?})",
            caps.formats
        ));
    }
    let (device, gpu_queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("shell-host adapter probe"),
        ..Default::default()
    }))
    .map_err(|e| format!("no device: {e}"))?;
    target.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            width: 1,
            height: 1,
            present_mode: choose_present(&caps.present_modes),
            desired_maximum_frame_latency: 2,
            alpha_mode: choose_alpha(&caps.alpha_modes),
            view_formats: vec![],
        },
    );
    let frame = match target.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        other => return Err(format!("acquire: {other:?}")),
    };
    let view = frame.texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    clear(&mut encoder, &view);
    gpu_queue.submit([encoder.finish()]);
    frame.present();
    // A one-off startup probe, not a frame: waiting here is what makes the present reach the
    // compositor before the round trip that looks for its verdict.
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("probe clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}

/// Round trip after the present and read the verdict: a rejected dmabuf arrives as a protocol
/// error before the sync's `done`.
fn settle(
    conn: &Connection,
    queue: &mut EventQueue<Probe>,
    probe: &mut Probe,
) -> std::result::Result<(), String> {
    let dispatched = queue.roundtrip(probe);
    if let Some(error) = conn.protocol_error() {
        return Err(format!(
            "protocol error {} on {}@{}: {}",
            error.code, error.object_interface, error.object_id, error.message
        ));
    }
    dispatched.map(|_| ()).map_err(|e| e.to_string())
}

impl Dispatch<WlRegistry, GlobalListContents> for Probe {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for Probe {
    fn event(
        state: &mut Self,
        layer: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer.ack_configure(serial);
                state.mapping = Mapping::Configured;
            }
            zwlr_layer_surface_v1::Event::Closed => state.mapping = Mapping::Closed,
            _ => {}
        }
    }
}

delegate_noop!(Probe: WlCompositor);
delegate_noop!(Probe: WlRegion);
delegate_noop!(Probe: ZwlrLayerShellV1);
delegate_noop!(Probe: ignore WlSurface);
