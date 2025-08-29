// symtex/crates/sim_orchestrator/src/state.rs
use api::gen::api::v1 as pb;
use dashmap::DashMap;
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::watch;

/// The single, authoritative source of truth for the simulation.
///
/// This struct is wrapped in an `Arc` and shared across all concurrent tasks
/// within the orchestrator. It uses thread-safe interior mutability patterns.
pub struct CanonicalState {
    /// A map of registered and active agents, keyed by their unique `agent_id`.
    pub agents: DashMap<u64, AgentRuntimeInfo>,
    /// A temporary holding map for agents that have been spawned but have not yet
    /// completed their gRPC registration. Keyed by a unique session ID (UUID string).
    pub pending_registrations: DashMap<String, tokio::process::Child>,
    /// The global, unified map of all discovered points, represented as a compressed bitmap.
    pub reveal_mask: RwLock<RoaringBitmap>,
    /// Static metadata about the point cloud, such as the total number of points.
    pub point_cloud_metadata: PointCloudMetadata,
    /// The sender side of a watch channel used to broadcast `WorldStateSnapshot` updates
    /// to all subscribed viewers.
    pub world_state_tx: watch::Sender<WorldStateSnapshot>,
    /// An atomic counter to generate unique, monotonic IDs for new agents.
    next_agent_id: std::sync::atomic::AtomicU64,
    /// A map of currently valid Arrow Flight tickets to their corresponding reveal mask snapshots.
    /// This prevents clients from using old tickets to access new data.
    pub valid_flight_tickets: RwLock<HashMap<Vec<u8>, Arc<RoaringBitmap>>>,
}

/// Holds all runtime information for a single agent, including its OS process handle.
pub struct AgentRuntimeInfo {
    /// The last time the orchestrator received a report from this agent. Used for health checks.
    pub last_seen: Instant,
    /// The most recent state reported by the agent.
    pub current_state: pb::AgentState,
    /// A handle to the agent's OS child process, allowing the orchestrator to manage its lifecycle.
    pub process_handle: Option<tokio::process::Child>,
}

/// An immutable, cloneable snapshot of the world state at a specific moment in time.
/// This is the data structure that is broadcast to viewers.
#[derive(Clone)]
pub struct WorldStateSnapshot {
    pub timestamp_ms: i64,
    pub agents: Vec<pb::AgentState>,
    pub reveal_mask_flight_ticket: Vec<u8>,
}

/// Static metadata about the point cloud.
pub struct PointCloudMetadata {
    pub total_points: u64,
}

impl CanonicalState {
    /// Creates a new, empty `CanonicalState` and the receiver for its broadcast channel.
    pub fn new(total_points: u64) -> (Arc<Self>, watch::Receiver<WorldStateSnapshot>) {
        let (tx, rx) = watch::channel(WorldStateSnapshot {
            timestamp_ms: 0,
            agents: Vec::new(),
            reveal_mask_flight_ticket: Vec::new(),
        });
        let this = Arc::new(Self {
            agents: DashMap::new(),
            pending_registrations: DashMap::new(),
            reveal_mask: RwLock::new(RoaringBitmap::new()),
            point_cloud_metadata: PointCloudMetadata { total_points },
            world_state_tx: tx,
            next_agent_id: std::sync::atomic::AtomicU64::new(1),
            valid_flight_tickets: RwLock::new(HashMap::new()),
        });
        (this, rx)
    }

    /// Atomically generates and returns a new, unique agent ID.
    pub fn next_agent_id(&self) -> u64 {
        self.next_agent_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Safely updates the state of a known agent based on a new report.
    ///
    /// This performs an in-place update to avoid overwriting the `process_handle`.
    pub fn update_agent_state(&self, agent_id: u64, state: pb::AgentState) {
        if let Some(mut agent_info) = self.agents.get_mut(&agent_id) {
            agent_info.last_seen = Instant::now();
            agent_info.current_state = state;
        } else {
            tracing::warn!(
                agent_id,
                "Received state update for an unknown or unregistered agent."
            );
        }
    }

    /// Merges a bitmap of discovered points from an agent into the global reveal mask.
    ///
    /// Returns the number of newly discovered points.
    pub fn merge_discovered_points(&self, discovered: &[u8]) -> Result<u64, String> {
        if discovered.is_empty() {
            return Ok(0);
        }

        let snapshot = RoaringBitmap::deserialize_from(&mut discovered.as_ref())
            .map_err(|e| format!("Failed to deserialize roaring bitmap: {}", e))?;

        let mut global = self.reveal_mask.write();
        let before = global.len();
        *global |= snapshot;
        let after = global.len();

        Ok(after - before)
    }

    /// Creates a new, unique ticket for Arrow Flight and associates it with a
    /// snapshot of the current reveal mask.
    pub fn create_flight_ticket(&self) -> Vec<u8> {
        let ticket = uuid::Uuid::new_v4().as_bytes().to_vec();
        let reveal_mask_snapshot = self.reveal_mask.read().clone();
        self.valid_flight_tickets
            .write()
            .insert(ticket.clone(), Arc::new(reveal_mask_snapshot));
        // TODO: Add logic to prune old tickets from the map.
        ticket
    }

    /// Gathers the current state, creates a snapshot, and broadcasts it to all subscribers.
    pub fn broadcast_world_state(&self) {
        let agents: Vec<pb::AgentState> = self
            .agents
            .iter()
            .map(|entry| entry.current_state.clone())
            .collect();

        let ticket = self.create_flight_ticket();

        let snapshot = WorldStateSnapshot {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            agents,
            reveal_mask_flight_ticket: ticket,
        };

        // Sending on a watch channel never fails.
        let _ = self.world_state_tx.send(snapshot);
    }

    /// Calculates the current map coverage ratio.
    pub fn get_coverage_ratio(&self) -> f64 {
        let revealed = self.reveal_mask.read().len();
        if self.point_cloud_metadata.total_points == 0 {
            0.0
        } else {
            revealed as f64 / self.point_cloud_metadata.total_points as f64
        }
    }
}
