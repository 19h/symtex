use crate::{
    camera::{Camera, CameraController},
    data::{point_cloud::load_hypc_tile, types::TileGpu},
    renderer::Renderer,
    ui,
};
use anyhow::Result;
use glam::Mat4;
use std::sync::Arc;
use walkdir::WalkDir;
use winit::{event::WindowEvent, window::Window};

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

pub struct App {
    pub renderer: Renderer,
    pub camera: Camera,
    pub camera_controller: CameraController,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub tiles: Vec<TileGpu>,
}

impl App {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let renderer = Renderer::new(window.clone()).await?;
        let size = renderer.gfx.size;

        // WebGPU/wgpu uses 0..1 depth; glam::Mat4::perspective_rh is RH, depth in [0,1].
        let proj = Mat4::perspective_rh(
            60f32.to_radians(),
            size.width as f32 / size.height.max(1) as f32,
            10.0,
            20_000_000.0,
        );

        // Default camera, orbiting a point over Berlin at a 5km radius.
        let camera = Camera::new(52.52, 13.40, 5000.0, proj);
        let camera_controller = CameraController::new();

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &*window,
            None,
            None,
        );

        Ok(Self {
            renderer,
            camera,
            camera_controller,
            egui_ctx,
            egui_state,
            tiles: Vec::new(),
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.renderer.resize(new_size);
            self.camera.proj = Mat4::perspective_rh(
                // Field of view
                60f32.to_radians(),
                // Aspect ratio
                new_size.width as f32 / new_size.height as f32,
                // Near plane distance
                10.0,
                20_000_000.0,
            );
        }
    }

    pub fn handle_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.egui_state.on_window_event(window, event);
        if response.consumed {
            return true;
        }

        self.camera_controller.handle_event(event, &mut self.camera);

        if let WindowEvent::Resized(physical_size) = event {
            self.resize(*physical_size);
        }

        false
    }

    pub fn build_all_tiles(&mut self, root: &str) -> Result<()> {
        let paths: Vec<_> = WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("hypc"))
            .map(|e| e.path().to_path_buf())
            .collect();

        if paths.is_empty() {
            log::warn!("No .hypc files found in '{}'", root);
            return Ok(());
        }

        log::info!("Loading {} tiles...", paths.len());

        let mut loaded_tiles = Vec::new();
        let mut total_points: u64 = 0;
        let mut sum_anchor_w = [0.0f64; 3]; // Σ (anchor_m * weight)
        let mut sum_w = 0.0f64; // Σ weight
        let mut anchors_m: Vec<[f64; 3]> = Vec::new();
        let mut min_upm: u32 = u32::MAX;
        let mut max_upm: u32 = 0;

        let viewport_size = [
            self.renderer.gfx.size.width as f32,
            self.renderer.gfx.size.height as f32,
        ];

        for path in paths {
            match load_hypc_tile(
                &self.renderer.gfx.device,
                &self.renderer.holo.tile_layout,
                &self.camera,
                &path,
                viewport_size,
            ) {
                Ok(tile) => {
                    // Convert this tile's anchor to meters using ITS UPM.
                    let upm = tile.units_per_meter as f64;

                    let a_m = [
                        tile.anchor_units[0] as f64 / upm,
                        tile.anchor_units[1] as f64 / upm,
                        tile.anchor_units[2] as f64 / upm,
                    ];

                    anchors_m.push(a_m);

                    let w = tile.instances_len as f64; // weight by points

                    sum_anchor_w[0] += a_m[0] * w;
                    sum_anchor_w[1] += a_m[1] * w;
                    sum_anchor_w[2] += a_m[2] * w;

                    sum_w += w;

                    min_upm = min_upm.min(tile.units_per_meter);
                    max_upm = max_upm.max(tile.units_per_meter);

                    total_points += tile.instances_len as u64;

                    log::debug!(
                        "Tile {:?}: upm={}, anchor_ecef_m=({:.3},{:.3},{:.3}), points={}",
                        tile.key
                            .map(|k| String::from_utf8_lossy(&k).trim_end_matches('\0').to_string()),
                        tile.units_per_meter,
                        a_m[0],
                        a_m[1],
                        a_m[2],
                        tile.instances_len
                    );

                    loaded_tiles.push(tile);
                }
                Err(e) => {
                    log::error!("Failed to load tile {}: {}", path.display(), e);
                }
            }
        }

        if !loaded_tiles.is_empty() && sum_w > 0.0 {
            // Weighted centroid in "claimed ECEF" meters
            let center_ecef_m = [
                sum_anchor_w[0] / sum_w,
                sum_anchor_w[1] / sum_w,
                sum_anchor_w[2] / sum_w,
            ];

            // Plausibility check for WGS‑84 ECEF surface vicinity
            let r = (center_ecef_m[0] * center_ecef_m[0]
                + center_ecef_m[1] * center_ecef_m[1]
                + center_ecef_m[2] * center_ecef_m[2])
                .sqrt();

            // Accept only ~6.2–6.5 Mm (allows terrain + a few km)
            let plausible = (6_200_000.0..=6_500_000.0).contains(&r);

            if !plausible {
                log::warn!(
                    "Anchor centroid radius {:.3} Mm not plausible for WGS‑84; skipping recenter. \
                     (Check input CS and HYPC anchors.)",
                    r * 1e-6
                );
            }

            // Approximate dataset radius from anchor spread
            let mut r2_max = 0.0f64;
            for a in &anchors_m {
                let dx = a[0] - center_ecef_m[0];
                let dy = a[1] - center_ecef_m[1];
                let dz = a[2] - center_ecef_m[2];
                r2_max = r2_max.max(dx * dx + dy * dy + dz * dz);
            }
            let radius_m = r2_max.sqrt();

            // Choose a starting orbit radius to fit the dataset in view from an angle.
            let start_radius_m = (radius_m * 2.0).clamp(100.0, 50_000.0);

            if plausible {
                self.camera
                    .set_target_and_radius(center_ecef_m, start_radius_m);
            }

            // Propagate grid world anchor so the grid is stable in EN.
            self.renderer.grid.set_origin(center_ecef_m);

            log::info!(
                "Loaded {} tiles | points={} | UPM range [{}..{}].",
                loaded_tiles.len(),
                total_points,
                min_upm,
                max_upm
            );

            let (lat, lon, _) =
                hypc::ecef_to_geodetic(center_ecef_m[0], center_ecef_m[1], center_ecef_m[2]);

            log::info!(
                "Dataset center ECEF(m)=({:.3},{:.3},{:.3}) -> geodetic ({:.6}°, {:.6}°). \
                 Anchor spread radius ~{:.1} m. Start orbit radius set to {:.1} m.",
                center_ecef_m[0],
                center_ecef_m[1],
                center_ecef_m[2],
                lat,
                lon,
                radius_m,
                start_radius_m
            );
        }

        self.tiles = loaded_tiles;
        Ok(())
    }

    pub fn render(&mut self, window: &Window) -> Result<(), wgpu::SurfaceError> {
        let frame = self.renderer.gfx.surface.get_current_texture()?;
        let swap_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let viewport_size = [
            self.renderer.gfx.size.width as f32,
            self.renderer.gfx.size.height as f32,
        ];

        // Dynamically adjust point size based on altitude - larger points at lower altitudes
        const MAX_POINT_SIZE: f32 = 3.0;
        const MIN_POINT_SIZE: f32 = 0.6;
        const MAX_ALT_M: f32 = 1000.0;
        let clamped_alt = (self.camera.h_m as f32).clamp(1.0, MAX_ALT_M);

        // Normalize altitude to [0, 1] where 1.0 is max altitude
        let normalized_alt = (clamped_alt.ln() / MAX_ALT_M.ln()).clamp(0.0, 1.0);

        // Inverse relationship: point size decreases as altitude increases
        // At normalized_alt = 0 (low altitude), point_size = MAX_POINT_SIZE
        // At normalized_alt = 1 (high altitude), point_size = MIN_POINT_SIZE
        let point_size = MAX_POINT_SIZE - normalized_alt * (MAX_POINT_SIZE - MIN_POINT_SIZE);

        for tile in &self.tiles {
            let ubo_data = tile.make_uniform(&self.camera, viewport_size, point_size);

            self.renderer
                .gfx
                .queue
                .write_buffer(&tile.ubo, 0, bytemuck::bytes_of(&ubo_data));
        }

        self.renderer.render(&swap_view, &self.tiles, &self.camera);

        let total_points = self.tiles.iter().map(|t| t.instances_len).sum();
        let egui_input = self.egui_state.take_egui_input(window);
        self.egui_ctx.begin_frame(egui_input);

        ui::draw_hud(&self.egui_ctx, self.camera.h_m as i32, total_points);

        if true {
            let gamma_deg =
                meridian_convergence_rad(self.camera.lat_deg, self.camera.lon_deg).to_degrees();

            ui::draw_debug_panel(
                &self.egui_ctx,
                &mut self.renderer.post_stack.params,
                gamma_deg,
            );
        }

        let egui_output = self.egui_ctx.end_frame();
        let shapes = self
            .egui_ctx
            .tessellate(egui_output.shapes, self.egui_ctx.pixels_per_point());

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [
                self.renderer.gfx.config.width,
                self.renderer.gfx.config.height,
            ],
            pixels_per_point: self.egui_ctx.pixels_per_point(),
        };

        let mut encoder = self
            .renderer
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("UI Encoder"),
            });

        for (id, delta) in &egui_output.textures_delta.set {
            self.renderer.egui_renderer.update_texture(
                &self.renderer.gfx.device,
                &self.renderer.gfx.queue,
                *id,
                delta,
            );
        }

        self.renderer.egui_renderer.update_buffers(
            &self.renderer.gfx.device,
            &self.renderer.gfx.queue,
            &mut encoder,
            &shapes,
            &screen_descriptor,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("EGUI Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.renderer
                .egui_renderer
                .render(&mut render_pass, &shapes, &screen_descriptor);
        }

        for id in &egui_output.textures_delta.free {
            self.renderer.egui_renderer.free_texture(id);
        }

        self.renderer
            .gfx
            .queue
            .submit(std::iter::once(encoder.finish()));
        frame.present();

        Ok(())
    }
}

impl TileGpu {
    pub fn make_uniform(
        &self,
        cam: &Camera,
        viewport_size: [f32; 2],
        point_size_px: f32,
    ) -> crate::data::types::TileUniformStd140 {
        cam.make_tile_uniform(
            self.anchor_units,
            self.units_per_meter,
            viewport_size,
            point_size_px,
        )
    }
}
