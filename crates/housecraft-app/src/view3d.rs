//! The 3D view: a wgpu renderer drawn inside egui, with orbit / pan / zoom /
//! walk navigation, camera presets, saved cameras and click-to-select.

use std::sync::Arc;

use egui::{Rect, Sense, Ui};
use egui_wgpu::wgpu;
use glam::{Mat4, Vec3, Vec4};

use housecraft_api::Command;
use housecraft_api::geom::mesh::{Category, Mesh, build, mesh_bounds};
use housecraft_api::model::{Camera, Id, Projection};
use housecraft_api::units::{Length, Point3};
use wgpu::util::DeviceExt as _;

use crate::app::App;
use crate::tools::CameraAction;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light: [f32; 4],
}

const SHADER: &str = r#"
struct U { view_proj: mat4x4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32> };
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) normal: vec3<f32>, @location(1) color: vec4<f32> };
@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    return o;
}
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light.xyz);
    let diffuse = abs(dot(n, l));
    let sky = 0.5 + 0.5 * n.z;
    let shade = 0.35 + 0.5 * diffuse + 0.15 * sky;
    return vec4<f32>(i.color.rgb * shade, 1.0);
}
"#;

struct Renderer3d {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    vbuf: Option<wgpu::Buffer>,
    ibuf: Option<wgpu::Buffer>,
    count: u32,
    key: u64,
}

pub fn init_renderer(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("housecraft 3d"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("housecraft 3d"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("housecraft 3d"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("housecraft 3d"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: crate::MSAA,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("housecraft 3d uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("housecraft 3d"),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }],
    });
    rs.renderer.write().callback_resources.insert(Renderer3d {
        pipeline,
        uniforms,
        bind,
        vbuf: None,
        ibuf: None,
        count: 0,
        key: 0,
    });
}

struct Paint3d {
    vertices: Arc<Vec<Vertex>>,
    indices: Arc<Vec<u32>>,
    key: u64,
    uniforms: Uniforms,
}

impl egui_wgpu::CallbackTrait for Paint3d {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Renderer3d>() else {
            return Vec::new();
        };
        queue.write_buffer(&r.uniforms, 0, bytemuck::bytes_of(&self.uniforms));
        if r.key != self.key {
            r.vbuf = (!self.vertices.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("housecraft 3d vertices"),
                    contents: bytemuck::cast_slice(&self.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            });
            r.ibuf = (!self.indices.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("housecraft 3d indices"),
                    contents: bytemuck::cast_slice(&self.indices),
                    usage: wgpu::BufferUsages::INDEX,
                })
            });
            r.count = self.indices.len() as u32;
            r.key = self.key;
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<Renderer3d>() else {
            return;
        };
        let (Some(v), Some(i)) = (&r.vbuf, &r.ibuf) else {
            return;
        };
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, &r.bind, &[]);
        pass.set_vertex_buffer(0, v.slice(..));
        pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..r.count, 0, 0..1);
    }
}

/// Distinct values mixed into the upload key for each hidden category.
const CATEGORY_KEYS: [Id; 8] = [
    Id(u64::MAX - 1),
    Id(u64::MAX - 2),
    Id(u64::MAX - 3),
    Id(u64::MAX - 4),
    Id(u64::MAX - 5),
    Id(u64::MAX - 6),
    Id(u64::MAX - 7),
    Id(u64::MAX - 8),
];

/// Uploaded geometry: (selection/document key, vertices, indices).
type GpuMesh = (u64, Arc<Vec<Vertex>>, Arc<Vec<u32>>);

/// Camera state in metres, z up.
pub struct View3d {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov: f32,
    pub ortho: bool,
    pub walk: bool,
    pub fit_requested: bool,
    /// Mesh categories not drawn (ceilings are hidden in overviews so rooms stay visible).
    pub hidden: Vec<Category>,
    mesh: Option<(u64, Mesh)>,
    gpu: Option<GpuMesh>,
    gpu_key: u64,
}

impl Default for View3d {
    fn default() -> Self {
        View3d {
            target: Vec3::new(6.0, 4.5, 1.2),
            yaw: 60f32.to_radians(),
            pitch: 30f32.to_radians(),
            distance: 25.0,
            fov: 45f32.to_radians(),
            ortho: false,
            walk: false,
            fit_requested: true,
            hidden: vec![Category::Ceiling],
            mesh: None,
            gpu: None,
            gpu_key: 0,
        }
    }
}

impl View3d {
    fn forward(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            -self.pitch.sin(),
        )
    }

    pub fn eye(&self) -> Vec3 {
        self.target - self.forward() * self.distance
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(0.3, 2000.0);
    }

    fn look_from(&mut self, eye: Vec3, target: Vec3) {
        let d = target - eye;
        let len = d.length().max(0.01);
        self.target = target;
        self.distance = len;
        self.yaw = d.y.atan2(d.x);
        self.pitch = (-d.z / len).asin();
    }

    pub fn apply_camera(&mut self, cam: &Camera) {
        let p = |q: Point3| {
            let [x, y, z] = q.mm();
            Vec3::new(x as f32, y as f32, z as f32) / 1000.0
        };
        self.look_from(p(cam.eye), p(cam.target));
        match cam.projection {
            Projection::Perspective { fov } => {
                self.ortho = false;
                self.fov = (fov as f32).to_radians();
            }
            Projection::Orthographic { .. } => self.ortho = true,
        }
        self.walk = false;
        self.fit_requested = false;
    }

    pub fn save_command(&self, name: String, layer_set: Id) -> Command {
        let mm = |v: Vec3| {
            Point3::from_mm(
                v.x as f64 * 1000.0,
                v.y as f64 * 1000.0,
                v.z as f64 * 1000.0,
            )
        };
        Command::AddCamera {
            name,
            projection: if self.ortho {
                Projection::Orthographic {
                    height: Length::from_m((self.ortho_height()) as f64),
                }
            } else {
                Projection::Perspective {
                    fov: self.fov.to_degrees() as f64,
                }
            },
            eye: mm(self.eye()),
            target: mm(self.target),
            layer_set: Some(layer_set),
        }
    }

    fn ortho_height(&self) -> f32 {
        2.0 * self.distance * (self.fov / 2.0).tan()
    }

    pub fn preset_for(&mut self, action: CameraAction) {
        self.fit_requested = true;
        self.hidden.retain(|c| *c != Category::Ceiling);
        if action != CameraAction::FullCamera {
            self.hidden.push(Category::Ceiling);
        }
        match action {
            CameraAction::PerspectiveOverview => {
                self.ortho = false;
                self.walk = false;
                // From the front-left (south-west), looking north-east.
                self.yaw = 60f32.to_radians();
                self.pitch = 30f32.to_radians();
            }
            CameraAction::OrthographicOverview => {
                self.ortho = true;
                self.walk = false;
                self.yaw = 45f32.to_radians();
                self.pitch = 35f32.to_radians();
            }
            CameraAction::FullCamera => {
                self.ortho = false;
                self.walk = true;
                self.yaw = 90f32.to_radians();
                self.pitch = 0.0;
            }
        }
    }

    fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let view = glam::camera::rh::view::look_at_mat4(eye, self.target, Vec3::Z);
        let far = (self.distance * 20.0).max(500.0);
        let proj = if self.ortho {
            let h = self.ortho_height() / 2.0;
            glam::camera::rh::proj::directx::orthographic(-h * aspect, h * aspect, -h, h, -far, far)
        } else {
            glam::camera::rh::proj::directx::perspective(self.fov, aspect, 0.05, far)
        };
        proj * view
    }

    fn fit(&mut self, mesh: &Mesh) {
        let Some((lo, hi)) = mesh_bounds(mesh) else {
            return;
        };
        let center = (lo + hi) / 2.0;
        let radius = ((hi - lo).length() / 2.0).max(2.0);
        if self.walk {
            // Stand at eye height on the south side, looking north into the building.
            let eye = Vec3::new(center.x, lo.y + (hi.y - lo.y) * 0.2, lo.z + 1.68);
            self.look_from(eye, eye + Vec3::Y * 3.0);
        } else {
            self.target = center;
            self.distance = radius / (self.fov / 2.0).sin() * 1.05;
        }
    }
}

/// Grade level in metres: 8" below the platform of the lowest storey above a
/// foundation, or just under the lowest floor when there is none.
fn grade(doc: &housecraft_api::model::Document) -> Option<f32> {
    use housecraft_api::model::Floor;
    let floors = doc.floors_by_elevation();
    let has_foundation = floors.iter().any(|f| f.foundation.is_some());
    let first: &Floor = floors.iter().find(|f| f.foundation.is_none())?;
    let z = if has_foundation {
        (first.elevation - first.floor_structure).mm() - 203.2
    } else {
        first.elevation.mm() - 20.0
    };
    Some((z / 1000.0) as f32)
}

fn to_gpu(
    mesh: &Mesh,
    selection: &[Id],
    hidden: &[Category],
    grade_z: Option<f32>,
) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices: Vec<Vertex> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .zip(&mesh.colors)
        .map(|((p, n), c)| Vertex {
            pos: *p,
            normal: *n,
            color: *c,
        })
        .collect();
    let accent = [1.0, 0.55, 0.1, 1.0];
    for part in &mesh.parts {
        if selection.contains(&part.object) {
            let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
            for &i in &mesh.indices[range] {
                let c = &mut vertices[i as usize].color;
                for k in 0..3 {
                    c[k] = c[k] * 0.4 + accent[k] * 0.6;
                }
            }
        }
    }
    // Ground plane under everything.
    if let Some((lo, hi)) = mesh_bounds(mesh) {
        let pad = ((hi - lo).length() * 0.6).max(10.0);
        let z = grade_z.unwrap_or(lo.z - 0.02);
        let base = vertices.len() as u32;
        let g = [0.62, 0.68, 0.55, 1.0];
        for (x, y) in [
            (lo.x - pad, lo.y - pad),
            (hi.x + pad, lo.y - pad),
            (hi.x + pad, hi.y + pad),
            (lo.x - pad, hi.y + pad),
        ] {
            vertices.push(Vertex {
                pos: [x, y, z],
                normal: [0.0, 0.0, 1.0],
                color: g,
            });
        }
        let mut indices = visible_indices(mesh, hidden);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        return (vertices, indices);
    }
    (vertices, visible_indices(mesh, hidden))
}

fn visible_indices(mesh: &Mesh, hidden: &[Category]) -> Vec<u32> {
    mesh.parts
        .iter()
        .filter(|p| !hidden.contains(&p.category))
        .flat_map(|p| {
            mesh.indices[p.first_index as usize..(p.first_index + p.index_count) as usize]
                .iter()
                .copied()
        })
        .collect()
}

/// Nearest object hit by a ray (Möller–Trumbore over the mesh).
fn pick(mesh: &Mesh, origin: Vec3, dir: Vec3, hidden: &[Category]) -> Option<Id> {
    let mut best: Option<(f32, Id)> = None;
    for part in &mesh.parts {
        if part.object == Id(0) || hidden.contains(&part.category) {
            continue;
        }
        let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
        for tri in mesh.indices[range].as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from_array(mesh.positions[tri[k] as usize]));
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - a;
            let u = s.dot(p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = dir.dot(q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = e2.dot(q) * inv;
            if t > 0.0 && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, part.object));
            }
        }
    }
    best.map(|(_, id)| id)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let v = &mut app.view3d;
    let mesh = match v.mesh.take() {
        Some((rev, mesh)) if rev == app.rev => mesh,
        _ => build(app.session.document(), None),
    };
    if std::mem::take(&mut v.fit_requested) {
        v.fit(&mesh);
    }

    // Navigation.
    let (shift, scroll, keys, dt) = ui.input(|i| {
        let k = |key| i.key_down(key);
        (
            i.modifiers.shift,
            i.smooth_scroll_delta().y,
            [
                k(egui::Key::W),
                k(egui::Key::S),
                k(egui::Key::A),
                k(egui::Key::D),
                k(egui::Key::Q),
                k(egui::Key::E),
            ],
            i.stable_dt.min(0.1),
        )
    });
    let d = resp.drag_delta();
    let pan = resp.dragged_by(egui::PointerButton::Middle)
        || resp.dragged_by(egui::PointerButton::Secondary)
        || (shift && resp.dragged_by(egui::PointerButton::Primary));
    let right = v.forward().cross(Vec3::Z).normalize_or_zero();
    if pan {
        let up = right.cross(v.forward());
        let k = v.distance * 0.0015;
        v.target += (-right * d.x + up * d.y) * k;
    } else if resp.dragged_by(egui::PointerButton::Primary) {
        let eye = v.eye();
        v.yaw -= d.x * 0.008;
        v.pitch = (v.pitch + d.y * 0.008).clamp(-1.55, 1.55);
        if v.walk {
            // Look around from a fixed eye.
            v.target = eye + v.forward() * v.distance;
        }
    }
    if resp.hovered() && scroll != 0.0 {
        if v.walk {
            let flat = Vec3::new(v.forward().x, v.forward().y, 0.0).normalize_or_zero();
            v.target += flat * scroll * 0.01;
        } else {
            v.zoom_by((-scroll * 0.0015).exp());
        }
    }
    if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
        let speed = if v.walk { 2.5 } else { v.distance * 0.6 } * dt;
        let flat = Vec3::new(v.forward().x, v.forward().y, 0.0).normalize_or_zero();
        let mut m = Vec3::ZERO;
        if keys[0] {
            m += flat;
        }
        if keys[1] {
            m -= flat;
        }
        if keys[2] {
            m -= right;
        }
        if keys[3] {
            m += right;
        }
        if keys[4] {
            m -= Vec3::Z;
        }
        if keys[5] {
            m += Vec3::Z;
        }
        if m != Vec3::ZERO {
            v.target += m * speed;
            ui.ctx().request_repaint();
        }
    }

    // Selection by click.
    let aspect = rect.width() / rect.height().max(1.0);
    let vp = v.view_proj(aspect);
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let ndc = egui::vec2(
            (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
            1.0 - (p.y - rect.top()) / rect.height() * 2.0,
        );
        let inv = vp.inverse();
        let near = inv * Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let (near, far) = (near.truncate() / near.w, far.truncate() / far.w);
        let hit = pick(&mesh, near, (far - near).normalize(), &app.view3d.hidden);
        let ctrl = ui.input(|i| i.modifiers.command);
        match hit {
            Some(id) if ctrl => {
                if let Some(i) = app.selection.iter().position(|x| *x == id) {
                    app.selection.remove(i);
                } else {
                    app.selection.push(id);
                }
            }
            Some(id) => app.selection = vec![id],
            None if !ctrl => app.selection.clear(),
            None => {}
        }
    }
    if resp.double_clicked() {
        app.open_object();
    }

    // Upload when the document or selection changed.
    let v = &mut app.view3d;
    let sel_hash = app
        .selection
        .iter()
        .chain(v.hidden.iter().map(|c| &CATEGORY_KEYS[*c as usize]))
        .fold(app.rev.wrapping_mul(1_000_003), |h, id| {
            h.wrapping_mul(31).wrapping_add(id.0)
        });
    if v.gpu.as_ref().is_none_or(|g| g.0 != sel_hash) {
        let (verts, idx) = to_gpu(
            &mesh,
            &app.selection,
            &v.hidden,
            grade(app.session.document()),
        );
        v.gpu_key = v.gpu_key.wrapping_add(1);
        v.gpu = Some((sel_hash, Arc::new(verts), Arc::new(idx)));
    }
    let (_, verts, idx) = v.gpu.clone().expect("uploaded");
    v.mesh = Some((app.rev, mesh));

    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    painter.rect_filled(
        rect,
        0.0,
        if dark {
            egui::Color32::from_rgb(38, 44, 54)
        } else {
            egui::Color32::from_rgb(196, 214, 232)
        },
    );
    let light = (Vec3::new(0.4, -0.6, 0.8)).normalize();
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        Paint3d {
            vertices: verts,
            indices: idx,
            key: v.gpu_key,
            uniforms: Uniforms {
                view_proj: vp.to_cols_array_2d(),
                light: [light.x, light.y, light.z, 0.0],
            },
        },
    ));
    overlay(app, ui, rect);
}

fn overlay(app: &mut App, ui: &mut Ui, rect: Rect) {
    let v = &mut app.view3d;
    let area = Rect::from_min_size(
        rect.min + egui::vec2(8.0, 8.0),
        egui::vec2(rect.width() - 16.0, 30.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.horizontal(|ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut v.ortho, false, "Perspective");
                ui.selectable_value(&mut v.ortho, true, "Orthographic");
                ui.separator();
                if ui
                    .selectable_label(v.walk, "Walk")
                    .on_hover_text("Walkthrough: drag to look, WASD to move")
                    .clicked()
                {
                    v.walk = !v.walk;
                    // Inside the house ceilings belong in the view; overviews hide them.
                    v.hidden.retain(|c| *c != Category::Ceiling);
                    if v.walk {
                        v.pitch = 0.0;
                        v.distance = 3.0;
                    } else {
                        v.hidden.push(Category::Ceiling);
                    }
                }
                ui.separator();
                for (name, yaw, pitch) in [
                    ("Top", -90.0f32, 89.5f32),
                    ("Front", 90.0, 0.0),
                    ("Back", -90.0, 0.0),
                    ("Left", 0.0, 0.0),
                    ("Right", 180.0, 0.0),
                    ("Iso", 45.0, 35.26),
                ] {
                    if ui.button(name).clicked() {
                        v.walk = false;
                        v.yaw = yaw.to_radians();
                        v.pitch = pitch.to_radians();
                        v.fit_requested = true;
                    }
                }
                ui.separator();
                if ui.button("Fit").clicked() {
                    v.fit_requested = true;
                }
            });
        });
    });
}
