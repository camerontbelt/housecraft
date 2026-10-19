//! Offscreen rendering: still images, high-quality renders (many jittered
//! passes averaged: smoother edges, soft sun shadows, depth of field),
//! 360° panoramas, HDR images, sun time-lapses and walkthrough frames.

use std::path::Path;

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::*;
use crate::app::App;

/// A model ready to render repeatedly at one size.
pub struct Offscreen {
    rs: egui_wgpu::RenderState,
    w: u32,
    h: u32,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    triangles: u32,
    total: u32,
    msaa_view: wgpu::TextureView,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    row: u32,
    clear: wgpu::Color,
    /// Model bounds (m), for aiming the sun's shadow map.
    pub bounds: Option<(Vec3, Vec3)>,
    /// This frame's view as the 3D view has it.
    base: Uniforms,
}

impl Offscreen {
    pub fn new(app: &mut App, width: u32, height: u32, transparent: bool) -> Result<Self, String> {
        let rs = app.render_state.clone().ok_or("3D rendering is not available")?;
        let device = &rs.device;
        let max = device.limits().max_texture_dimension_2d;
        let (w, h) = (width.clamp(16, max), height.clamp(16, max));
        let doc = app.session.document();
        let v = &mut app.view3d;
        // A fresh model without selection highlights.
        let mesh = build(doc, None);
        {
            let mut renderer = rs.renderer.write();
            if let Some(r) = renderer.callback_resources.get_mut::<Renderer3d>()
                && r.tex_keys != mesh.textures
            {
                r.tex_bind = texture_bind(device, &rs.queue, &r.tex_bgl, &mesh.textures);
                r.tex_keys = mesh.textures.clone();
            }
        }
        let (verts, idx, triangles) = to_gpu(&mesh, &[], &v.hidden, grade(doc), ground(doc), v.render.outlines);
        let bounds = mesh_bounds(&mesh);
        v.mesh = Some((app.rev, mesh));
        let base = frame_uniforms(v, doc, v.view_proj(w as f32 / h as f32));
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("offscreen vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("offscreen indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let msaa = texture("offscreen colour", rs.target_format, crate::MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolved = texture(
            "offscreen resolve",
            rs.target_format,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture("offscreen depth", crate::DEPTH_FORMAT, crate::MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let row = (w * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: row as u64 * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        // The sky's horizon colour, in linear terms for an sRGB target.
        let sky = |c: u8| (c as f64 / 255.0).powf(2.2);
        let [hr, hg, hb, _] = sky_colors(&v.render).1 .0;
        let clear = if transparent {
            wgpu::Color::TRANSPARENT
        } else {
            wgpu::Color { r: sky(hr), g: sky(hg), b: sky(hb), a: 1.0 }
        };
        Ok(Offscreen {
            msaa_view: msaa.create_view(&Default::default()),
            resolved_view: resolved.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            resolved,
            rs,
            w,
            h,
            vbuf,
            ibuf,
            triangles,
            total: idx.len() as u32,
            readback,
            row,
            clear,
            bounds,
            base,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// One frame as RGBA bytes, row by row.
    fn render(&self, u: &Uniforms) -> Result<Vec<u8>, String> {
        let (rs, device) = (&self.rs, &self.rs.device);
        {
            let renderer = rs.renderer.read();
            let r = renderer.callback_resources.get::<Renderer3d>().ok_or("the 3D renderer is not ready")?;
            rs.queue.write_buffer(&r.uniforms, 0, bytemuck::bytes_of(u));
            let mut encoder = device.create_command_encoder(&Default::default());
            if u.params[0] > 0.5 {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("offscreen shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &r.shadow_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&r.shadow_pipeline);
                pass.set_bind_group(0, &r.bind, &[]);
                pass.set_vertex_buffer(0, self.vbuf.slice(..));
                pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.triangles, 0, 0..1);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("offscreen"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&self.resolved_view),
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(self.clear), store: wgpu::StoreOp::Discard },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &r.bind, &[]);
                pass.set_bind_group(1, &r.shadow_bind, &[]);
                pass.set_bind_group(2, &r.tex_bind, &[]);
                pass.set_vertex_buffer(0, self.vbuf.slice(..));
                pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&r.pipeline);
                pass.draw_indexed(0..self.triangles, 0, 0..1);
                if self.total > self.triangles {
                    pass.set_pipeline(&r.line_pipeline);
                    pass.draw_indexed(self.triangles..self.total, 0, 0..1);
                }
                pass.set_pipeline(&r.clear_pipeline);
                pass.draw_indexed(0..self.triangles, 0, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.resolved,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.readback,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.row), rows_per_image: Some(self.h) },
                },
                wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
            );
            rs.queue.submit([encoder.finish()]);
        }
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
        let bgra = matches!(rs.target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut pixels = Vec::with_capacity((self.w * self.h * 4) as usize);
        for y in 0..self.h as usize {
            let line = &data[y * self.row as usize..y * self.row as usize + self.w as usize * 4];
            for px in line.as_chunks::<4>().0 {
                let [a, b, c, d] = *px;
                pixels.extend(if bgra { [c, b, a, d] } else { [a, b, c, d] });
            }
        }
        drop(data);
        self.readback.unmap();
        Ok(pixels)
    }

    /// `base` looking from `eye` at `aim` through `proj`.
    fn looking(&self, base: &Uniforms, eye: Vec3, aim: Vec3, proj: Mat4) -> Uniforms {
        let up = if (aim - eye).normalize_or_zero().cross(Vec3::Z).length() < 1e-3 { Vec3::Y } else { Vec3::Z };
        let view = glam::camera::rh::view::look_at_mat4(eye, aim, up);
        let mut u = *base;
        u.view_proj = (proj * view).to_cols_array_2d();
        u.eye = eye.extend(1.0).to_array();
        u.target = aim.extend(1.0).to_array();
        u.misc[2] = eye.distance(aim);
        u
    }

    /// `base` with the sun shining from `light` (shadow map re-aimed).
    fn sun(&self, base: &Uniforms, light: Vec3, strength: f32) -> Uniforms {
        let mut u = *base;
        u.light = [light.x, light.y, light.z, 0.0];
        u.params[1] = strength * (light.z / 0.1).clamp(0.0, 1.0);
        u.params[0] = if u.params[1] > 0.0 && self.bounds.is_some() && base.params[0] > 0.5 { 1.0 } else { 0.0 };
        if let Some((lo, hi)) = self.bounds {
            u.light_vp = light_view_proj(light, lo, hi).to_cols_array_2d();
        }
        u
    }
}

fn save_png(path: &Path, pixels: &[u8], w: u32, h: u32) -> Result<(), String> {
    image::save_buffer(path, pixels, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
}

/// Render the current 3D view and save it as a PNG.
pub fn export_image(app: &mut App, width: u32, height: u32, transparent: bool, path: &Path) -> Result<(), String> {
    let off = Offscreen::new(app, width, height, transparent)?;
    let px = off.render(&off.base)?;
    let (w, h) = off.size();
    save_png(path, &px, w, h)
}

/// A small deterministic sequence in [-0.5, 0.5) (golden-ratio steps).
fn jitter(i: u32, k: f32) -> f32 {
    ((i as f32 + 1.0) * (0.618_034 + k * 0.137)).fract() - 0.5
}

/// A high-quality render: `samples` passes, each nudged by a fraction of a
/// pixel (smooth edges), the sun moved within its disc (soft shadows) and
/// the eye moved across the lens (depth of field, when set), averaged.
pub fn export_quality(app: &mut App, width: u32, height: u32, samples: u32, path: &Path) -> Result<(), String> {
    let off = Offscreen::new(app, width, height, false)?;
    let (w, h) = off.size();
    let v = &app.view3d;
    let eye = v.eye();
    let aim = v.target;
    let fwd = (aim - eye).normalize_or_zero();
    let right = fwd.cross(Vec3::Z).normalize_or(Vec3::X);
    let up = right.cross(fwd);
    let light = Vec3::from_slice(&off.base.light[..3]);
    let (lr, lu) = (light.cross(Vec3::Z).normalize_or(Vec3::X), light.cross(light.cross(Vec3::Z)).normalize_or(Vec3::Y));
    let aspect = w as f32 / h as f32;
    let far = (v.distance * 20.0).max(500.0);
    let proj = if v.ortho {
        let half = v.ortho_height() / 2.0;
        glam::camera::rh::proj::directx::orthographic(-half * aspect, half * aspect, -half, half, -far, far)
    } else {
        glam::camera::rh::proj::directx::perspective(v.fov, aspect, 0.05, far)
    };
    let dof = v.render.depth_of_field.filter(|_| !v.ortho);
    let samples = samples.clamp(1, 256);
    let mut sum = vec![0.0f32; (w * h * 4) as usize];
    for i in 0..samples {
        let (jx, jy) = (jitter(i, 0.0), jitter(i, 1.0));
        // Sub-pixel shift of the whole image.
        let shift = Mat4::from_translation(Vec3::new(jx * 2.0 / w as f32, jy * 2.0 / h as f32, 0.0));
        let (e, a) = match dof {
            Some(d) => {
                // Lens radius from the f-number, focused at `focus`.
                let radius = 0.025 / d.f_stop.max(0.5) as f32 * 2.0;
                let focus = eye + fwd * (d.focus.mm() as f32 / 1000.0).max(0.1);
                (eye + (right * jitter(i, 2.0) + up * jitter(i, 3.0)) * 2.0 * radius, focus)
            }
            None => (eye, aim),
        };
        let mut u = off.looking(&off.base, e, a, shift * proj);
        // The sun's disc is about half a degree across; spread a little more
        // for a pleasant soft edge.
        let soft = 0.012;
        let l = (light + (lr * jitter(i, 4.0) + lu * jitter(i, 5.0)) * soft * 2.0).normalize();
        u = off.sun(&u, l, off.base.params[1]);
        let px = off.render(&u)?;
        for (s, p) in sum.iter_mut().zip(&px) {
            *s += *p as f32;
        }
    }
    let px: Vec<u8> = sum.iter().map(|s| (s / samples as f32).round().clamp(0.0, 255.0) as u8).collect();
    save_png(path, &px, w, h)
}

/// A 360° equirectangular panorama from the camera's eye, `width` × width/2.
pub fn export_panorama(app: &mut App, width: u32, path: &Path) -> Result<(), String> {
    let face = (width / 4).clamp(64, 4096);
    let off = Offscreen::new(app, face, face, false)?;
    let eye = app.view3d.eye();
    let far = (app.view3d.distance * 20.0).max(500.0);
    let proj = glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.05, far);
    // +X, -X, +Y, -Y, +Z, -Z, each with its own up.
    let dirs = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
    let ups = [Vec3::Z, Vec3::Z, Vec3::Z, Vec3::Z, -Vec3::Y, Vec3::Y];
    let mut faces = Vec::new();
    for (d, upv) in dirs.iter().zip(ups) {
        let view = glam::camera::rh::view::look_at_mat4(eye, eye + *d, upv);
        let mut u = off.base;
        u.view_proj = (proj * view).to_cols_array_2d();
        u.eye = eye.extend(1.0).to_array();
        u.target = (eye + *d).extend(1.0).to_array();
        u.misc[1] = 0.0;
        faces.push((view, off.render(&u)?));
    }
    let (pw, ph) = (face * 4, face * 2);
    let mut px = vec![0u8; (pw * ph * 4) as usize];
    for y in 0..ph {
        let lat = std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / ph as f32 * std::f32::consts::PI;
        for x in 0..pw {
            let lon = (x as f32 + 0.5) / pw as f32 * std::f32::consts::TAU - std::f32::consts::PI;
            let d = Vec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin());
            // The face this direction falls on, and where.
            let k = if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() {
                if d.x > 0.0 { 0 } else { 1 }
            } else if d.y.abs() >= d.z.abs() {
                if d.y > 0.0 { 2 } else { 3 }
            } else if d.z > 0.0 {
                4
            } else {
                5
            };
            let (view, data) = &faces[k];
            let c = proj * *view * (eye + d).extend(1.0);
            let (nx, ny) = (c.x / c.w, c.y / c.w);
            let sx = (((nx * 0.5 + 0.5) * face as f32) as u32).min(face - 1);
            let sy = (((0.5 - ny * 0.5) * face as f32) as u32).min(face - 1);
            let si = ((sy * face + sx) * 4) as usize;
            let di = ((y * pw + x) * 4) as usize;
            px[di..di + 4].copy_from_slice(&data[si..si + 4]);
        }
    }
    save_png(path, &px, pw, ph)
}

/// An HDR image (Radiance .hdr) merged from three exposures, so bright
/// windows and dark corners both keep their detail.
pub fn export_hdr(app: &mut App, width: u32, height: u32, path: &Path) -> Result<(), String> {
    let off = Offscreen::new(app, width, height, false)?;
    let (w, h) = off.size();
    let stops = [0.25f32, 1.0, 4.0];
    let mut shots = Vec::new();
    for s in stops {
        let mut u = off.base;
        u.style[3] *= s;
        shots.push(off.render(&u)?);
    }
    let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for i in 0..(w * h) as usize {
        let mut rgb = [0.0f32; 3];
        let mut wsum = 0.0;
        for (shot, s) in shots.iter().zip(stops) {
            let p = &shot[i * 4..i * 4 + 3];
            // Trust mid-tones; clipped or crushed values count little.
            let m = p.iter().map(|c| *c as f32 / 255.0).fold(0.0, f32::max);
            let weight = (1.0 - (2.0 * m - 1.0).abs()).max(0.02);
            for k in 0..3 {
                rgb[k] += weight * lin(p[k]) / s;
            }
            wsum += weight;
        }
        let rgb = rgb.map(|c| c / wsum);
        out.extend(rgbe(rgb));
    }
    let mut file = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nEXPOSURE=1.0\n\n-Y {h} +X {w}\n").into_bytes();
    file.extend(out);
    std::fs::write(path, file).map_err(|e| e.to_string())
}

/// A colour in Radiance's shared-exponent form.
fn rgbe(c: [f32; 3]) -> [u8; 4] {
    let m = c[0].max(c[1]).max(c[2]);
    if m < 1e-32 {
        return [0, 0, 0, 0];
    }
    let e = m.log2().floor() as i32 + 1;
    let scale = 256.0 / 2f32.powi(e);
    [
        (c[0] * scale).clamp(0.0, 255.0) as u8,
        (c[1] * scale).clamp(0.0, 255.0) as u8,
        (c[2] * scale).clamp(0.0, 255.0) as u8,
        (e + 128).clamp(0, 255) as u8,
    ]
}

/// 5×7 glyphs for time stamps.
fn glyph(ch: char) -> [u8; 7] {
    match ch {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        _ => [0; 7],
    }
}

/// Write `text` (digits, ':', '-', '/' and spaces) into the image's
/// bottom-left corner, white on a dark box.
pub fn stamp(px: &mut [u8], w: u32, h: u32, text: &str) {
    let scale = (h / 180).max(2);
    let (cw, ch) = (6 * scale, 8 * scale);
    let (x0, y0) = (scale * 4, h.saturating_sub(ch + scale * 6));
    let width = cw * text.chars().count() as u32 + scale * 4;
    for y in y0.saturating_sub(scale * 2)..(y0 + ch + scale * 2).min(h) {
        for x in x0.saturating_sub(scale * 2)..(x0 + width).min(w) {
            let i = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                px[i + k] /= 3;
            }
        }
    }
    for (n, c) in text.chars().enumerate() {
        let g = glyph(c);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (16 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = x0 + n as u32 * cw + col * scale + dx;
                        let y = y0 + row as u32 * scale + dy;
                        if x < w && y < h {
                            let i = ((y * w + x) * 4) as usize;
                            px[i..i + 3].copy_from_slice(&[255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
}

/// Frames of the sun crossing the sky on one day, each stamped with its
/// time: `folder/sun_0001.png`…; how many were written.
#[allow(clippy::too_many_arguments)]
pub fn export_time_lapse(
    app: &mut App,
    width: u32,
    height: u32,
    latitude: f64,
    day_of_year: u32,
    hours: (f64, f64),
    step_minutes: f64,
    folder: &Path,
) -> Result<usize, String> {
    let off = Offscreen::new(app, width, height, false)?;
    let (w, h) = off.size();
    let north = app.session.document().settings.north;
    let step = (step_minutes / 60.0).max(1.0 / 60.0);
    let mut t = hours.0;
    let mut n = 0;
    while t <= hours.1 + 1e-9 {
        let (azimuth, altitude) = housecraft_api::geom::sun::position(latitude, day_of_year, t);
        let sun = housecraft_api::model::Sun { azimuth, altitude, shadows: true };
        let light = housecraft_api::geom::sun::direction_of(&sun, north).as_vec3();
        let mut u = off.base;
        u.params[0] = 1.0;
        u = off.sun(&u, light, 1.0);
        let mut px = off.render(&u)?;
        let hh = t.floor() as u32;
        let mm = ((t - t.floor()) * 60.0).round() as u32;
        stamp(&mut px, w, h, &format!("{day_of_year} {hh:02}:{mm:02}"));
        n += 1;
        save_png(&folder.join(format!("sun_{n:04}.png")), &px, w, h)?;
        t += step;
    }
    Ok(n)
}

/// Every frame of a walkthrough: `folder/frame_0001.png`…
pub fn export_walkthrough(
    app: &mut App,
    walk: &housecraft_api::model::Walkthrough,
    width: u32,
    height: u32,
    folder: &Path,
) -> Result<usize, String> {
    if let Some(r) = &walk.render {
        app.view3d.render = r.clone();
    }
    let off = Offscreen::new(app, width, height, false)?;
    let (w, h) = off.size();
    let v = &app.view3d;
    let far = (v.distance * 20.0).max(500.0);
    let proj = glam::camera::rh::proj::directx::perspective(v.fov, w as f32 / h as f32, 0.05, far);
    let frames = (walk.duration() * walk.fps).ceil().max(1.0) as usize;
    for f in 0..=frames {
        let t = f as f64 / walk.fps;
        let Some((e, a)) = walk.sample(t) else {
            break;
        };
        let m = |p: [f64; 3]| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 1000.0;
        let u = off.looking(&off.base, m(e), m(a), proj);
        let px = off.render(&u)?;
        save_png(&folder.join(format!("frame_{:04}.png", f + 1)), &px, w, h)?;
    }
    Ok(frames + 1)
}
