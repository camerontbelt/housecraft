//! The 3D view: a wgpu renderer drawn inside egui, with orbit / pan / zoom /
//! walk navigation, camera presets, saved cameras and click-to-select.

use std::sync::Arc;

use egui::{Rect, Sense, Ui};
use egui_wgpu::wgpu;
use glam::{Mat4, Vec3, Vec4};

use housecraft_api::{Command, PaintScope};
use housecraft_api::geom::mesh::{Category, Mesh, build, mesh_bounds};
use housecraft_api::model::{Camera, Id, MaterialIndex, Projection};
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
    /// Roughness, metallic, emissive, opacity.
    pbr: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    /// Sun's view-projection, for the shadow map.
    light_vp: [[f32; 4]; 4],
    /// Direction toward the sun.
    light: [f32; 4],
    /// x: shadows on (1/0); y: sun strength (0 below the horizon).
    params: [f32; 4],
    /// Cutaway plane: fragments with dot(xyz, p) > w are discarded (when params.z is 1).
    clip: [f32; 4],
    /// Camera position (m), for highlights.
    eye: [f32; 4],
    /// Fixture lights: position (m) then colour and strength, per light;
    /// `params.w` is how many.
    lights: [[f32; 4]; 2 * MAX_LIGHTS],
}

/// Most fixture lights drawn at once (the nearest to the view).
const MAX_LIGHTS: usize = 16;

/// What a material painter click covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintReach {
    /// The clicked surface of the clicked object.
    Surface,
    /// Every surface of the clicked object.
    Object,
    /// That kind of surface on everything on the object's floor.
    Floor,
    /// That kind of surface everywhere (the default material).
    Plan,
}

impl PaintReach {
    const ALL: [PaintReach; 4] = [
        PaintReach::Surface,
        PaintReach::Object,
        PaintReach::Floor,
        PaintReach::Plan,
    ];

    fn name(self) -> &'static str {
        match self {
            PaintReach::Surface => "This surface",
            PaintReach::Object => "Whole object",
            PaintReach::Floor => "Whole floor",
            PaintReach::Plan => "Whole plan",
        }
    }
}

/// The material painter: clicks paint `material` (or reset to the default
/// when `None`) instead of selecting; Alt-click picks up a material.
#[derive(Debug, Clone, Copy)]
pub struct Painter {
    pub material: Option<Id>,
    pub reach: PaintReach,
}

impl Painter {
    pub fn new(doc: &housecraft_api::model::Document) -> Self {
        Painter {
            material: doc
                .iter::<housecraft_api::model::Material>()
                .next()
                .map(|m| m.id),
            reach: PaintReach::Surface,
        }
    }
}

/// Live cross-section cutaway in the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cutaway {
    Off,
    FrontToBack,
    SideToSide,
    TopDown,
}

impl Cutaway {
    fn name(self) -> &'static str {
        match self {
            Cutaway::Off => "No cutaway",
            Cutaway::FrontToBack => "Cut front to back",
            Cutaway::SideToSide => "Cut side to side",
            Cutaway::TopDown => "Cut from the top",
        }
    }
}

/// Shadow map resolution (square).
const SHADOW_SIZE: u32 = 2048;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r#"
struct U { view_proj: mat4x4<f32>, light_vp: mat4x4<f32>, light: vec4<f32>, params: vec4<f32>, clip: vec4<f32>, eye: vec4<f32>, lights: array<vec4<f32>, 32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) pbr: vec4<f32> };
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) pbr: vec4<f32>,
};
@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    o.world = v.pos;
    o.pbr = v.pbr;
    return o;
}
@vertex fn vs_shadow(v: VIn) -> @builtin(position) vec4<f32> {
    return u.light_vp * vec4<f32>(v.pos, 1.0);
}
fn sunlit(world: vec3<f32>) -> f32 {
    if (u.params.x < 0.5) {
        return 1.0;
    }
    let p = u.light_vp * vec4<f32>(world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0) {
        return 1.0;
    }
    // 3x3 percentage-closer filtering softens the shadow edge.
    let texel = 1.0 / f32(textureDimensions(shadow_map).x);
    var sum = 0.0;
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let o = vec2<f32>(f32(x), f32(y)) * texel;
            sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, ndc.z - 0.0015);
        }
    }
    return sum / 9.0;
}
// Physically based shading: diffuse plus a roughness-dependent highlight
// from the sun and a soft sky reflection; metals tint their reflections;
// emissive surfaces add their own light. Alpha is the material's opacity.
fn shade(i: VOut) -> vec4<f32> {
    // Ground vertices carry alpha 0: the cutaway never removes the ground.
    if (u.params.z > 0.5 && i.color.a > 0.5 && dot(u.clip.xyz, i.world) > u.clip.w) {
        discard;
    }
    let v = normalize(u.eye.xyz - i.world);
    var n = normalize(i.normal);
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    let l = normalize(u.light.xyz);
    let rough = clamp(i.pbr.x, 0.04, 1.0);
    let metal = clamp(i.pbr.y, 0.0, 1.0);
    let base = i.color.rgb;
    let sun = sunlit(i.world) * u.params.y;
    let diffuse = abs(dot(n, l)) * sun;
    let sky = 0.5 + 0.5 * n.z;
    let lit = 0.35 + 0.5 * diffuse + 0.15 * sky;
    // Blinn-Phong lobe matched to roughness, energy-normalised.
    let h = normalize(l + v);
    let a = rough * rough;
    let power = clamp(2.0 / (a * a) - 2.0, 1.0, 4096.0);
    let spec = pow(max(dot(n, h), 0.0), power) * (power + 8.0) / 25.13 * max(dot(n, l), 0.0) * sun;
    let f0 = mix(vec3<f32>(0.04), base, metal);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(n, v), 0.0), 5.0) * (1.0 - rough);
    // Without an environment map, reflections take the scene's lighting;
    // metals reflect it at their own colour, others only faintly.
    let env = fresnel * (0.2 + 0.8 * lit) * mix(0.3, 1.0, metal) * (1.0 - 0.4 * rough);
    // Light fixtures: point lights with inverse-square falloff.
    var lamps = vec3<f32>(0.0);
    let count = i32(u.params.w);
    for (var k = 0; k < count; k++) {
        let lp = u.lights[2 * k];
        let lc = u.lights[2 * k + 1];
        let dv = lp.xyz - i.world;
        let dist2 = max(dot(dv, dv), 0.01);
        let ld = dv * inverseSqrt(dist2);
        let ndl = max(dot(n, ld), 0.0);
        // Downlights: nothing above a ceiling fixture, a cone under a can.
        // How far below the light the surface is (1 straight down).
        let down = ld.z;
        var spread = 1.0;
        if (lp.w > 1.5) {
            spread = smoothstep(0.25, 0.7, down);
        } else if (lp.w > 0.5) {
            spread = smoothstep(-0.15, 0.25, down);
        }
        let falloff = lc.w * spread * 2.0 / (dist2 + 1.0);
        let lh = normalize(ld + v);
        let lspec = pow(max(dot(n, lh), 0.0), power) * (power + 8.0) / 25.13 * ndl;
        lamps += lc.rgb * falloff * (base * (1.0 - metal) * ndl + f0 * lspec * 0.5);
    }
    let color = base * (1.0 - metal) * lit + fresnel * spec * 0.6 + env + lamps + base * i.pbr.z;
    return vec4<f32>(color, clamp(i.pbr.w, 0.0, 1.0));
}
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
    let c = shade(i);
    if (c.a < 0.99) {
        discard;
    }
    return vec4<f32>(c.rgb, 1.0);
}
// See-through surfaces, blended over the opaque scene.
@fragment fn fs_clear(i: VOut) -> @location(0) vec4<f32> {
    let c = shade(i);
    if (c.a >= 0.99) {
        discard;
    }
    return c;
}
"#;

struct Renderer3d {
    pipeline: wgpu::RenderPipeline,
    clear_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    shadow_bind: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
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
    let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("housecraft shadow map"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("housecraft 3d"),
        bind_group_layouts: &[Some(&bgl), Some(&shadow_bgl)],
        immediate_size: 0,
    });
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("housecraft shadow"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let attributes =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4];
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
    let clear_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("housecraft 3d see-through"),
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
            depth_write_enabled: Some(false),
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
            entry_point: Some("fs_clear"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("housecraft shadow"),
        layout: Some(&shadow_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_shadow"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SHADOW_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            // Push depths back a little so lit faces don't shadow themselves.
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    });
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("housecraft shadow map"),
        size: wgpu::Extent3d {
            width: SHADOW_SIZE,
            height: SHADOW_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_texture.create_view(&Default::default());
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("housecraft shadow sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("housecraft shadow map"),
        layout: &shadow_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&shadow_sampler),
            },
        ],
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
        clear_pipeline,
        shadow_pipeline,
        uniforms,
        bind,
        shadow_bind,
        shadow_view,
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
        encoder: &mut wgpu::CommandEncoder,
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
        // Render the scene's depth from the sun into the shadow map.
        if let (Some(v), Some(i)) = (&r.vbuf, &r.ibuf) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("housecraft shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if self.uniforms.params[0] > 0.5 {
                pass.set_pipeline(&r.shadow_pipeline);
                pass.set_bind_group(0, &r.bind, &[]);
                pass.set_vertex_buffer(0, v.slice(..));
                pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..r.count, 0, 0..1);
            }
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
        pass.set_bind_group(1, &r.shadow_bind, &[]);
        pass.set_vertex_buffer(0, v.slice(..));
        pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..r.count, 0, 0..1);
        pass.set_pipeline(&r.clear_pipeline);
        pass.draw_indexed(0..r.count, 0, 0..1);
    }
}

/// Direction toward the sun (z up) from the document's north and sun settings.
fn sun_direction(doc: &housecraft_api::model::Document) -> Vec3 {
    housecraft_api::geom::sun::direction(doc).as_vec3()
}

/// An orthographic view-projection looking down `light` that covers `lo..hi`
/// (padded so long low-sun shadows still land on the map).
fn light_view_proj(light: Vec3, lo: Vec3, hi: Vec3) -> Mat4 {
    let center = (lo + hi) / 2.0;
    let r = ((hi - lo).length() / 2.0).max(1.0) * 1.6;
    let up = if light.cross(Vec3::Z).length() < 1e-3 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let view = glam::camera::rh::view::look_at_mat4(center + light * r * 2.0, center, up);
    let proj = glam::camera::rh::proj::directx::orthographic(-r, r, -r, r, 0.01, r * 4.0);
    proj * view
}

/// Distinct values mixed into the upload key for each hidden category.
const CATEGORY_KEYS: [Id; 9] = [
    Id(u64::MAX - 1),
    Id(u64::MAX - 2),
    Id(u64::MAX - 3),
    Id(u64::MAX - 4),
    Id(u64::MAX - 5),
    Id(u64::MAX - 6),
    Id(u64::MAX - 7),
    Id(u64::MAX - 8),
    Id(u64::MAX - 9),
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
    pub cutaway: Cutaway,
    /// How far the cutaway has sliced through, 0 to 1.
    pub cut: f32,
    /// The material painter, when active.
    pub painter: Option<Painter>,
    /// Image behind the model (the applied camera's backdrop).
    pub backdrop: Option<String>,
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
            cutaway: Cutaway::Off,
            cut: 0.5,
            mesh: None,
            gpu: None,
            gpu_key: 0,
            painter: None,
            backdrop: None,
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
        self.backdrop = cam.backdrop.clone();
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

/// Grade level in metres (see [`housecraft_api::geom::sun::grade`]).
fn grade(doc: &housecraft_api::model::Document) -> Option<f32> {
    housecraft_api::geom::sun::grade(doc).map(|z| (z / 1000.0) as f32)
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
        .zip(&mesh.pbr)
        .map(|(((p, n), c), m)| Vertex {
            pos: *p,
            normal: *n,
            color: *c,
            pbr: *m,
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
        // Alpha 0 marks the ground for the shader (it is drawn opaque).
        let g = [0.62, 0.68, 0.55, 0.0];
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
                pbr: [0.95, 0.0, 0.0, 1.0],
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

/// Nearest object hit by a ray (Möller–Trumbore over the mesh), and the
/// surface role of the triangle hit.
fn pick(
    mesh: &Mesh,
    origin: Vec3,
    dir: Vec3,
    hidden: &[Category],
) -> Option<(Id, Option<housecraft_api::model::Surface>)> {
    let mut best: Option<(f32, Id, u32)> = None;
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
            if t > 0.0 && best.is_none_or(|(bt, _, _)| t < bt) {
                best = Some((t, part.object, tri[0]));
            }
        }
    }
    best.map(|(_, id, v)| (id, mesh.surfaces.get(v as usize).copied().flatten()))
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let v = &mut app.view3d;
    // The revision the mesh shows (a click below may change the document).
    let mesh_rev = app.rev;
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
        let (ctrl, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.alt));
        if let Some(painter) = app.view3d.painter {
            match hit {
                Some((id, Some(surface))) if alt => {
                    // Eyedropper: take the material this surface shows.
                    let doc = app.session.document();
                    let found = MaterialIndex::new(doc).resolve(id, surface).map(|m| m.id);
                    if let (Some(p), Some(m)) = (&mut app.view3d.painter, found) {
                        p.material = Some(m);
                    }
                }
                Some((id, surface)) => {
                    let (scope, surface) = match painter.reach {
                        PaintReach::Surface => (PaintScope::Object, surface),
                        PaintReach::Object => (PaintScope::Object, None),
                        PaintReach::Floor => (PaintScope::Floor, surface),
                        PaintReach::Plan => (PaintScope::Plan, surface),
                    };
                    if scope == PaintScope::Plan && surface.is_none() {
                        app.error("That surface can't be painted plan-wide.");
                    } else {
                        app.run1(Command::PaintMaterial {
                            material: painter.material,
                            scope,
                            ids: vec![id],
                            surface,
                        });
                    }
                }
                None => {}
            }
        }
        let painting = app.view3d.painter.is_some();
        let hit = hit.map(|(id, _)| id).filter(|_| !painting);
        match hit {
            Some(id) if ctrl => {
                if let Some(i) = app.selection.iter().position(|x| *x == id) {
                    app.selection.remove(i);
                } else {
                    app.selection.push(id);
                }
            }
            Some(id) => app.selection = vec![id],
            None if !ctrl && !painting => app.selection.clear(),
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
        .fold(mesh_rev.wrapping_mul(1_000_003), |h, id| {
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
    v.mesh = Some((mesh_rev, mesh));

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
    if let Some(path) = &v.backdrop {
        let r = rect;
        crate::images::paint(
            ui.ctx(),
            &painter,
            [r.left_bottom(), r.right_bottom(), r.right_top(), r.left_top()],
            path,
            1.0,
            None,
        );
    }
    let uniforms = frame_uniforms(v, app.session.document(), vp);
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        Paint3d {
            vertices: verts,
            indices: idx,
            key: v.gpu_key,
            uniforms,
        },
    ));
    overlay(app, ui, rect);
    painter_bar(app, ui, rect);
}

/// The shader inputs for a frame: camera, sun, shadows, cutaway, lights.
fn frame_uniforms(v: &View3d, doc: &housecraft_api::model::Document, vp: Mat4) -> Uniforms {
    let light = sun_direction(doc);
    // Below the horizon the sun stops lighting and casting.
    let strength = (light.z / 0.1).clamp(0.0, 1.0);
    let bounds = v.mesh.as_ref().and_then(|(_, m)| mesh_bounds(m));
    let shadows = doc.settings.sun.shadows && strength > 0.0 && bounds.is_some();
    let light_vp = bounds.map_or(Mat4::IDENTITY, |(lo, hi)| light_view_proj(light, lo, hi));
    // Cutaway: slice the model away from one side as the slider moves.
    let clip = bounds.and_then(|(lo, hi)| {
        let t = v.cut.clamp(0.0, 1.0);
        match v.cutaway {
            Cutaway::Off => None,
            Cutaway::FrontToBack => Some([0.0, -1.0, 0.0, -(lo.y + t * (hi.y - lo.y))]),
            Cutaway::SideToSide => Some([-1.0, 0.0, 0.0, -(lo.x + t * (hi.x - lo.x))]),
            Cutaway::TopDown => Some([0.0, 0.0, 1.0, hi.z - t * (hi.z - lo.z)]),
        }
    });
    let eye = v.eye();
    // The fixture lights nearest the view target.
    let mut near: Vec<&housecraft_api::geom::mesh::MeshLight> = v
        .mesh
        .as_ref()
        .map(|(_, m)| m.lights.iter().collect())
        .unwrap_or_default();
    near.sort_by(|a, b| {
        let d = |l: &housecraft_api::geom::mesh::MeshLight| Vec3::from_array(l.position).distance(v.target);
        d(a).total_cmp(&d(b))
    });
    let lamps: Vec<[f32; 4]> = near
        .iter()
        .take(MAX_LIGHTS)
        .flat_map(|l| {
            [
                [l.position[0], l.position[1], l.position[2], l.spread as f32],
                [l.color[0], l.color[1], l.color[2], l.intensity],
            ]
        })
        .collect();
    Uniforms {
                view_proj: vp.to_cols_array_2d(),
                light_vp: light_vp.to_cols_array_2d(),
                light: [light.x, light.y, light.z, 0.0],
                params: [
                    if shadows { 1.0 } else { 0.0 },
                    strength,
                    if clip.is_some() { 1.0 } else { 0.0 },
                    lamps.len() as f32 / 2.0,
                ],
                clip: clip.unwrap_or([0.0; 4]),
                eye: eye.extend(1.0).to_array(),
                lights: {
                    let mut a = [[0.0; 4]; 2 * MAX_LIGHTS];
                    a[..lamps.len()].copy_from_slice(&lamps);
                    a
                },
            }
}

/// Render the current 3D view at `width` × `height` pixels and save it as
/// a PNG; with `transparent`, the sky is left see-through.
pub fn export_image(
    app: &mut App,
    width: u32,
    height: u32,
    transparent: bool,
    path: &std::path::Path,
) -> Result<(), String> {
    let rs = app.render_state.clone().ok_or("3D rendering is not available")?;
    let device = &rs.device;
    let max = device.limits().max_texture_dimension_2d;
    let (w, h) = (width.clamp(16, max), height.clamp(16, max));
    let doc = app.session.document();
    let v = &mut app.view3d;
    // A fresh model without selection highlights.
    let mesh = build(doc, None);
    let (verts, idx) = to_gpu(&mesh, &[], &v.hidden, grade(doc));
    v.mesh = Some((app.rev, mesh));
    let uniforms = frame_uniforms(v, doc, v.view_proj(w as f32 / h as f32));
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("export vertices"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("export indices"),
        contents: bytemuck::cast_slice(&idx),
        usage: wgpu::BufferUsages::INDEX,
    });
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
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
    let msaa = texture("export colour", rs.target_format, crate::MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let resolved = texture(
        "export resolve",
        rs.target_format,
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture("export depth", crate::DEPTH_FORMAT, crate::MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let (msaa_view, resolved_view, depth_view) = (
        msaa.create_view(&Default::default()),
        resolved.create_view(&Default::default()),
        depth.create_view(&Default::default()),
    );
    // Rows are padded to 256 bytes for the copy.
    let row = (w * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export readback"),
        size: row as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    {
        let renderer = rs.renderer.read();
        let r = renderer
            .callback_resources
            .get::<Renderer3d>()
            .ok_or("the 3D renderer is not ready")?;
        rs.queue.write_buffer(&r.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = device.create_command_encoder(&Default::default());
        if uniforms.params[0] > 0.5 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("export shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&r.shadow_pipeline);
            pass.set_bind_group(0, &r.bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..idx.len() as u32, 0, 0..1);
        }
        {
            // The light theme's sky, in linear terms for an sRGB target.
            let sky = |c: f64| (c / 255.0).powf(2.2);
            let clear = if transparent {
                wgpu::Color::TRANSPARENT
            } else {
                wgpu::Color {
                    r: sky(196.0),
                    g: sky(214.0),
                    b: sky(232.0),
                    a: 1.0,
                }
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("export"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&resolved_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &r.bind, &[]);
            pass.set_bind_group(1, &r.shadow_bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&r.pipeline);
            pass.draw_indexed(0..idx.len() as u32, 0, 0..1);
            pass.set_pipeline(&r.clear_pipeline);
            pass.draw_indexed(0..idx.len() as u32, 0, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &resolved,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            size,
        );
        rs.queue.submit([encoder.finish()]);
    }
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let bgra = matches!(
        rs.target_format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let line = &data[y * row as usize..y * row as usize + w as usize * 4];
        for px in line.as_chunks::<4>().0 {
            let [a, b, c, d] = *px;
            pixels.extend(if bgra { [c, b, a, d] } else { [a, b, c, d] });
        }
    }
    drop(data);
    readback.unmap();
    image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
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
                egui::ComboBox::from_id_salt("cutaway")
                    .selected_text(v.cutaway.name())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for c in [
                            Cutaway::Off,
                            Cutaway::FrontToBack,
                            Cutaway::SideToSide,
                            Cutaway::TopDown,
                        ] {
                            ui.selectable_value(&mut v.cutaway, c, c.name());
                        }
                    });
                if v.cutaway != Cutaway::Off {
                    ui.add(egui::Slider::new(&mut v.cut, 0.0..=1.0).show_value(false));
                }
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
                    ("Top", 90.0f32, 89.5f32),
                    ("Front", 90.0, 0.0),
                    ("Back", -90.0, 0.0),
                    ("Left", 0.0, 0.0),
                    ("Right", 180.0, 0.0),
                    ("Iso", 45.0, 35.26),
                ] {
                    if ui.button(name).clicked() {
                        v.backdrop = None;
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

/// The material painter's controls, along the bottom of the 3D view.
fn painter_bar(app: &mut App, ui: &mut Ui, rect: Rect) {
    let Some(mut p) = app.view3d.painter else {
        return;
    };
    let area = Rect::from_min_size(
        egui::pos2(rect.left() + 8.0, rect.bottom() - 42.0),
        egui::vec2(rect.width() - 16.0, 34.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let mut close = false;
    child.horizontal(|ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Material Painter");
                let doc = app.session.document();
                if let Some(m) = p.material.and_then(|m| doc.get::<housecraft_api::model::Material>(m)) {
                    crate::materials::swatch(ui, m.color, 16.0);
                }
                if let Some(c) =
                    crate::materials::material_combo(ui, doc, "painter_material", p.material, "Default (reset)")
                {
                    p.material = c;
                }
                egui::ComboBox::from_id_salt("painter_reach")
                    .selected_text(p.reach.name())
                    .show_ui(ui, |ui| {
                        for r in PaintReach::ALL {
                            ui.selectable_value(&mut p.reach, r, r.name());
                        }
                    });
                ui.weak("Click to paint · Alt+click picks up a material");
                if ui.button("Done").clicked() {
                    close = true;
                }
            });
        });
    });
    app.view3d.painter = (!close).then_some(p);
}
