use roaring::RoaringBitmap;
use nalgebra::{Vector3, Point3};
use api::gen::api::v1::{Vec3m, AgentState};

pub struct PerceptionSystem {
    // Simulation parameters
    scan_range_m: f64,
    scan_resolution: u32,
    noise_level: f64,
}

impl PerceptionSystem {
    pub fn new(scan_range_m: f64, scan_resolution: u32, noise_level: f64) -> Self {
        Self {
            scan_range_m,
            scan_resolution,
            noise_level,
        }
    }

    /// Simulate discovering points based on agent position and orientation
    /// Returns a portable Roaring bitmap of discovered point IDs
    pub fn discover_points(&self, agent_state: &AgentState) -> Result<Vec<u8>, String> {
        let position = agent_state.position_ecef_m.as_ref()
            .ok_or("Missing agent position")?;
        
        let mut discovered = RoaringBitmap::new();

        // Simple grid-based discovery simulation
        // In a real system, this would use actual sensor models and environment data
        let center = Point3::new(position.x, position.y, position.z);
        let grid_size = self.scan_range_m / (self.scan_resolution as f64);

        for i in 0..self.scan_resolution {
            for j in 0..self.scan_resolution {
                for k in 0..self.scan_resolution {
                    let offset = Vector3::new(
                        (i as f64 - self.scan_resolution as f64 / 2.0) * grid_size,
                        (j as f64 - self.scan_resolution as f64 / 2.0) * grid_size,
                        (k as f64 - self.scan_resolution as f64 / 2.0) * grid_size,
                    );
                    
                    let point = center + offset;
                    let distance = offset.magnitude();
                    
                    // Only consider points within scan range
                    if distance <= self.scan_range_m {
                        // Simple noise model - probability of detection decreases with distance
                        let detection_probability = (self.scan_range_m - distance) / self.scan_range_m;
                        let noise_factor = 1.0 - self.noise_level * rand::random::<f64>();
                        
                        if detection_probability * noise_factor > 0.5 {
                            // Hash point coordinates to get a consistent point ID
                            let point_id = self.hash_point(&point);
                            discovered.insert(point_id);
                        }
                    }
                }
            }
        }

        // Serialize to portable format
        let mut buffer = Vec::new();
        discovered.serialize_into(&mut buffer)
            .map_err(|e| format!("Failed to serialize discovered points: {}", e))?;
        
        Ok(buffer)
    }

    /// Simple hash function to convert 3D coordinates to consistent point IDs
    /// In a real system, this would be based on actual point cloud indexing
    fn hash_point(&self, point: &Point3<f64>) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        // Discretize coordinates to avoid floating-point precision issues
        let discretized = (
            (point.x * 1000.0) as i64,
            (point.y * 1000.0) as i64,
            (point.z * 1000.0) as i64,
        );
        discretized.hash(&mut hasher);
        (hasher.finish() % 1_000_000) as u32  // Limit to reasonable point ID range
    }
}

// Add rand dependency for the noise simulation
use rand;

#[cfg(test)]
mod tests {
    use super::*;
    use api::gen::api::v1::{Vec3mps, UnitQuaternion, AgentMode};

    #[test]
    fn test_perception_system() {
        let perception = PerceptionSystem::new(10.0, 5, 0.1);
        
        let agent_state = AgentState {
            agent_id: 1,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            position_ecef_m: Some(Vec3m { x: 0.0, y: 0.0, z: 0.0 }),
            velocity_ecef_mps: Some(Vec3mps { x: 0.0, y: 0.0, z: 0.0 }),
            orientation_ecef: Some(UnitQuaternion { w: 1.0, x: 0.0, y: 0.0, z: 0.0 }),
            mode: AgentMode::Scanning as i32,
            sequence: 1,
            schema_version: 1,
        };

        let discovered = perception.discover_points(&agent_state).unwrap();
        assert!(!discovered.is_empty());
    }
}
