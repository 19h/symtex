use std::{collections::HashMap, sync::Arc, time::Instant};
use parking_lot::RwLock;
use dashmap::DashMap;
use roaring::RoaringBitmap;
use tokio::sync::watch;
use api::gen::api::v1 as pb;

pub struct CanonicalState {
    pub agents: DashMap<u64, AgentRuntimeInfo>,
    pub reveal_mask: RwLock<RoaringBitmap>,
    pub point_cloud_metadata: PointCloudMetadata,
    pub world_state_tx: watch::Sender<WorldStateSnapshot>,
    next_agent_id: std::sync::atomic::AtomicU64,
    pub valid_flight_tickets: RwLock<HashMap<Vec<u8>, Arc<RoaringBitmap>>>,
}

pub struct AgentRuntimeInfo {
    pub last_seen: Instant,
    pub current_state: pb::AgentState,
    pub process_handle: Option<tokio::process::Child>,
}

#[derive(Clone)]
pub struct WorldStateSnapshot {
    pub timestamp_ms: i64,
    pub agents: Vec<pb::AgentState>,
    pub reveal_mask_flight_ticket: Vec<u8>,
}

pub struct PointCloudMetadata { 
    pub total_points: u64 
}

impl CanonicalState {
    pub fn new(total_points: u64) -> (Arc<Self>, watch::Receiver<WorldStateSnapshot>) {
        let (tx, rx) = watch::channel(WorldStateSnapshot {
            timestamp_ms: 0, 
            agents: Vec::new(), 
            reveal_mask_flight_ticket: Vec::new()
        });
        let this = Arc::new(Self {
            agents: DashMap::new(),
            reveal_mask: RwLock::new(RoaringBitmap::new()),
            point_cloud_metadata: PointCloudMetadata { total_points },
            world_state_tx: tx,
            next_agent_id: std::sync::atomic::AtomicU64::new(1),
            valid_flight_tickets: RwLock::new(HashMap::new()),
        });
        (this, rx)
    }

    pub fn next_agent_id(&self) -> u64 {
        self.next_agent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn update_agent_state(&self, agent_id: u64, state: pb::AgentState) {
        let runtime_info = AgentRuntimeInfo {
            last_seen: Instant::now(),
            current_state: state,
            process_handle: None,
        };
        self.agents.insert(agent_id, runtime_info);
    }

    pub fn merge_discovered_points(&self, discovered: &[u8]) -> Result<u64, String> {
        if discovered.is_empty() {
            return Ok(0);
        }

        let mut snapshot = RoaringBitmap::new();
        snapshot.deserialize_from(&mut discovered.as_ref())
            .map_err(|e| format!("Failed to deserialize roaring bitmap: {}", e))?;

        let mut global = self.reveal_mask.write();
        let before = global.len();
        *global |= snapshot;
        let after = global.len();
        
        Ok(after - before)
    }

    pub fn create_flight_ticket(&self) -> Vec<u8> {
        let ticket = uuid::Uuid::new_v4().as_bytes().to_vec();
        let reveal_mask = self.reveal_mask.read().clone();
        self.valid_flight_tickets.write().insert(ticket.clone(), Arc::new(reveal_mask));
        ticket
    }

    pub fn broadcast_world_state(&self) {
        let agents: Vec<pb::AgentState> = self.agents
            .iter()
            .map(|entry| entry.current_state.clone())
            .collect();

        let ticket = self.create_flight_ticket();
        
        let snapshot = WorldStateSnapshot {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            agents,
            reveal_mask_flight_ticket: ticket,
        };

        let _ = self.world_state_tx.send(snapshot);
    }

    pub fn get_coverage_ratio(&self) -> f64 {
        let revealed = self.reveal_mask.read().len();
        if self.point_cloud_metadata.total_points == 0 {
            0.0
        } else {
            revealed as f64 / self.point_cloud_metadata.total_points as f64
        }
    }
}
