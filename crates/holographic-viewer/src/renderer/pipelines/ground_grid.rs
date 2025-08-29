// Renders a dynamic, adaptive grid on a local tangent plane to the WGS‑84 ellipsoid.

use crate::camera::Camera;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

// --- Geodetic Helpers for Grid Convergence ---

/// Calculates the central meridian of a standard UTM zone in degrees longitude.
fn utm_central_meridian_deg(lon_deg: f64) -> f64 {
    // UTM zone number (standard formula)
    let zone = ((lon_deg + 180.0) / 6.0).floor() + 1.0;
    // Central meridian of that zone
    -183.0 + 6.0 * zone
}

/// Calculates the meridian (grid) convergence angle, γ, in radians.
fn meridian_convergence_rad(lat_deg: f64, lon_deg: f64) -> f64 {
    let phi = lat_deg.to_radians();
    let lam = lon_deg.to_radians();
    let lam0 = utm_central_meridian_deg(lon_deg).to_radians();
    ((lam - lam0).tan() * phi.sin()).atan()
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniforms {
    /// Transform from model space to clip space.
    pub model_view_proj: Mat4,     // 64 B
    /// Camera height above the tangent plane, meters.
    pub camera_height_m: f32,      // +4
    pub _pad0: [f32; 3],           // +12 -> 80
    /// EN offset (meters) relative to world anchor; vec2 takes a 16‑B slot in uniforms.
    pub enu_offset_m: [f32; 2],    // +8
    pub _pad1: [f32; 2],           // +8  -> 96  (pad vec2 to 16)
    /// Half‑extent (meters) from center to edge.
    pub plane_extent_m: f32,       // +4
    pub _pad2: [f32; 3],           // +12 -> 112 (struct size rounded to 16)
}

// Compile‑time safety check: buffer size must match WGSL‑reflected size.
const _: [(); 112] = [(); core::mem::size_of::<GridUniforms>()];

pub struct GroundGridPipeline {
    pipeline:       wgpu::RenderPipeline,
    bind_group:     wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    quad_vb:        wgpu::Buffer,
    origin_ecef_m:  [f64; 3],   // dataset/world anchor
    plane_extent_m: f32,        // meters from center to edge
}

impl GroundGridPipeline {
    pub fn new(
        device:     &wgpu::Device,
        color_fmt:  wgpu::TextureFormat,
        dlin_fmt:   wgpu::TextureFormat,
        depth_fmt:  wgpu::TextureFormat,
    ) -> Self {
        // Uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:               Some("Grid Uniform Buffer"),
            size:                std::mem::size_of::<GridUniforms>() as u64,
            usage:               wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("Grid BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });

        // Bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("Grid Bind Group"),
            layout:  &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Full‑screen quad (two triangles)
        let corners: [[f32; 2]; 6] = [
            [-1.0, -1.0], [1.0, -1.0], [1.0, 1.0],
            [-1.0, -1.0], [1.0, 1.0],  [-1.0, 1.0],
        ];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("Grid Quad VB"),
            contents: bytemuck::cast_slice(&corners),
            usage:    wgpu::BufferUsages::VERTEX,
        });

        // Shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("Grid WGSL"),
            source: wgpu::ShaderSource::Wgsl(GRID_WGSL.into()),
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:               Some("Grid Pipeline Layout"),
            bind_group_layouts:  &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:   Some("Ground Grid Pipeline"),
            layout:  Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:            &shader,
                entry_point:       "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as u64,
                    step_mode:    wgpu::VertexStepMode::Vertex,
                    attributes:   &[wgpu::VertexAttribute {
                        shader_location: 0,
                        format:          wgpu::VertexFormat::Float32x2,
                        offset:          0,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:            &shader,
                entry_point:       "fs_main",
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format:     color_fmt,
                        blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format:     dlin_fmt,
                        blend:      None, // Direct tag write
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive:   wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format:                 depth_fmt,
                depth_write_enabled:    false, // Do not occlude points
                depth_compare:          wgpu::CompareFunction::LessEqual,
                stencil:                wgpu::StencilState::default(),
                bias:                   wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview:   None,
        });

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            quad_vb,
            origin_ecef_m: [0.0; 3],
            plane_extent_m: 500_000.0,
        }
    }

    /// Set the world anchor used to stabilize the grid in EN coordinates (meters).
    pub fn set_origin(&mut self, ecef_m: [f64; 3]) {
        self.origin_ecef_m = ecef_m;
    }

    pub fn draw<'a>(
        &'a self,
        rpass:  &mut wgpu::RenderPass<'a>,
        queue:  &wgpu::Queue,
        camera: &Camera,
        grid_utm_align: bool,
    ) {
        // Tangent point on the ellipsoid (height = 0)
        let tangent_ecef = hypc::geodetic_to_ecef(camera.lat_deg, camera.lon_deg, 0.0);
        let cam_ecef = camera.ecef_m();

        // Camera relative position to tangent point
        let view_proj = camera.view_proj_ecef();
        let rel_pos = Vec3::new(
            (tangent_ecef[0] - cam_ecef[0]) as f32,
            (tangent_ecef[1] - cam_ecef[1]) as f32,
            (tangent_ecef[2] - cam_ecef[2]) as f32,
        );

        // Calculate grid convergence angle (gamma) if aligning to UTM grid north.
        let gamma = if grid_utm_align {
            meridian_convergence_rad(camera.lat_deg, camera.lon_deg) as f32
        } else {
            0.0
        };

        // Build a model that aligns the plane with ENU (world), NOT the camera.
        let r_ecef_to_enu = camera.ecef_to_enu_matrix();
        let r_enu_to_ecef = r_ecef_to_enu.transpose(); // orthonormal inverse
        let model = Mat4::from_translation(rel_pos)
            * Mat4::from_mat3(r_enu_to_ecef)
            * Mat4::from_rotation_z(gamma) // Rotate grid to UTM grid-north
            * Mat4::from_scale(Vec3::splat(self.plane_extent_m));

        // EN offset (meters) of this tangent plane relative to the world anchor.
        let diff_anchor = Vec3::new(
            (tangent_ecef[0] - self.origin_ecef_m[0]) as f32,
            (tangent_ecef[1] - self.origin_ecef_m[1]) as f32,
            (tangent_ecef[2] - self.origin_ecef_m[2]) as f32,
        );
        let diff_en = r_ecef_to_enu * diff_anchor; // meters in ENU
        let enu_offset_m = [diff_en.x, diff_en.y];

        // Uniforms
        let uniforms = GridUniforms {
            model_view_proj: view_proj * model,
            camera_height_m: camera.h_m as f32,
            _pad0:          [0.0; 3],
            enu_offset_m,
            _pad1:          [0.0; 2],
            plane_extent_m: self.plane_extent_m,
            _pad2:          [0.0; 3],
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Issue draw call
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.quad_vb.slice(..));
        rpass.draw(0..6, 0..1);
    }
}

pub const GRID_WGSL: &str = r#"
struct GridUniforms {
    model_view_proj: mat4x4<f32>,
    camera_height_m: f32,
    _pad0: vec3<f32>,
    enu_offset_m: vec2<f32>,
    plane_extent_m: f32,
    _pad1: f32,
};
@group(0) @binding(0) var<uniform> U: GridUniforms;

struct VSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec2<f32>, // EN coordinates in meters (stable)
}

@vertex
fn vs_main(@location(0) corner: vec2<f32>) -> VSOut {
    var out: VSOut;
    out.clip = U.model_view_proj * vec4<f32>(corner, 0.0, 1.0);
    // Convert NDC quad corners to EN meters and add the world-anchor offset.
    out.world_pos = corner * U.plane_extent_m + U.enu_offset_m;
    return out;
}

struct FSOut {
    @location(0) color: vec4<f32>,
    @location(1) dlin:  vec4<f32>,
}

// Anti‑aliased line mask
fn line(coord: f32, step: f32) -> f32 {
    let t   = coord / step;
    // Cap AA width so very small steps can't smear into a solid fill.
    let aaw = min(fwidth(t) * 1.5, 0.5);
    let f   = fract(t);
    let d   = min(f, 1.0 - f);
    return 1.0 - smoothstep(0.0, aaw, d);
}

@fragment
fn fs_main(in: VSOut) -> FSOut {
    if (U.camera_height_m < 50.0 || U.camera_height_m > 200000.0) {
        discard;
    }

    let log_h = log2(U.camera_height_m);

    // Grid spacing levels
    let level0_log = floor(log_h / log2(10.0));
    // Do not allow sub‑meter cells; this keeps line density bounded.
    let min_step    = 1.0;
    let level0_step = max(pow(10.0, level0_log), min_step);
    let level1_step = max(level0_step * 10.0, min_step * 10.0);

    // Blend factor between levels
    let t = fract(log_h / log2(10.0));
    let p = in.world_pos;

    let minor0 = max(line(p.x, level0_step), line(p.y, level0_step));
    let major0 = max(line(p.x, level0_step * 10.0), line(p.y, level0_step * 10.0));
    let grid0  = minor0 * 0.5 + major0 * 0.5;

    let minor1 = max(line(p.x, level1_step), line(p.y, level1_step));
    let major1 = max(line(p.x, level1_step * 10.0), line(p.y, level1_step * 10.0));
    let grid1  = minor1 * 0.5 + major1 * 0.5;

    let grid = mix(grid0, grid1, t);

    // Fade at extreme heights
    let opacity = grid * (1.0 - smoothstep(20000.0, 200000.0, U.camera_height_m));
    let color   = vec3<f32>(0.176, 0.969, 1.000); // HUD cyan

    var out: FSOut;
    out.color = vec4<f32>(color, opacity * 0.10); // was 0.18
    // Overlay tag (alpha=0) and background depth (r=1)
    out.dlin = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    return out;
}
"#;
