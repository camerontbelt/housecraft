//! The 3D view: a wgpu renderer drawn inside egui, with orbit / pan / zoom /
//! walk navigation, camera presets, saved cameras and click-to-select.

use std::sync::Arc;

use egui::{Rect, Sense, Ui};
use egui_wgpu::wgpu;
use glam::{Mat4, Vec3, Vec4};

use housecraft_api::{Command, PaintScope};
use housecraft_api::geom::mesh::{Category, Mesh, TextureKey, build, mesh_bounds};
use housecraft_api::model::{Camera, Id, MaterialIndex, Projection};
use housecraft_api::units::{Length, Point3};
use wgpu::util::DeviceExt as _;

use crate::app::App;
use crate::tools::CameraAction;


mod render;
pub use render::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    /// Roughness, metallic, emissive, opacity.
    pbr: [f32; 4],
    /// Texture coordinates (tiles) and texture layer (−1 for none).
    uv: [f32; 2],
    tex: f32,
    /// 1 for walls, 2 for roofs (for see-through styles and hiding walls
    /// near the camera), 0 otherwise.
    flags: f32,
}

/// Each texture layer's size in pixels.
const TEX_SIZE: u32 = 512;

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
    /// x: style, y: fog density, z: shadow intensity, w: exposure.
    style: [f32; 4],
    duo_dark: [f32; 4],
    duo_light: [f32; 4],
    /// Fog and sky colour at the horizon.
    horizon: [f32; 4],
    /// x: time (s), y: hide walls near the camera (1/0), z: distance to
    /// the target, w: sunlight colour warmth (night 1).
    misc: [f32; 4],
    /// What the camera looks at (m).
    target: [f32; 4],
    /// Ambient light tint (rgb) and how much it applies (w).
    ambient: [f32; 4],
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
struct U { view_proj: mat4x4<f32>, light_vp: mat4x4<f32>, light: vec4<f32>, params: vec4<f32>, clip: vec4<f32>, eye: vec4<f32>, lights: array<vec4<f32>, 32>, style: vec4<f32>, duo_dark: vec4<f32>, duo_light: vec4<f32>, horizon: vec4<f32>, misc: vec4<f32>, aim: vec4<f32>, ambient: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(0) var tex_array: texture_2d_array<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) pbr: vec4<f32>, @location(4) uv: vec2<f32>, @location(5) tex: f32, @location(6) flags: f32 };
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) pbr: vec4<f32>,
    @location(4) uv: vec2<f32>,
    @location(5) tex: f32,
    @location(6) flags: f32,
};
@vertex fn vs(v: VIn) -> VOut {
    return vert(v);
}
fn vert(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    o.world = v.pos;
    o.pbr = v.pbr;
    o.uv = v.uv;
    o.tex = v.tex;
    o.flags = v.flags;
    return o;
}
// Edge lines: drawn just in front of the faces they outline.
@vertex fn vs_line(v: VIn) -> VOut {
    var o = vert(v);
    o.clip.z = o.clip.z - 0.0004 * o.clip.w;
    return o;
}
@fragment fn fs_line(i: VOut) -> @location(0) vec4<f32> {
    if (u.params.z > 0.5 && dot(u.clip.xyz, i.world) > u.clip.w) {
        discard;
    }
    if (hidden_wall(i)) {
        discard;
    }
    return vec4<f32>(0.07, 0.07, 0.09, 1.0);
}
// Walls nearer than the target and facing the camera hide when asked.
fn hidden_wall(i: VOut) -> bool {
    if (u.misc.y < 0.5 || i.flags < 0.5 || i.flags > 1.5) {
        return false;
    }
    let to_eye = u.eye.xyz - i.world;
    let fwd = normalize(u.aim.xyz - u.eye.xyz);
    return dot(i.normal, to_eye) > 0.0 && dot(-to_eye, fwd) < u.misc.z * 0.85;
}
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}
@vertex fn vs_shadow(v: VIn) -> @builtin(position) vec4<f32> {
    // Reference (ghosted) parts cast no shadow: put them outside the clip box.
    if (v.flags > 3.5) {
        return vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
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
    if (hidden_wall(i)) {
        discard;
    }
    let style = i32(u.style.x + 0.5);
    let v = normalize(u.eye.xyz - i.world);
    var n = normalize(i.normal);
    // Water (emissive -1): moving ripples tilt the normal.
    let water = i.pbr.z < -0.5;
    if (water) {
        let t = u.misc.x;
        let w = i.world.xy;
        let dx = 0.04 * cos(w.x * 3.1 + t * 1.3) + 0.03 * cos((w.x + w.y) * 5.7 + t * 2.1);
        let dy = 0.04 * cos(w.y * 2.7 + t * 1.1) + 0.03 * cos((w.x - w.y) * 6.3 + t * 1.7);
        n = normalize(n + vec3<f32>(dx, dy, 0.0));
    }
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    let l = normalize(u.light.xyz);
    let rough = clamp(i.pbr.x, 0.04, 1.0);
    let metal = clamp(i.pbr.y, 0.0, 1.0);
    // Textured surfaces take their colour from the texture (sampled
    // unconditionally to keep control flow uniform).
    let texel = textureSample(tex_array, tex_sampler, i.uv, max(i32(i.tex + 0.5), 0));
    var base = select(i.color.rgb, texel.rgb, i.tex > -0.5);
    if (style == 1) {
        base = vec3<f32>(0.84, 0.82, 0.79);
    }
    let sun = mix(1.0, sunlit(i.world), u.style.z) * u.params.y;
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
    // Moonlight is cool and dim.
    let sun_tint = mix(vec3<f32>(1.0), vec3<f32>(0.55, 0.65, 1.0), u.misc.w);
    let amb = mix(vec3<f32>(1.0), u.ambient.rgb, u.ambient.w);
    var color = (base * (1.0 - metal) * lit * amb + fresnel * spec * 0.6) * sun_tint + env + lamps + base * max(i.pbr.z, 0.0);
    let lum = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    if (style == 2) {
        // Technical illustration: flat colour, a little shading by angle.
        color = base * (0.7 + 0.3 * abs(dot(n, v)));
    } else if (style == 3) {
        // Watercolor: soft bands, paler, with paper grain.
        let band = floor(lum * 4.0 + 0.5) / 4.0;
        color = mix(color * (band / max(lum, 0.02)), vec3<f32>(1.0), 0.3);
        color = color * (0.93 + 0.07 * hash(floor(i.clip.xy / 3.0)));
    } else if (style == 4) {
        color = mix(u.duo_dark.rgb, u.duo_light.rgb, clamp(lum * 1.1, 0.0, 1.0));
    }
    color = color * u.style.w;
    // Depth cueing: fade toward the horizon colour with distance.
    let dist = length(u.eye.xyz - i.world);
    let fog = 1.0 - exp(-u.style.y * dist * 0.02);
    color = mix(color, u.horizon.rgb, clamp(fog, 0.0, 1.0));
    var alpha = clamp(i.pbr.w, 0.0, 1.0);
    if (style == 5 && i.flags > 0.5) {
        alpha = min(alpha, 0.28);
    }
    return vec4<f32>(color, alpha);
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
    tex_bgl: wgpu::BindGroupLayout,
    tex_bind: wgpu::BindGroup,
    tex_keys: Vec<TextureKey>,
    pipeline: wgpu::RenderPipeline,
    clear_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    /// Indices after `count` are edge lines.
    lines: u32,
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
    let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("housecraft textures"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("housecraft 3d"),
        bind_group_layouts: &[Some(&bgl), Some(&shadow_bgl), Some(&tex_bgl)],
        immediate_size: 0,
    });
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("housecraft shadow"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4, 4 => Float32x2, 5 => Float32, 6 => Float32
    ];
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
    let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("housecraft 3d lines"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_line"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
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
            entry_point: Some("fs_line"),
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
    let tex_bind = texture_bind(device, &rs.queue, &tex_bgl, &[]);
    rs.renderer.write().callback_resources.insert(Renderer3d {
        tex_bgl,
        tex_bind,
        tex_keys: Vec::new(),
        pipeline,
        clear_pipeline,
        line_pipeline,
        lines: 0,
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
    textures: Arc<Vec<TextureKey>>,
    vertices: Arc<Vec<Vertex>>,
    indices: Arc<Vec<u32>>,
    /// How many of `indices` are triangles; the rest are edge lines.
    triangles: u32,
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
        if r.tex_keys != *self.textures {
            r.tex_bind = texture_bind(device, queue, &r.tex_bgl, &self.textures);
            r.tex_keys = (*self.textures).clone();
        }
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
            r.count = self.triangles;
            r.lines = self.indices.len() as u32 - self.triangles;
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
        pass.set_bind_group(2, &r.tex_bind, &[]);
        pass.set_vertex_buffer(0, v.slice(..));
        pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..r.count, 0, 0..1);
        if r.lines > 0 {
            pass.set_pipeline(&r.line_pipeline);
            pass.draw_indexed(r.count..r.count + r.lines, 0, 0..1);
        }
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

/// The mesh categories as listed in the Display menu.
const CATEGORY_NAMES: [(Category, &str); 11] = [
    (Category::Wall, "Walls"),
    (Category::Opening, "Doors and windows"),
    (Category::Floor, "Floors"),
    (Category::Ceiling, "Ceilings"),
    (Category::Roof, "Roofs"),
    (Category::Foundation, "Foundations"),
    (Category::Stair, "Stairs and railings"),
    (Category::Fixture, "Fixtures and furniture"),
    (Category::Cabinet, "Cabinets"),
    (Category::Terrain, "Terrain and site"),
    (Category::Framing, "Framing"),
];

/// Distinct values mixed into the upload key for each hidden category.
const CATEGORY_KEYS: [Id; 11] = [
    Id(u64::MAX - 1),
    Id(u64::MAX - 2),
    Id(u64::MAX - 3),
    Id(u64::MAX - 4),
    Id(u64::MAX - 5),
    Id(u64::MAX - 6),
    Id(u64::MAX - 7),
    Id(u64::MAX - 8),
    Id(u64::MAX - 9),
    Id(u64::MAX - 10),
    Id(u64::MAX - 11),
];

/// Uploaded geometry: (selection/document key, vertices, indices).
type GpuMesh = (u64, Arc<Vec<Vertex>>, Arc<Vec<u32>>, u32);

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
    /// Mesh categories drawn as faint see-through reference (not pickable).
    pub ghosted: Vec<Category>,
    pub cutaway: Cutaway,
    /// How far the cutaway has sliced through, 0 to 1.
    pub cut: f32,
    /// The material painter, when active.
    pub painter: Option<Painter>,
    /// Image behind the model (the applied camera's backdrop).
    pub backdrop: Option<String>,
    /// Style, lines, fog, sky and lights for this view.
    pub render: housecraft_api::model::RenderSettings,
    /// The Render Settings window is open.
    pub render_window: bool,
    /// Seconds since start, for moving water.
    pub time: f32,
    /// A walkthrough playing: its id and when it started (s).
    pub playing: Option<(Id, f64)>,
    /// Keyframes being recorded and when the last was taken (s).
    pub recording: Option<(Vec<housecraft_api::model::Keyframe>, f64)>,
    /// Placing a 3D note or dimension (the first point of a dimension).
    pub annotate: Option<Annotate>,
    /// The camera last applied (3D notes can belong to one camera).
    pub camera: Option<Id>,
    /// The Lighting Manager window is open.
    pub lighting_window: bool,
    /// A panorama's average colour, cached by path.
    pano_tint: Option<(String, [f32; 3])>,
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
            ghosted: Vec::new(),
            cutaway: Cutaway::Off,
            cut: 0.5,
            mesh: None,
            gpu: None,
            gpu_key: 0,
            painter: None,
            backdrop: None,
            render: Default::default(),
            render_window: false,
            time: 0.0,
            playing: None,
            recording: None,
            annotate: None,
            camera: None,
            lighting_window: false,
            pano_tint: None,
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

    pub fn look_from(&mut self, eye: Vec3, target: Vec3) {
        let d = target - eye;
        let len = d.length().max(0.01);
        self.target = target;
        self.distance = len;
        self.yaw = d.y.atan2(d.x);
        self.pitch = (-d.z / len).asin();
    }

    /// Categories clicks pass through: hidden or shown only for reference.
    pub fn unpickable(&self) -> Vec<Category> {
        self.hidden.iter().chain(&self.ghosted).copied().collect()
    }

    /// How a category is shown: 0 normally, 1 as reference, 2 hidden.
    pub fn category_mode(&self, c: Category) -> u8 {
        if self.hidden.contains(&c) {
            2
        } else if self.ghosted.contains(&c) {
            1
        } else {
            0
        }
    }

    pub fn set_category_mode(&mut self, c: Category, mode: u8) {
        self.hidden.retain(|x| *x != c);
        self.ghosted.retain(|x| *x != c);
        match mode {
            1 => self.ghosted.push(c),
            2 => self.hidden.push(c),
            _ => {}
        }
    }

    pub fn apply_camera(&mut self, cam: &Camera) {
        self.camera = Some(cam.id);
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
        self.render = cam.render.clone();
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
            render: Some(self.render.clone()),
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

/// Whether to draw the flat ground plane, and its height (metres) when a
/// terrain extends to the horizon.
fn ground(doc: &housecraft_api::model::Document) -> (bool, Option<f32>) {
    match doc.iter::<housecraft_api::model::Terrain>().next() {
        None => (true, None),
        Some(t) if t.extend_to_horizon => (true, Some((t.base_elevation.mm() / 1000.0) as f32 - 0.005)),
        Some(_) => (false, None),
    }
}

fn to_gpu(
    mesh: &Mesh,
    selection: &[Id],
    hidden: &[Category],
    ghosted: &[Category],
    grade_z: Option<f32>,
    ground: (bool, Option<f32>),
    outlines: bool,
) -> (Vec<Vertex>, Vec<u32>, u32) {
    let mut vertices: Vec<Vertex> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .zip(&mesh.colors)
        .zip(&mesh.pbr)
        .zip(mesh.uvs.iter().zip(&mesh.tex))
        .map(|((((p, n), c), m), (uv, t))| Vertex {
            pos: *p,
            normal: *n,
            color: *c,
            pbr: *m,
            uv: *uv,
            tex: *t,
            flags: 0.0,
        })
        .collect();
    let accent = [1.0, 0.55, 0.1, 1.0];
    for part in &mesh.parts {
        let flag = match part.category {
            Category::Wall => 1.0,
            Category::Roof => 2.0,
            _ => 0.0,
        };
        if flag > 0.0 {
            let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
            for &i in &mesh.indices[range] {
                vertices[i as usize].flags = flag;
            }
        }
        if selection.contains(&part.object) {
            let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
            for &i in &mesh.indices[range] {
                let c = &mut vertices[i as usize].color;
                for k in 0..3 {
                    c[k] = c[k] * 0.4 + accent[k] * 0.6;
                }
            }
        }
        if ghosted.contains(&part.category) {
            // Reference display: pale, untextured and see-through.
            let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
            for &i in &mesh.indices[range] {
                let v = &mut vertices[i as usize];
                let grey = (v.color[0] + v.color[1] + v.color[2]) / 3.0;
                for k in 0..3 {
                    v.color[k] = 0.55 + grey * 0.3;
                }
                v.pbr = [0.9, 0.0, 0.0, 0.22];
                v.tex = -1.0;
                v.flags = 4.0;
            }
        }
    }
    // Ground plane under everything (with a terrain, only out to the
    // horizon when asked).
    let mut ground_base = None;
    if let Some((lo, hi)) = mesh_bounds(mesh)
        && ground.0
    {
        // Out toward the horizon (seen in walkthroughs and panoramas).
        let pad = if ground.1.is_some() { 3000.0 } else { ((hi - lo).length() * 0.6).max(400.0) };
        let z = ground.1.or(grade_z).unwrap_or(lo.z - 0.02);
        let base = vertices.len() as u32;
        ground_base = Some(base);
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
                uv: [0.0, 0.0],
                tex: -1.0,
                flags: 0.0,
            });
        }
    }
    let mut indices = visible_indices(mesh, hidden);
    // Edge lines outline the solid model only, not the ground or reference parts.
    let lines = if outlines {
        let hide_too: Vec<Category> = hidden.iter().chain(ghosted).copied().collect();
        edge_lines(mesh, &visible_indices(mesh, &hide_too), &mut vertices)
    } else {
        Vec::new()
    };
    if let Some(base) = ground_base {
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let triangles = indices.len() as u32;
    indices.extend(lines);
    (vertices, indices, triangles)
}

/// Index pairs (into `vertices`, appended) for the crease and boundary
/// edges of the visible triangles: where faces meet at more than 25°.
fn edge_lines(mesh: &Mesh, tris: &[u32], vertices: &mut Vec<Vertex>) -> Vec<u32> {
    use std::collections::HashMap;
    let key = |i: u32| {
        let p = mesh.positions[i as usize];
        [(p[0] * 1000.0).round() as i64, (p[1] * 1000.0).round() as i64, (p[2] * 1000.0).round() as i64]
    };
    // Edge (its ends, rounded to mm) → (its vertices, the faces' normals).
    type Edges = HashMap<([i64; 3], [i64; 3]), (u32, u32, Vec<Vec3>)>;
    let mut edges: Edges = HashMap::new();
    for t in tris.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| Vec3::from_array(mesh.positions[i as usize]));
        let n = (b - a).cross(c - a).normalize_or_zero();
        if n == Vec3::ZERO {
            continue;
        }
        for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ki, kj) = (key(i), key(j));
            let k = if ki < kj { (ki, kj) } else { (kj, ki) };
            edges.entry(k).or_insert((i, j, Vec::new())).2.push(n);
        }
    }
    let cos = 35f32.to_radians().cos();
    let mut out = Vec::new();
    for (_, (i, j, normals)) in edges {
        let crease = normals.len() == 1
            || normals.iter().any(|a| normals.iter().any(|b| a.dot(*b).abs() < cos && a.dot(*b) < cos));
        if crease {
            let base = vertices.len() as u32;
            for k in [i, j] {
                let mut v = vertices[k as usize];
                v.color = [0.0, 0.0, 0.0, 1.0];
                vertices.push(v);
            }
            out.extend([base, base + 1]);
        }
    }
    out
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
    pick_hit(mesh, origin, dir, hidden).map(|(id, s, _)| (id, s))
}

/// The nearest surface along a ray: its object, surface role and distance.
fn pick_hit(
    mesh: &Mesh,
    origin: Vec3,
    dir: Vec3,
    hidden: &[Category],
) -> Option<(Id, Option<housecraft_api::model::Surface>, f32)> {
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
    best.map(|(t, id, v)| (id, mesh.surfaces.get(v as usize).copied().flatten(), t))
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
    // Walkthroughs: play one, or record keyframes as the camera moves.
    let now = ui.input(|i| i.time);
    if let Some((id, start)) = v.playing {
        match app.session.document().get::<housecraft_api::model::Walkthrough>(id) {
            Some(w) if now - start <= w.duration() => {
                if let Some((e, t)) = w.sample(now - start) {
                    let m = |p: [f64; 3]| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 1000.0;
                    v.look_from(m(e), m(t));
                    v.ortho = false;
                }
                ui.ctx().request_repaint();
            }
            _ => v.playing = None,
        }
    }
    let (eye, target) = (v.eye(), v.target);
    if let Some((keys, last)) = &mut v.recording {
        let mm = |p: Vec3| Point3::from_mm(p.x as f64 * 1000.0, p.y as f64 * 1000.0, p.z as f64 * 1000.0);
        let moved = keys.last().is_none_or(|k: &housecraft_api::model::Keyframe| {
            let [x, y, z] = k.eye.mm();
            Vec3::new(x as f32, y as f32, z as f32).distance(eye * 1000.0) > 300.0
                || {
                    let [x, y, z] = k.target.mm();
                    Vec3::new(x as f32, y as f32, z as f32).distance(target * 1000.0) > 300.0
                }
        });
        if now - *last >= 1.0 && moved {
            let seconds = if keys.is_empty() { 0.0 } else { (now - *last).min(4.0) };
            keys.push(housecraft_api::model::Keyframe { eye: mm(eye), target: mm(target), seconds, pause: 0.0 });
            *last = now;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
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
        let dir = (far - near).normalize();
        if let Some(mode) = app.view3d.annotate {
            if let Some((_, _, t)) = pick_hit(&mesh, near, dir, &app.view3d.unpickable()) {
                let p = near + dir * t;
                let mm = |v: Vec3| Point3::from_mm(v.x as f64 * 1000.0, v.y as f64 * 1000.0, v.z as f64 * 1000.0);
                let camera = app.view3d.camera;
                match mode {
                    Annotate::Note => {
                        app.view3d.annotate = None;
                        app.note3d = Some((mm(p), String::new(), camera));
                    }
                    Annotate::Dimension(None) => app.view3d.annotate = Some(Annotate::Dimension(Some(p))),
                    Annotate::Dimension(Some(a)) => {
                        app.view3d.annotate = None;
                        app.run1(Command::AddAnnotation3d {
                            camera,
                            mark: housecraft_api::model::Mark3d::Dimension { a: mm(a), b: mm(p) },
                        });
                    }
                }
            }
            v_restore(app, mesh_rev, mesh);
            return;
        }
        let hit = pick(&mesh, near, dir, &app.view3d.unpickable());
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
        .fold(mesh_rev.wrapping_mul(1_000_003) ^ v.render.outlines as u64, |h, id| {
            h.wrapping_mul(31).wrapping_add(id.0)
        });
    let sel_hash = v
        .ghosted
        .iter()
        .fold(sel_hash, |h, c| h.wrapping_mul(37).wrapping_add(CATEGORY_KEYS[*c as usize].0 ^ 0x5a5a));
    if v.gpu.as_ref().is_none_or(|g| g.0 != sel_hash) {
        let (verts, idx, tris) = to_gpu(
            &mesh,
            &app.selection,
            &v.hidden,
            &v.ghosted,
            grade(app.session.document()),
            ground(app.session.document()),
            v.render.outlines,
        );
        v.gpu_key = v.gpu_key.wrapping_add(1);
        v.gpu = Some((sel_hash, Arc::new(verts), Arc::new(idx), tris));
    }
    let (_, verts, idx, triangles) = v.gpu.clone().expect("uploaded");
    // A panorama's colour for lighting (worked out once per image).
    if let Some(p) = &v.render.panorama
        && v.pano_tint.as_ref().is_none_or(|(path, _)| *path != p.path)
        && let Some(c) = crate::images::average_color(&p.path)
    {
        let [r, g, b, _] = c.0;
        v.pano_tint = Some((p.path.clone(), [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]));
    }
    // Water moves: keep drawing while any is in view.
    v.time = ui.input(|i| i.time) as f32;
    if mesh.pbr.iter().any(|p| p[2] < -0.5) {
        ui.ctx().request_repaint();
    }
    v.mesh = Some((mesh_rev, mesh));

    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    // The sky: a gradient from the zenith down to the horizon (the theme's
    // plain background in the dark theme's default).
    let (zenith, horizon) = sky_colors(&v.render);
    let c32 = |c: housecraft_api::model::Color| {
        let [r, g, b, _] = c.0;
        egui::Color32::from_rgb(r, g, b)
    };
    if let Some(pano) = v.render.panorama.clone() {
        paint_panorama(ui.ctx(), &painter, rect, v, &pano, vp);
    } else if dark && v.render.sky == housecraft_api::model::Sky::default() {
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(38, 44, 54));
    } else {
        let mut sky = egui::Mesh::default();
        sky.colored_vertex(rect.left_top(), c32(zenith));
        sky.colored_vertex(rect.right_top(), c32(zenith));
        sky.colored_vertex(rect.right_bottom(), c32(horizon));
        sky.colored_vertex(rect.left_bottom(), c32(horizon));
        sky.add_triangle(0, 1, 2);
        sky.add_triangle(0, 2, 3);
        painter.add(sky);
    }
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
    let textures = Arc::new(v.mesh.as_ref().map(|(_, m)| m.textures.clone()).unwrap_or_default());
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        Paint3d {
            textures,
            vertices: verts,
            indices: idx,
            triangles,
            key: v.gpu_key,
            uniforms,
        },
    ));
    annotations(app, &painter, rect, vp);
    overlay(app, ui, rect);
    painter_bar(app, ui, rect);
}

/// Put the mesh back after an early return from `show`.
fn v_restore(app: &mut App, rev: u64, mesh: Mesh) {
    app.view3d.mesh = Some((rev, mesh));
}

/// What a click in 3D places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Annotate {
    Note,
    /// The first end once clicked.
    Dimension(Option<Vec3>),
}

/// A 360° image behind the model: a grid over the view whose corners look
/// up the image by direction.
fn paint_panorama(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: Rect,
    v: &View3d,
    pano: &housecraft_api::model::Panorama,
    vp: Mat4,
) {
    let Some(tex) = crate::images::texture_repeat(ctx, &pano.path) else {
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(90, 100, 110));
        return;
    };
    let inv = vp.inverse();
    let eye = v.eye();
    let (nx, ny) = (24, 16);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let uv_at = |sx: f32, sy: f32| {
        let ndc = Vec4::new(sx * 2.0 - 1.0, 1.0 - sy * 2.0, 1.0, 1.0);
        let far = inv * ndc;
        let d = (far.truncate() / far.w - eye).normalize_or_zero();
        let lon = d.y.atan2(d.x) - (pano.rotation as f32).to_radians();
        let lat = d.z.clamp(-1.0, 1.0).asin();
        let u = 0.5 - lon / std::f32::consts::TAU;
        let vv = (0.5 - lat / std::f32::consts::PI - pano.horizon as f32 * 0.25).clamp(0.0, 1.0);
        egui::pos2(u, vv)
    };
    for j in 0..=ny {
        for i in 0..=nx {
            let (sx, sy) = (i as f32 / nx as f32, j as f32 / ny as f32);
            let pos = egui::pos2(rect.left() + sx * rect.width(), rect.top() + sy * rect.height());
            mesh.vertices.push(egui::epaint::Vertex { pos, uv: uv_at(sx, sy), color: egui::Color32::WHITE });
        }
    }
    let w = nx + 1;
    for j in 0..ny {
        for i in 0..nx {
            let k = (j * w + i) as u32;
            // Keep each quad on one side of the image's seam.
            let us: Vec<f32> = [k, k + 1, k + w as u32, k + w as u32 + 1].iter().map(|q| mesh.vertices[*q as usize].uv.x).collect();
            let lo = us.iter().copied().fold(f32::MAX, f32::min);
            if us.iter().any(|u| u - lo > 0.5) {
                for q in [k, k + 1, k + w as u32, k + w as u32 + 1] {
                    let vx = &mut mesh.vertices[q as usize];
                    if vx.uv.x - lo > 0.5 {
                        // Unwrap: the repeating texture carries on past 1.
                        vx.uv.x -= 1.0;
                    }
                }
            }
            mesh.add_triangle(k, k + 1, k + w as u32 + 1);
            mesh.add_triangle(k, k + w as u32 + 1, k + w as u32);
        }
    }
    painter.add(mesh);
}

/// 3D notes and dimensions over the view (the ones for this camera or all).
fn annotations(app: &App, painter: &egui::Painter, rect: Rect, vp: Mat4) {
    let doc = app.session.document();
    let to_screen = |p: Point3| {
        let [x, y, z] = p.mm();
        let c = vp * Vec4::new(x as f32 / 1000.0, y as f32 / 1000.0, z as f32 / 1000.0, 1.0);
        (c.w > 0.0).then(|| egui::pos2(rect.left() + (c.x / c.w * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - c.y / c.w * 0.5) * rect.height()))
    };
    let ink = egui::Color32::from_rgb(20, 20, 24);
    let paper = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 220);
    let font = egui::FontId::proportional(14.0);
    let label = |at: egui::Pos2, text: String| {
        let galley = painter.layout_no_wrap(text, font.clone(), ink);
        let r = egui::Rect::from_center_size(at, galley.size()).expand(3.0);
        painter.rect_filled(r, 3.0, paper);
        painter.galley(r.min + egui::vec2(3.0, 3.0), galley, ink);
    };
    for a in doc.iter::<housecraft_api::model::Annotation3d>() {
        if a.camera.is_some() && a.camera != app.view3d.camera {
            continue;
        }
        match &a.mark {
            housecraft_api::model::Mark3d::Note { at, text } => {
                if let Some(p) = to_screen(*at) {
                    painter.circle_filled(p, 3.5, ink);
                    let at = p + egui::vec2(30.0, -24.0);
                    painter.line_segment([p, at], egui::Stroke::new(1.2, ink));
                    label(at, text.clone());
                }
            }
            housecraft_api::model::Mark3d::Dimension { a, b } => {
                if let (Some(pa), Some(pb)) = (to_screen(*a), to_screen(*b)) {
                    let stroke = egui::Stroke::new(1.5, ink);
                    painter.line_segment([pa, pb], stroke);
                    let n = (pb - pa).normalized().rot90() * 6.0;
                    for p in [pa, pb] {
                        painter.line_segment([p - n, p + n], stroke);
                    }
                    let d: f64 = a.mm().iter().zip(b.mm()).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt();
                    label(pa + (pb - pa) * 0.5, housecraft_api::units::Length::from_mm(d).format(doc.settings.display_unit));
                }
            }
        }
    }
    if let Some(Annotate::Dimension(Some(first))) = app.view3d.annotate {
        let c = vp * first.extend(1.0);
        if c.w > 0.0 {
            let p = egui::pos2(rect.left() + (c.x / c.w * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - c.y / c.w * 0.5) * rect.height());
            painter.circle_filled(p, 4.0, egui::Color32::from_rgb(255, 140, 30));
        }
    }
}

/// The Lighting Manager: every light in the plan, switched and dimmed in
/// one table.
pub fn lighting_window(app: &mut App, ctx: &egui::Context) {
    if !app.view3d.lighting_window {
        return;
    }
    let mut open = true;
    let mut commands = Vec::new();
    let doc = app.session.document();
    egui::Window::new("Lighting Manager").open(&mut open).default_width(560.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            egui::Grid::new("lights").num_columns(6).striped(true).show(ui, |ui| {
                for h in ["Light", "On", "Dimmer", "Lumens", "Kelvin", "Shadows"] {
                    ui.strong(h);
                }
                ui.end_row();
                let mut row = |ui: &mut Ui, id: Id, name: String, s: housecraft_api::model::LightSettings, field: &str| {
                    ui.label(name);
                    let mut t = s;
                    ui.checkbox(&mut t.on, "");
                    ui.add(egui::Slider::new(&mut t.dimmer, 0.0..=1.0).show_value(false));
                    ui.add(egui::DragValue::new(&mut t.lumens).range(0.0..=20000.0).speed(10.0));
                    ui.add(egui::DragValue::new(&mut t.kelvin).range(1500.0..=10000.0).speed(20.0));
                    ui.checkbox(&mut t.shadows, "");
                    ui.end_row();
                    if t != s {
                        let set = match field {
                            "light" => serde_json::json!({ "light": t }),
                            _ => serde_json::json!({ "behavior": { "type": "light", "light": t } }),
                        };
                        commands.push(Command::Update { id, set: set.as_object().expect("object").clone() });
                    }
                };
                for e in doc.iter::<housecraft_api::model::Electrical>().filter(|e| e.electrical_kind.emits_light()) {
                    let s = e.light.unwrap_or_else(|| housecraft_api::model::LightSettings::typical(e.electrical_kind));
                    let floor = doc.get::<housecraft_api::model::Floor>(e.floor).map_or(String::new(), |f| f.name.clone());
                    row(ui, e.id, format!("{} ({floor})", e.electrical_kind.name()), s, "light");
                }
                for d in doc.iter::<housecraft_api::model::SymbolDef>() {
                    if let housecraft_api::model::SymbolBehavior::Light { light } = &d.behavior {
                        row(ui, d.id, format!("{} (every copy)", d.name), *light, "behavior");
                    }
                }
            });
        });
        ui.weak("To turn lights off in one view only, use Render Settings > Lights in this view.");
    });
    app.view3d.lighting_window = open;
    if !commands.is_empty() {
        app.run(commands);
    }
}

/// The shader inputs for a frame: camera, sun, shadows, cutaway, lights.
fn frame_uniforms(v: &View3d, doc: &housecraft_api::model::Document, vp: Mat4) -> Uniforms {
    let r = &v.render;
    let light = match &r.sun {
        Some(sun) => housecraft_api::geom::sun::direction_of(sun, doc.settings.north).as_vec3(),
        None => sun_direction(doc),
    };
    // Below the horizon the sun stops lighting and casting; at night the
    // moon gives a little.
    let night = r.sky.night;
    let strength = (light.z / 0.1).clamp(0.0, 1.0) * r.sky.sun_brightness as f32 * if night { 0.12 } else { 1.0 };
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
        .map(|(_, m)| m.lights.iter().filter(|l| !r.lights_off.contains(&l.object)).collect())
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
                style: [
                    r.style.index() as f32,
                    r.fog as f32,
                    if r.style == housecraft_api::model::RenderStyle::Technical { 0.0 } else { r.shadow_intensity as f32 },
                    r.exposure as f32,
                ],
                duo_dark: color4(r.duotone_dark),
                duo_light: color4(r.duotone_light),
                horizon: color4(sky_colors(r).1),
                misc: [
                    v.time,
                    if r.hide_near_walls { 1.0 } else { 0.0 },
                    eye.distance(v.target),
                    if night { 1.0 } else { 0.0 },
                ],
                target: v.target.extend(1.0).to_array(),
                ambient: match (&r.panorama, &v.pano_tint) {
                    (Some(p), Some((path, c))) if p.lights_scene && *path == p.path => {
                        // Normalise so the tint changes colour, not brightness.
                        let m = c[0].max(c[1]).max(c[2]).max(0.01);
                        [c[0] / m, c[1] / m, c[2] / m, 0.7]
                    }
                    _ => [1.0, 1.0, 1.0, 0.0],
                },
            }
}

/// A colour as linear RGBA for the shader.
fn color4(c: housecraft_api::model::Color) -> [f32; 4] {
    let [r, g, b, _] = c.0;
    let f = |v: u8| (v as f32 / 255.0).powf(2.2);
    [f(r), f(g), f(b), 1.0]
}

/// The sky's zenith and horizon colours (darkened at night).
pub fn sky_colors(r: &housecraft_api::model::RenderSettings) -> (housecraft_api::model::Color, housecraft_api::model::Color) {
    let dim = |c: housecraft_api::model::Color, k: f32| {
        let [r, g, b, _] = c.0;
        let f = |v: u8| (v as f32 * k) as u8;
        housecraft_api::model::Color::rgb(f(r), f(g), f(b))
    };
    if r.sky.night {
        (dim(r.sky.zenith, 0.12), dim(r.sky.horizon, 0.25))
    } else {
        (r.sky.zenith, r.sky.horizon)
    }
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
                // Each kind of object shown, shown as faint reference, or hidden.
                let keep_open = egui::containers::menu::MenuConfig::new()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                let (display, _) = egui::containers::menu::MenuButton::new("Display").config(keep_open).ui(ui, |ui| {
                    egui::Grid::new("category_modes").num_columns(4).show(ui, |ui| {
                        ui.label("");
                        ui.label("Show");
                        ui.label("Reference");
                        ui.label("Hide");
                        ui.end_row();
                        for (c, name) in CATEGORY_NAMES {
                            ui.label(name);
                            let mut mode = v.category_mode(c);
                            for m in 0..3u8 {
                                ui.radio_value(&mut mode, m, "");
                            }
                            if mode != v.category_mode(c) {
                                v.set_category_mode(c, mode);
                            }
                            ui.end_row();
                        }
                    });
                });
                display.on_hover_text("Show, ghost (reference) or hide each kind of object");
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
                ui.separator();
                egui::ComboBox::from_id_salt("render_style")
                    .selected_text(v.render.style.name())
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        for s in housecraft_api::model::RenderStyle::ALL {
                            ui.selectable_value(&mut v.render.style, s, s.name());
                        }
                    });
                if ui.button("Render…").on_hover_text("Lines, shadows, fog, sky and lights for this view").clicked() {
                    v.render_window = !v.render_window;
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
                // Adjust the material's texture live (scale and turn).
                let mut edits = None;
                if let Some(m) = p.material.and_then(|m| doc.get::<housecraft_api::model::Material>(m))
                    && (m.maps.color.is_some() || m.pattern.is_some())
                {
                    ui.separator();
                    let mut scale = m.texture_width.map_or(48.0, |l| l.inches());
                    let mut angle = m.texture_angle;
                    let r1 = ui.add(egui::DragValue::new(&mut scale).range(1.0..=480.0).speed(0.5).prefix("tile ").suffix("\""));
                    let r2 = ui.add(egui::DragValue::new(&mut angle).range(-180.0..=180.0).speed(1.0).prefix("turn ").suffix("°"));
                    if r1.changed() || r2.changed() {
                        let ratio = m
                            .texture_height
                            .zip(m.texture_width)
                            .map_or(1.0, |(h, w)| h.mm() / w.mm().max(1.0));
                        edits = Some((m.id, scale, scale * ratio, angle));
                    }
                }
                ui.weak("Click to paint · Alt+click picks up a material");
                if let Some((id, w, h, a)) = edits {
                    app.run1(Command::Update {
                        id,
                        set: serde_json::json!({
                            "texture_width": Length::from_inches(w),
                            "texture_height": Length::from_inches(h),
                            "texture_angle": a,
                        })
                        .as_object()
                        .expect("object")
                        .clone(),
                    });
                }
                if ui.button("Done").clicked() {
                    close = true;
                }
            });
        });
    });
    app.view3d.painter = (!close).then_some(p);
}

/// The pixels of one texture layer: an image file (resized) or a
/// generated pattern; mid-grey when a file can't be read.
fn texture_pixels(key: &TextureKey) -> Vec<u8> {
    let rgba = match key {
        TextureKey::File(path) => image::open(path).ok().map(|i| i.to_rgba8()),
        TextureKey::Pattern(p, color) => {
            let (w, h, px) = housecraft_api::geom::pattern::image(p, *color, TEX_SIZE);
            image::RgbaImage::from_raw(w, h, px)
        }
    };
    match rgba {
        Some(img) => image::imageops::resize(&img, TEX_SIZE, TEX_SIZE, image::imageops::FilterType::Triangle)
            .into_raw(),
        None => vec![128; (TEX_SIZE * TEX_SIZE * 4) as usize],
    }
}

/// A texture array holding `keys` (at least one layer) and its bind group.
fn texture_bind(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    keys: &[TextureKey],
) -> wgpu::BindGroup {
    let max = device.limits().max_texture_array_layers.max(1);
    let layers = (keys.len() as u32).clamp(1, max);
    let size = wgpu::Extent3d {
        width: TEX_SIZE,
        height: TEX_SIZE,
        depth_or_array_layers: layers,
    };
    let mips = TEX_SIZE.ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("housecraft textures"),
        size,
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, key) in keys.iter().take(layers as usize).enumerate() {
        // Each smaller level averages the one above, so distant patterns
        // blend instead of shimmering.
        let mut level = image::RgbaImage::from_raw(TEX_SIZE, TEX_SIZE, texture_pixels(key))
            .expect("full-size layer");
        for mip in 0..mips {
            let side = (TEX_SIZE >> mip).max(1);
            if mip > 0 {
                level = image::imageops::resize(&level, side, side, image::imageops::FilterType::Triangle);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                level.as_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("housecraft textures"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("housecraft textures"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

/// The Render Settings window: how this 3D view is drawn. Saved with a
/// camera made from the view.
pub fn render_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.view3d.render_window;
    if !open {
        return;
    }
    let lights: Vec<(Id, String)> = app
        .view3d
        .mesh
        .as_ref()
        .map(|(_, m)| {
            m.lights
                .iter()
                .map(|l| (l.object, housecraft_api::query::describe(app.session.document(), app.session.document().object(l.object).expect("light")).label))
                .collect()
        })
        .unwrap_or_default();
    let doc_sun = app.session.document().settings.sun;
    let r = &mut app.view3d.render;
    let color = |ui: &mut Ui, c: &mut housecraft_api::model::Color| {
        let [cr, cg, cb, _] = c.0;
        let mut c32 = egui::Color32::from_rgb(cr, cg, cb);
        if ui.color_edit_button_srgba(&mut c32).changed() {
            *c = housecraft_api::model::Color::rgb(c32.r(), c32.g(), c32.b());
        }
    };
    egui::Window::new("Render Settings").open(&mut open).default_width(320.0).show(ctx, |ui| {
        egui::Grid::new("render_settings").num_columns(2).show(ui, |ui| {
            ui.label("Style");
            egui::ComboBox::from_id_salt("render_style_w").selected_text(r.style.name()).show_ui(ui, |ui| {
                for s in housecraft_api::model::RenderStyle::ALL {
                    ui.selectable_value(&mut r.style, s, s.name());
                }
            });
            ui.end_row();
            ui.label("Lines");
            ui.checkbox(&mut r.outlines, "Draw edges over the shading");
            ui.end_row();
            ui.label("Shadows");
            ui.add(egui::Slider::new(&mut r.shadow_intensity, 0.0..=1.0));
            ui.end_row();
            ui.label("Fog (depth cueing)");
            ui.add(egui::Slider::new(&mut r.fog, 0.0..=1.0));
            ui.end_row();
            ui.label("Brightness");
            ui.add(egui::Slider::new(&mut r.exposure, 0.2..=3.0).logarithmic(true));
            ui.end_row();
            if r.style == housecraft_api::model::RenderStyle::Duotone {
                ui.label("Duotone");
                ui.horizontal(|ui| {
                    color(ui, &mut r.duotone_dark);
                    color(ui, &mut r.duotone_light);
                });
                ui.end_row();
            }
            ui.label("Sky");
            ui.horizontal(|ui| {
                color(ui, &mut r.sky.zenith);
                color(ui, &mut r.sky.horizon);
                ui.checkbox(&mut r.sky.night, "Night (moonlight)");
            });
            ui.end_row();
            ui.label("Sun brightness");
            ui.add(egui::Slider::new(&mut r.sky.sun_brightness, 0.0..=2.0));
            ui.end_row();
            ui.label("Sun for this view");
            ui.horizontal(|ui| {
                let mut own = r.sun.is_some();
                if ui.checkbox(&mut own, "Own sun angle").on_hover_text("A sun study: this view keeps its own sun").changed() {
                    r.sun = own.then_some(doc_sun);
                }
                if let Some(sun) = &mut r.sun {
                    ui.add(egui::DragValue::new(&mut sun.azimuth).range(0.0..=360.0).suffix("° bearing"));
                    ui.add(egui::DragValue::new(&mut sun.altitude).range(-10.0..=90.0).suffix("° high"));
                }
            });
            ui.end_row();
            ui.label("Walls");
            ui.checkbox(&mut r.hide_near_walls, "Hide walls between me and the view");
            ui.end_row();
            ui.label("360° backdrop");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let name = r.panorama.as_ref().map_or("None".to_string(), |p| {
                        std::path::Path::new(&p.path).file_name().map_or(p.path.clone(), |f| f.to_string_lossy().into_owned())
                    });
                    ui.label(name);
                    if ui.small_button("Choose…").clicked()
                        && let Some(f) = crate::props::pick_image()
                    {
                        r.panorama = Some(housecraft_api::model::Panorama { path: f, rotation: 0.0, horizon: 0.0, lights_scene: true });
                    }
                    if r.panorama.is_some() && ui.small_button("Remove").clicked() {
                        r.panorama = None;
                    }
                });
                if let Some(p) = &mut r.panorama {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut p.rotation).range(-180.0..=180.0).suffix("° turn"));
                        ui.add(egui::Slider::new(&mut p.horizon, -1.0..=1.0).text("horizon"));
                    });
                    ui.checkbox(&mut p.lights_scene, "Light the model with its colours");
                }
            });
            ui.end_row();
        });
        if !lights.is_empty() {
            egui::CollapsingHeader::new(format!("Lights in this view ({})", lights.len())).show(ui, |ui| {
                for (id, label) in &lights {
                    let mut on = !r.lights_off.contains(id);
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            r.lights_off.retain(|x| x != id);
                        } else {
                            r.lights_off.push(*id);
                        }
                    }
                }
            });
        }
        ui.weak("Save the view as a camera (3D > Save Current 3D View as Camera) to keep these settings.");
    });
    app.view3d.render_window = open;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(super::SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(super::SHADER)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }
}
