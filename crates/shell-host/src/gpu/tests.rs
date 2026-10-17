//! The GPU modules' public surface, offscreen: no Wayland connection and no surface is ever
//! opened here. The pure tables (adapter ranking, present and alpha choice, the shm swizzle)
//! run anywhere; the render tests need a real adapter (this machine's NVIDIA over Vulkan) and
//! share one device, as every surface of a running host does.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::gpu::context::{GpuContext, devices_created};
use crate::gpu::presenter::FrameTarget;
use crate::gpu::render::capture_document;
use crate::gpu::shm::swap_red_blue;
use crate::gpu::swapchain::{
    alpha_mode_of, choose_alpha, choose_present, pixel_extent, present_mode_of,
};
use crate::stats::{AlphaMode, PresentMode};
use crate::{AdapterPref, Scale, Size};
use anyrender::{PaintRef, PaintScene as _, RenderContext, ResourceId, Scene};
use blitz_dom::node::ComputedStyles;
use blitz_dom::{Document as _, DocumentConfig, Widget};
use blitz_traits::events::UiEvent;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::core::{Element, VirtualDom};
use dioxus::prelude::*;
use dioxus_native_dom::{CustomWidgetAttr, DioxusDocument};
use peniko::kurbo::{Affine, Rect};
use peniko::{Fill, ImageBrush, ImageSampler};
use wgpu::{CompositeAlphaMode as A, PresentMode as P};

// ---- swapchain choices --------------------------------------------------------------------

#[test]
fn present_mode_is_mailbox_else_fifo() {
    let cases: &[(&str, &[P], P)] = &[
        (
            "this machine's NVIDIA",
            &[P::Mailbox, P::Fifo, P::Immediate],
            P::Mailbox,
        ),
        ("no mailbox", &[P::Fifo, P::Immediate], P::Fifo),
        ("fifo only", &[P::Fifo], P::Fifo),
        ("nothing listed", &[], P::Fifo),
    ];
    for (name, offered, want) in cases {
        assert_eq!(choose_present(offered), *want, "{name}");
    }
}

#[test]
fn a_pixel_extent_is_at_least_one_and_within_the_device_limit() {
    let size = |width, height| Size { width, height };
    let cases = [
        ("ordinary", size(1920, 1080), (1920, 1080)),
        ("a zero side", size(0, 32), (1, 32)),
        ("negative", size(-4, 7), (1, 7)),
        ("past the limit", size(40_000, i32::MAX), (8192, 8192)),
    ];
    for (name, buffer, want) in cases {
        assert_eq!(pixel_extent(buffer), want, "{name}");
    }
}

#[test]
fn alpha_is_premultiplied_else_postmultiplied_else_opaque() {
    let cases: &[(&str, &[A], A, AlphaMode)] = &[
        (
            "this machine's NVIDIA",
            &[A::Opaque, A::PreMultiplied],
            A::PreMultiplied,
            AlphaMode::PreMultiplied,
        ),
        (
            "post only",
            &[A::Opaque, A::PostMultiplied],
            A::PostMultiplied,
            AlphaMode::PostMultiplied,
        ),
        (
            "both, pre wins",
            &[A::PostMultiplied, A::PreMultiplied],
            A::PreMultiplied,
            AlphaMode::PreMultiplied,
        ),
        (
            "opaque only: blur off",
            &[A::Opaque],
            A::Opaque,
            AlphaMode::Opaque,
        ),
        (
            "inherit only: reported opaque",
            &[A::Inherit],
            A::Inherit,
            AlphaMode::Opaque,
        ),
        ("nothing listed", &[], A::Opaque, AlphaMode::Opaque),
    ];
    for (name, offered, want, stats) in cases {
        let chosen = choose_alpha(offered);
        assert_eq!(chosen, *want, "{name}");
        assert_eq!(alpha_mode_of(chosen), *stats, "{name}");
    }
}

#[test]
fn present_modes_have_stats_names() {
    let cases: &[(P, PresentMode)] = &[
        (P::Mailbox, PresentMode::Mailbox),
        (P::Fifo, PresentMode::Fifo),
        (P::FifoRelaxed, PresentMode::Fifo),
        (P::AutoVsync, PresentMode::Fifo),
        (P::Immediate, PresentMode::Immediate),
        (P::AutoNoVsync, PresentMode::Immediate),
    ];
    for (mode, want) in cases {
        assert_eq!(present_mode_of(*mode), *want, "{mode:?}");
    }
}

// ---- the shm swizzle ----------------------------------------------------------------------

#[test]
fn rgba_becomes_the_byte_order_of_argb8888() {
    // Premultiplied RGBA from vello_cpu -> wl_shm ARGB8888 bytes on little endian (B, G, R, A).
    let cases: &[(&str, [u8; 4], [u8; 4])] = &[
        ("opaque red", [255, 0, 0, 255], [0, 0, 255, 255]),
        ("opaque blue", [0, 0, 255, 255], [255, 0, 0, 255]),
        ("green stays", [0, 200, 0, 255], [0, 200, 0, 255]),
        (
            "half-transparent pink, premultiplied",
            [128, 0, 64, 128],
            [64, 0, 128, 128],
        ),
        ("transparent", [0, 0, 0, 0], [0, 0, 0, 0]),
    ];
    for (name, rgba, bgra) in cases {
        let mut out = [9u8; 4];
        swap_red_blue(rgba, &mut out);
        assert_eq!(out, *bgra, "{name}");
        let mut back = [9u8; 4];
        swap_red_blue(&out, &mut back);
        assert_eq!(back, *rgba, "{name}: the swizzle is its own inverse");
    }
}

#[test]
fn the_swizzle_covers_every_pixel_of_a_row() {
    let row = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let mut out = [0u8; 12];
    swap_red_blue(&row, &mut out);
    assert_eq!(out, [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12]);
}

// ---- rendering on the shared device -------------------------------------------------------

/// The one device of this test process, as the host has one for all its surfaces.
fn gpu() -> MutexGuard<'static, GpuContext> {
    static GPU: OnceLock<Mutex<GpuContext>> = OnceLock::new();
    GPU.get_or_init(|| {
        let gpu = GpuContext::offscreen(&AdapterPref::Discrete)
            .expect("these tests need a GPU adapter (this machine: NVIDIA over Vulkan)");
        eprintln!("gpu tests on {}", gpu.adapter().label());
        Mutex::new(gpu)
    })
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn document(app: fn() -> Element, buffer: Size, scale: Scale) -> DioxusDocument {
    let viewport = Viewport::new(
        buffer.width as u32,
        buffer.height as u32,
        scale.as_f64() as f32,
        ColorScheme::Light,
    );
    let mut doc = DioxusDocument::new(
        VirtualDom::new(app),
        DocumentConfig {
            viewport: Some(viewport),
            ..Default::default()
        },
    );
    doc.initial_build();
    for _ in 0..8 {
        if !doc.poll(None) {
            break;
        }
    }
    doc
}

fn render(doc: &mut DioxusDocument, buffer: Size, scale: Scale) -> image::RgbaImage {
    let mut base = doc.inner.borrow_mut();
    base.resolve(0.0);
    let target = FrameTarget::whole(buffer, scale, 0.0);
    capture_document(&mut gpu(), &mut base, target).expect("offscreen render")
}

fn pixel(image: &image::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    image.get_pixel(x, y).0
}

fn close(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 1)
}

fn half_pink() -> Element {
    rsx! {
        style { "html, body {{ margin: 0; height: 100%; background: transparent; }}
                 #bar {{ width: 20px; height: 16px; background: rgb(255, 0, 128); }}" }
        div { id: "bar" }
    }
}

#[test]
fn a_document_renders_to_a_texture_at_each_scale_and_reads_back() {
    const PINK: [u8; 4] = [255, 0, 128, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];
    type Probe = (&'static str, u32, [u8; 4]);
    const ONE_X: &[Probe] = &[
        ("inside the bar", 5, PINK),
        ("last bar column", 19, PINK),
        ("first column past the bar, transparent", 20, CLEAR),
        ("far right, transparent", 60, CLEAR),
    ];
    const ONE_AND_A_HALF_X: &[Probe] = &[
        ("last bar column at 1.5x", 29, PINK),
        ("first column past the bar at 1.5x", 30, CLEAR),
    ];
    // scale, buffer, the row probed, probes (x, expected). At 1.5x the 20 logical px bar is
    // exactly 30 physical px.
    let cases: &[(Scale, Size, u32, &[Probe])] = &[
        (
            Scale::ONE,
            Size {
                width: 64,
                height: 16,
            },
            8,
            ONE_X,
        ),
        (
            Scale(180),
            Size {
                width: 96,
                height: 24,
            },
            12,
            ONE_AND_A_HALF_X,
        ),
    ];
    for &(scale, buffer, row, probes) in cases {
        let image = render(&mut document(half_pink, buffer, scale), buffer, scale);
        for &(name, x, want) in probes {
            let got = pixel(&image, x, row);
            assert!(close(got, want), "{name}: x={x} got {got:?} want {want:?}");
        }
    }
}

// ---- custom-widget textures ---------------------------------------------------------------

/// Times any `Swatch` registered a texture.
static SWATCH_REGISTRATIONS: AtomicU32 = AtomicU32::new(0);
const SWATCH_GREEN: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 1.0,
    b: 0.0,
    a: 1.0,
};

/// A tiny GPU widget: on its first paint it creates a texture of its box's size, clears it to
/// green on the device the renderer hands it, and registers it; every paint draws it.
#[derive(Default)]
struct Swatch {
    device: Option<wgpu_context::DeviceHandle>,
    texture: Option<ResourceId>,
}

impl Swatch {
    fn register(
        &mut self,
        ctx: &mut dyn RenderContext,
        width: u32,
        height: u32,
    ) -> Option<ResourceId> {
        let device = self.device.as_ref()?;
        let texture = device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("swatch"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("swatch clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(SWATCH_GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        device.queue.submit([encoder.finish()]);
        let id = ctx.try_register_custom_resource(Box::new(texture)).ok()?;
        SWATCH_REGISTRATIONS.fetch_add(1, Ordering::SeqCst);
        Some(id)
    }
}

impl Widget for Swatch {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.device = render_ctx
            .renderer_specific_context()
            .and_then(|ctx| ctx.downcast::<wgpu_context::DeviceHandle>().ok())
            .map(|handle| *handle);
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
        if self.texture.is_none() {
            self.texture = self.register(render_ctx, width, height);
        }
        if let Some(image) = self.texture {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintRef::Resource(ImageBrush {
                    image,
                    sampler: ImageSampler::default(),
                }),
                None,
                &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
        }
        scene
    }
}

fn swatch_app() -> Element {
    rsx! {
        style { "html, body {{ margin: 0; height: 100%; background: transparent; }}
                 object {{ display: block; width: 16px; height: 16px; }}" }
        object { "data": CustomWidgetAttr::new(Swatch::default()) }
    }
}

/// The swatch tests count the shared map's textures, so they take turns.
static WIDGET_TESTS: Mutex<()> = Mutex::new(());

#[test]
fn a_closed_documents_widget_textures_leave_the_shared_map() {
    let _turn = WIDGET_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let buffer = Size {
        width: 32,
        height: 16,
    };
    let mut doc = document(swatch_app, buffer, Scale::ONE);
    let before = gpu().widgets.textures.len();
    render(&mut doc, buffer, Scale::ONE);
    assert_eq!(
        gpu().widgets.textures.len(),
        before + 1,
        "the widget's texture is in the map after its first paint"
    );
    gpu()
        .widgets
        .forget_document(&mut doc.inner.borrow_mut());
    assert_eq!(
        gpu().widgets.textures.len(),
        before,
        "closing the document gave its texture back"
    );
}

#[test]
fn a_widget_texture_is_registered_once_and_drawn_every_frame() {
    let _turn = WIDGET_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let buffer = Size {
        width: 32,
        height: 16,
    };
    let mut doc = document(swatch_app, buffer, Scale::ONE);
    let before = SWATCH_REGISTRATIONS.load(Ordering::SeqCst);
    for frame in 0..3 {
        let image = render(&mut doc, buffer, Scale::ONE);
        let inside = pixel(&image, 8, 8);
        let outside = pixel(&image, 24, 8);
        assert!(
            close(inside, [0, 255, 0, 255]),
            "frame {frame}: widget pixel {inside:?}"
        );
        assert!(
            close(outside, [0, 0, 0, 0]),
            "frame {frame}: outside pixel {outside:?}"
        );
    }
    assert_eq!(
        SWATCH_REGISTRATIONS.load(Ordering::SeqCst),
        before + 1,
        "the texture registered on the first frame is still bound on the later ones"
    );
}

#[test]
fn every_render_shares_one_device() {
    let buffer = Size {
        width: 8,
        height: 8,
    };
    let before = {
        let _gpu = gpu();
        devices_created()
    };
    render(
        &mut document(half_pink, buffer, Scale::ONE),
        buffer,
        Scale::ONE,
    );
    let other = Size {
        width: 40,
        height: 12,
    };
    render(
        &mut document(half_pink, other, Scale(180)),
        other,
        Scale(180),
    );
    assert_eq!(before, 1, "one device for the whole process");
    assert_eq!(devices_created(), before, "rendering never creates another");
}
