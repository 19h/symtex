use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use nalgebra::Isometry3;
use roaring::RoaringBitmap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use wgpu::util::DeviceExt;

const WORKGROUP_SIZE: u32 = 256;

/// A CPU-side struct that mirrors the `AgentPose` uniform structure in the WGSL shader.
///
/// It must be aligned to 16 bytes (`vec4`), so we add padding.
/// Derives `Pod` and `Zeroable` to allow for safe, zero-cost casting to a byte slice.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct AgentPoseUniform {
    position: [f32; 3],
    _padding1: f32,
    scan_range_sq: f32,
    _padding2: [f32; 3],
}

/// Manages the headless wgpu context and resources for GPU-based perception simulation.
pub struct PerceptionSystem {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    point_cloud_buffer: wgpu::Buffer,
    result_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    pose_uniform_buffer: wgpu::Buffer,
    num_points: u64,
    scan_range_m: f32,
}

impl PerceptionSystem {
    /// Creates a new `PerceptionSystem`, initializing the wgpu device and pipeline.
    ///
    /// This function is asynchronous as GPU initialization is non-blocking.
    pub async fn new(scan_range_m: f32, point_cloud_path: &Path) -> anyhow::Result<Self> {
        let startup_instant = Instant::now();
        tracing::info!("Initializing PerceptionSystem...");

        // --- 1. Initialize WGPU Instance, Adapter, Device, and Queue ---
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: true, // Crucial for headless/server environments
                compatible_surface: None,
            })
            .await
            .context("Failed to find a suitable wgpu adapter.")?;

        tracing::info!(adapter = ?adapter.get_info(), "Selected WGPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Perception Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("Failed to get wgpu device.")?;

        // --- 2. Load Point Cloud Data ---
        let (num_points, point_cloud_data) = Self::load_point_cloud(point_cloud_path)?;
        tracing::info!(
            num_points,
            data_size_mb = point_cloud_data.len() as f64 / 1e6,
            "Loaded point cloud data"
        );

        // --- 3. Create Buffers ---
        let point_cloud_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Point Cloud Buffer"),
            contents: &point_cloud_data,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let pose_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Agent Pose Uniform Buffer"),
            size: std::mem::size_of::<AgentPoseUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // The result buffer needs to hold the atomic count (4 bytes) plus an index (u32)
        // for every single point in the worst-case scenario.
        let result_buffer_size = (4 + num_points * 4) as u64;
        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Discovered Points Result Buffer"),
            size: result_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // The staging buffer is used to copy data from the GPU back to the CPU.
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: result_buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- 4. Create Shader and Pipeline ---
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Perception Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Perception Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Perception Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: pose_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: point_cloud_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: result_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Perception Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Perception Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });

        tracing::info!(
            duration_ms = startup_instant.elapsed().as_millis(),
            "PerceptionSystem initialized successfully"
        );

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            point_cloud_buffer,
            result_buffer,
            staging_buffer,
            pose_uniform_buffer,
            num_points,
            scan_range_m,
        })
    }

    /// Runs a simulated LiDAR scan from the agent's current pose.
    pub fn run_lidar_scan(&self, pose: &Isometry3<f64>) -> anyhow::Result<RoaringBitmap> {
        // --- 1. Update Uniform Buffer ---
        let position = pose.translation.vector;
        let uniform = AgentPoseUniform {
            position: [position.x as f32, position.y as f32, position.z as f32],
            scan_range_sq: self.scan_range_m * self.scan_range_m,
            _padding1: 0.0,
            _padding2: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.pose_uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        // Reset the atomic counter in the result buffer to 0 before each run.
        self.queue.write_buffer(&self.result_buffer, 0, &[0, 0, 0, 0]);

        // --- 2. Create and Submit Command Buffer ---
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Perception Command Encoder"),
            });

        {
            let mut compute_pass =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Perception Compute Pass"),
                    timestamp_writes: None,
                });

            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &self.bind_group, &[]);

            let n = u64::try_from(self.num_points).unwrap();

            anyhow::ensure!(n <= u64::from(u32::MAX), "num_points exceeds u32::MAX for dispatch");

            let workgroups = ((n as u32) + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;

            compute_pass.dispatch_workgroups(workgroups, 1, 1);
        }

        encoder.copy_buffer_to_buffer(
            &self.result_buffer,
            0,
            &self.staging_buffer,
            0,
            self.result_buffer.size(),
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // --- 3. Await GPU and Read Results ---
        let buffer_slice = self.staging_buffer.slice(..);
        let (sender, receiver) = futures::channel::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });

        // Poll the device to make sure the submission is processed.
        // `pollster::block_on` will drive the future to completion.
        self.device.poll(wgpu::Maintain::Wait);
        pollster::block_on(receiver)??;

        let mut discovered_points = RoaringBitmap::new();
        {
            let view = buffer_slice.get_mapped_range();
            let indices: &[u32] = bytemuck::cast_slice(&view[4..]);

            let mut count = u32::from_le_bytes(view[0..4].try_into().unwrap());
            let max_indices = ((view.len() - 4) / 4) as u32;

            if count > max_indices {
                count = max_indices;
            }

            discovered_points.extend(&indices[..count as usize]);
        }
        self.staging_buffer.unmap();

        Ok(discovered_points)
    }

    /// Loads point cloud from a .hypc file.
    /// Format: u64 num_points, followed by tightly packed f32 xyz coordinates.
    /// Pads the data to vec4 alignment for the GPU.
    fn load_point_cloud(path: &Path) -> anyhow::Result<(u64, Vec<u8>)> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open point cloud file: {:?}", path))?;

        let mut count_buf = [0u8; 8];
        file.read_exact(&mut count_buf)?;
        let num_points = u64::from_le_bytes(count_buf);

        let mut xyz_data = Vec::new();
        file.read_to_end(&mut xyz_data)?;

        let expected_size = num_points as usize * 3 * 4;
        anyhow::ensure!(
            xyz_data.len() == expected_size,
            "Point cloud file size mismatch. Expected {} bytes for {} points, got {}",
            expected_size,
            num_points,
            xyz_data.len()
        );

        // Pad the vec3 data to vec4 for 16-byte alignment on the GPU.
        let points_f32: &[f32] = bytemuck::cast_slice(&xyz_data);
        let mut padded_data = Vec::<f32>::with_capacity(num_points as usize * 4);
        for p in points_f32.chunks_exact(3) {
            padded_data.extend_from_slice(&[p[0], p[1], p[2], 0.0]);
        }

        Ok((num_points, bytemuck::cast_slice(&padded_data).to_vec()))
    }
}
