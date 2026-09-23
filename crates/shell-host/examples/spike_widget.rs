//! Milestone 0 spike item (d): a custom-painted Blitz widget that renders into its own wgpu
//! texture inside a Dioxus document — the path the terminal, video and PDF viewer will use.
//!
//! `Spinner` implements `blitz_dom::Widget`: `can_create_surfaces` downcasts the renderer's
//! context to `wgpu_context::DeviceHandle`, builds a render pipeline and a texture, and
//! registers the texture with `try_register_custom_resource`; `paint` draws a rotating
//! triangle into that texture each frame and returns a scene that paints the registered
//! resource over the widget's box; `requires_redraw` is true for 2s from construction, then
//! false.
//!
//! There is no window, so there is nothing to present this into. Two things are proven
//! instead, both against `spike/out/d.*`:
//!
//!   1. The document/headless path (`shell_host::headless::render_png`, CPU-only via
//!      `anyrender_vello_cpu`) actually reaches `Spinner::paint` through a real
//!      `DioxusDocument` + `blitz_paint::paint_scene`. Its `RenderContext` has no wgpu device
//!      (`renderer_specific_context()` is `None` on the CPU backend), so `Spinner` stays
//!      `Suspended` and paints nothing — the widget region comes out transparent. That is the
//!      expected, documented behaviour of the CPU path (see `src/headless.rs`), not a bug.
//!   2. A standalone wgpu instance/adapter/device (no Blitz, no window, no compatible
//!      surface — offscreen device creation is exactly what headless GPU work looks like)
//!      drives the *same* `Spinner::can_create_surfaces`/`paint` calls a GPU-backed anyrender
//!      backend would make, and reads the resulting texture back to a PNG. This is the actual
//!      proof that the texture path works on this machine's adapter.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyrender::{PaintRef, RenderContext, ResourceId, Scene};
use blitz_dom::Widget;
use blitz_dom::node::ComputedStyles;
use blitz_traits::events::UiEvent;
use blitz_traits::shell::ColorScheme;
use dioxus::core::Element;
use dioxus::prelude::*;
use dioxus_native_dom::CustomWidgetAttr;
use image::RgbaImage;
use peniko::kurbo::{Affine, Rect};
use peniko::{Fill, ImageBrush, ImageSampler};
use shell_host::headless::{Fonts, HeadlessConfig, PixelSize, Scale, render_png};

const STAGE_SIZE: (u32, u32) = (240, 180);
const AT_SECONDS: [f64; 4] = [0.0, 0.5, 1.0, 3.0];
const REDRAW_WINDOW: Duration = Duration::from_secs(2);

fn app() -> Element {
    rsx! {
        style { "{STYLES}" }
        div { id: "stage",
            object { "data": CustomWidgetAttr::new(Spinner::new()) }
        }
    }
}

const STYLES: &str = "
#stage { width: 240px; height: 180px; background: #202225; }
#stage object { display: block; width: 100%; height: 100%; }
";

fn main() {
    let out_dir = spike_out_dir();
    std::fs::create_dir_all(&out_dir).expect("spike/out is creatable");

    run_cpu_headless_path(&out_dir);
    run_standalone_gpu_probe(&out_dir);
}

/// Proof 1: drive `Spinner` through the real document/headless pipeline. Expected to come out
/// transparent in the widget's box — see the module doc comment.
fn run_cpu_headless_path(out_dir: &Path) {
    let images = render_png(
        app,
        HeadlessConfig {
            size: PixelSize::new(STAGE_SIZE.0, STAGE_SIZE.1),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: AT_SECONDS.to_vec(),
            fonts: Fonts::Bundled,
        },
    );

    for (t, image) in AT_SECONDS.iter().zip(images) {
        let path = out_dir.join(format!("d_cpu_t{t:.1}.png"));
        image.save(&path).expect("write CPU-path PNG");
        println!(
            "cpu path  t={t:>4.1}s -> {} (widget region transparent, expected)",
            path.display()
        );
    }
}

/// Proof 2: a standalone, offscreen wgpu device (no compatible surface, so this is exactly
/// what a headless/GPU-capable anyrender backend would hand `Spinner::can_create_surfaces`)
/// drives the widget directly and reads the texture back.
fn run_standalone_gpu_probe(out_dir: &Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("current-thread tokio runtime");

    let device_handle = runtime.block_on(wgpu_context::DeviceHandle::new_from_compatible_surface(
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle()),
        None,
        None,
        None,
    ));

    let device_handle = match device_handle {
        Ok(handle) => handle,
        Err(err) => {
            println!(
                "gpu probe skipped: no headless wgpu adapter on this machine ({err}); \
                 the CPU headless path above already shows the documented transparent \
                 fallback, which is the only behaviour this machine can prove."
            );
            return;
        }
    };
    println!("gpu probe adapter: {:?}", device_handle.adapter.get_info());

    let mut ctx = StandaloneRenderContext::new(device_handle);

    // requires_redraw: true well inside the 2s window, false once it has elapsed.
    let mut fresh = Spinner::new();
    fresh.can_create_surfaces(&mut ctx);
    assert!(
        fresh.requires_redraw(),
        "spinner must request redraws immediately after creation"
    );

    for &t in &AT_SECONDS {
        let mut spinner = Spinner::started_at(Instant::now() - Duration::from_secs_f64(t));
        spinner.can_create_surfaces(&mut ctx);
        let still_redrawing = spinner.requires_redraw();

        // `Widget::paint`'s `&ComputedStyles` argument (unused by `Spinner`) is only ever
        // produced by a real style resolution inside blitz-dom; building one standalone isn't
        // worth the reach into `stylo` internals, so the probe drives the same texture-draw
        // path `paint` calls (`ActiveSpinner::render`) directly. `run_cpu_headless_path`
        // above already exercises the real `paint` trait method end to end, with a genuine
        // `ComputedStyles`, through `blitz_paint::paint_scene`.
        let resource_id = spinner
            .render_for_probe(&mut ctx, STAGE_SIZE.0, STAGE_SIZE.1)
            .expect("active spinner registers its texture");
        let texture = ctx
            .texture(resource_id)
            .expect("registered resource is the spinner's texture");
        let image = read_texture_to_image(
            &ctx.device_handle.device,
            &ctx.device_handle.queue,
            texture,
            STAGE_SIZE.0,
            STAGE_SIZE.1,
        );

        let path = out_dir.join(format!("d_gpu_t{t:.1}.png"));
        image.save(&path).expect("write GPU-path PNG");
        let probe = image.get_pixel(STAGE_SIZE.0 / 2, STAGE_SIZE.1 / 2);
        println!(
            "gpu path  t={t:>4.1}s -> {} (centre pixel {:?}, requires_redraw={still_redrawing})",
            path.display(),
            probe.0
        );
    }

    println!(
        "requires_redraw stops after {}s: {}",
        REDRAW_WINDOW.as_secs(),
        !Spinner::started_at(Instant::now() - REDRAW_WINDOW - Duration::from_millis(1))
            .requires_redraw()
    );
}

fn spike_out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/out")
}

// ---------------------------------------------------------------------------------------------
// The widget under test.
// ---------------------------------------------------------------------------------------------

struct Spinner {
    start_time: Instant,
    state: SpinnerState,
}

enum SpinnerState {
    Suspended,
    Active(Box<ActiveSpinner>),
}

struct ActiveSpinner {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    texture: Option<TextureAndHandle>,
}

#[derive(Clone)]
struct TextureAndHandle {
    texture: wgpu::Texture,
    handle: ResourceId,
}

impl Spinner {
    fn new() -> Self {
        Self::started_at(Instant::now())
    }

    /// Only used to drive the widget at synthetic timestamps without a real sleep (see
    /// `run_standalone_gpu_probe`); `Instant` cannot be constructed from an arbitrary value,
    /// so back-dating `start_time` is the only way to do that.
    fn started_at(start_time: Instant) -> Self {
        Self {
            start_time,
            state: SpinnerState::Suspended,
        }
    }

    /// Drives the same texture-draw path `paint` uses, without needing a `ComputedStyles`
    /// value: only used by the standalone GPU probe (see the module doc comment on why).
    fn render_for_probe(
        &mut self,
        ctx: &mut dyn RenderContext,
        width: u32,
        height: u32,
    ) -> Option<ResourceId> {
        let SpinnerState::Active(active) = &mut self.state else {
            return None;
        };
        let elapsed = self.start_time.elapsed().as_secs_f32();
        active.render(ctx, width, height, elapsed)
    }
}

impl Widget for Spinner {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        let Some(context) = render_ctx.renderer_specific_context() else {
            println!(
                "Spinner::can_create_surfaces: renderer supplied no context (not a wgpu backend)"
            );
            return;
        };
        let Ok(device_handle) = context.downcast::<wgpu_context::DeviceHandle>() else {
            println!(
                "Spinner::can_create_surfaces: renderer context is not wgpu_context::DeviceHandle"
            );
            return;
        };
        self.state = SpinnerState::Active(Box::new(ActiveSpinner::new(&device_handle)));
    }

    fn destroy_surfaces(&mut self) {
        self.state = SpinnerState::Suspended;
    }

    fn requires_redraw(&self) -> bool {
        self.start_time.elapsed() < REDRAW_WINDOW
    }

    fn handle_event(&mut self, _event: &UiEvent) {}

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        if width == 0 || height == 0 {
            return scene;
        }
        let SpinnerState::Active(active) = &mut self.state else {
            return scene;
        };
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let Some(resource_id) = active.render(render_ctx, width, height, elapsed) else {
            return scene;
        };

        use anyrender::PaintScene as _;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintRef::Resource(ImageBrush {
                image: resource_id,
                sampler: ImageSampler::default(),
            }),
            None,
            &Rect::from_origin_size((0.0, 0.0), (width as f64, height as f64)),
        );
        scene
    }
}

impl ActiveSpinner {
    fn new(device_handle: &wgpu_context::DeviceHandle) -> Self {
        let device = &device_handle.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("spike-widget-spinner"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike-widget-spinner-vertices"),
            size: 3 * 8,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            queue: device_handle.queue.clone(),
            pipeline,
            vertex_buffer,
            texture: None,
        }
    }

    fn render(
        &mut self,
        ctx: &mut dyn RenderContext,
        width: u32,
        height: u32,
        elapsed_secs: f32,
    ) -> Option<ResourceId> {
        if self
            .texture
            .as_ref()
            .is_some_and(|t| t.texture.width() != width || t.texture.height() != height)
        {
            let stale = self.texture.take().unwrap();
            ctx.unregister_resource(stale.handle);
        }
        if self.texture.is_none() {
            let texture = create_texture(&self.device, width, height);
            let handle = ctx
                .try_register_custom_resource(Box::new(texture.clone()))
                .expect("CPU backend never calls render() (renderer_specific_context was None)");
            self.texture = Some(TextureAndHandle { texture, handle });
        }
        let texture_and_handle = self.texture.as_ref().unwrap();

        // One full rotation every 2 seconds, matching `requires_redraw`'s window.
        let angle = elapsed_secs * TAU / 2.0;
        let vertices: [[f32; 2]; 3] = std::array::from_fn(|i| {
            let a = angle + i as f32 * TAU / 3.0;
            [a.cos() * 0.7, a.sin() * 0.7]
        });
        let mut bytes = [0u8; 24];
        for (i, [x, y]) in vertices.iter().enumerate() {
            bytes[i * 8..i * 8 + 4].copy_from_slice(&x.to_le_bytes());
            bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&y.to_le_bytes());
        }
        self.queue.write_buffer(&self.vertex_buffer, 0, &bytes);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let view = texture_and_handle
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                multiview_mask: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        Some(texture_and_handle.handle)
    }
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("spike-widget-spinner-texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

const SHADER: &str = "
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.15, 0.75, 1.0);
}
";

// ---------------------------------------------------------------------------------------------
// A minimal `RenderContext` standing in for a GPU-backed anyrender backend, so the standalone
// probe drives `Spinner` through the exact same trait calls `blitz_paint` would make.
// ---------------------------------------------------------------------------------------------

struct StandaloneRenderContext {
    device_handle: wgpu_context::DeviceHandle,
    resources: std::collections::HashMap<ResourceId, wgpu::Texture>,
}

impl StandaloneRenderContext {
    fn new(device_handle: wgpu_context::DeviceHandle) -> Self {
        Self {
            device_handle,
            resources: std::collections::HashMap::new(),
        }
    }

    fn texture(&self, id: ResourceId) -> Option<&wgpu::Texture> {
        self.resources.get(&id)
    }
}

impl RenderContext for StandaloneRenderContext {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn std::any::Any>,
    ) -> Result<ResourceId, anyrender::RegisterResourceError> {
        let texture = *resource
            .downcast::<wgpu::Texture>()
            .map_err(|_| anyrender::RegisterResourceErrorKind::UnsupportedResourceKind)?;
        let id = ResourceId::new();
        self.resources.insert(id, texture);
        Ok(id)
    }

    fn unregister_resource(&mut self, resource_id: ResourceId) {
        self.resources.remove(&resource_id);
    }

    fn renderer_specific_context(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.device_handle.clone()))
    }
}

fn read_texture_to_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> RgbaImage {
    let unpadded_bytes_per_row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("spike-widget-readback"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("device poll");
    rx.recv()
        .expect("map_async callback ran")
        .expect("buffer mapped for read");

    let data = slice.get_mapped_range();
    let mut pixels = vec![0u8; (unpadded_bytes_per_row * height) as usize];
    for row in 0..height {
        let src_start = (row * padded_bytes_per_row) as usize;
        let src = &data[src_start..src_start + unpadded_bytes_per_row as usize];
        let dst_start = (row * unpadded_bytes_per_row) as usize;
        pixels[dst_start..dst_start + unpadded_bytes_per_row as usize].copy_from_slice(src);
    }
    drop(data);
    buffer.unmap();

    RgbaImage::from_raw(width, height, pixels).expect("readback buffer matches width * height * 4")
}
