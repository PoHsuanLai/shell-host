//! `capture <path>`: re-render a surface offscreen at its buffer size and save a PNG. Not
//! counted as a frame; blocks on the GPU (a debug command, never on the frame path).

use super::gpu::{FORMAT, Gpu};
use super::render::paint;
use super::surface::SpikeSurface;

/// Writes the surface's current content to `path` (premultiplied RGBA, as rendered).
pub fn capture(gpu: &mut Gpu, s: &mut SpikeSurface, path: &str) -> Result<(), String> {
    let (w, h) = s.buffer;
    let device = gpu.handle.device.clone();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let row = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    paint(gpu, s, &mut encoder, &view)?;
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.handle.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let line = &data[y * row as usize..][..w as usize * 4];
        for px in line.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    drop(data);
    buffer.unmap();
    image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
}
