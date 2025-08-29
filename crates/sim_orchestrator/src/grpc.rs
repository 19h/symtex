use crate::{
    metrics::Metrics,
    state::{AgentRuntimeInfo, CanonicalState, WorldStateSnapshot},
};
use api::gen::api::v1::{
    simulation_c2_server::{SimulationC2, SimulationC2Server},
    *,
};
use futures::Stream;
use std::{pin::Pin, sync::Arc, time::Duration, time::Instant};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status};

/// The implementation of the `SimulationC2` gRPC service.
pub struct C2Svc {
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
}

#[tonic::async_trait]
impl SimulationC2 for C2Svc {
    /// RPC for a `sim_agent` to register with the orchestrator.
    /// This is the second phase of a two-phase registration process. The agent process
    /// is first spawned by the `AgentManager`, which places its handle in a pending map.
    /// This RPC call finalizes the registration by moving the handle to the active agents map.
    async fn register_agent(
        &self,
        req: Request<RegisterAgentRequest>,
    ) -> Result<Response<RegisterAgentResponse>, Status> {
        let req_inner = req.into_inner();
        let session_id = req_inner.session_id;

        // Phase 2: Finalize registration.
        // Atomically remove the pending registration to prevent race conditions.
        let child_handle = match self.state.pending_registrations.remove(&session_id) {
            Some(entry) => entry.1, // entry is a (key, value) tuple
            None => {
                tracing::warn!(
                    session_id,
                    "Agent registration failed: session ID not found in pending list."
                );
                return Err(Status::not_found("Session ID not found or expired."));
            }
        };

        let agent_id = self.state.next_agent_id();
        tracing::info!(agent_id, session_id, "Registering agent");

        let runtime_info = AgentRuntimeInfo {
            last_seen: Instant::now(),
            current_state: AgentState {
                agent_id,
                mode: AgentMode::AwaitingTask as i32,
                ..Default::default()
            },
            process_handle: Some(child_handle),
        };

        self.state.agents.insert(agent_id, runtime_info);
        self.metrics.agents_registered_total.inc();
        self.metrics
            .update_active_agents(self.state.agents.len() as i64);

        let resp = RegisterAgentResponse {
            agent_id,
            server_time_ms: chrono::Utc::now().timestamp_millis(),
            report_interval_ms: 500,
            max_report_bytes: 1024 * 1024,
            schema_version: 1,
        };

        Ok(Response::new(resp))
    }

    type ReportStateStream =
        Pin<Box<dyn Stream<Item = Result<ReportStateResponse, Status>> + Send + 'static>>;

    /// Long-lived bidirectional stream for an agent to report its state and discoveries,
    /// and for the orchestrator to send back tasks.
    async fn report_state(
        &self,
        req: Request<tonic::Streaming<AgentReport>>,
    ) -> Result<Response<Self::ReportStateStream>, Status> {
        let (tx, rx) = mpsc::channel(16);
        let mut stream = req.into_inner();

        let state = self.state.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut agent_id_opt: Option<u64> = None;

            loop {
                tokio::select! {
                    msg = stream.message() => {
                        match msg {
                            Ok(Some(report)) => {
                                let agent_id = report.agent_id;
                                if agent_id_opt.is_none() {
                                    agent_id_opt = Some(agent_id);
                                    tracing::info!(agent_id, "Established report stream.");
                                }

                                // Update agent state
                                if let Some(agent_state) = report.state {
                                    state.update_agent_state(agent_id, agent_state);
                                }

                                // Process discovered points
                                if !report.discovered_point_ids_portable.is_empty() {
                                    match state.merge_discovered_points(&report.discovered_point_ids_portable) {
                                        Ok(new_points) => {
                                            if new_points > 0 {
                                                metrics.points_revealed_total.inc_by(new_points);
                                                metrics.update_coverage(state.get_coverage_ratio());
                                                state.broadcast_world_state();
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, agent_id, "Failed to process discovered points");
                                        }
                                    }
                                }

                                // TODO: Implement task allocation logic
                                let resp = ReportStateResponse {
                                    assigned_task: None,
                                    schema_version: 1,
                                };

                                if tx.send(Ok(resp)).await.is_err() {
                                    tracing::warn!(agent_id, "Response channel closed by client.");
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Client closed the stream gracefully.
                                break;
                            }
                            Err(e) => {
                                if let Some(agent_id) = agent_id_opt {
                                    tracing::warn!(agent_id, error = %e, "Agent report stream error.");
                                }
                                let _ = tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                }
            }

            if let Some(agent_id) = agent_id_opt {
                tracing::info!(agent_id, "Agent report stream closed.");
                // The AgentManager's health check will handle cleanup of the stale agent.
            }
        });

        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::ReportStateStream
        ))
    }

    type SubscribeWorldStateStream =
        Pin<Box<dyn Stream<Item = Result<WorldState, Status>> + Send + 'static>>;

    /// Long-lived server-streaming RPC for a viewer to receive updates on the world state.
    async fn subscribe_world_state(
        &self,
        _req: Request<SubscribeWorldStateRequest>,
    ) -> Result<Response<Self::SubscribeWorldStateStream>, Status> {
        self.metrics.grpc_requests_total.inc();
        tracing::info!("New world state subscriber connected.");

        let rx = self.state.world_state_tx.subscribe();
        let state_clone = self.state.clone();

        let stream = tokio_stream::wrappers::WatchStream::new(rx).map(
            move |snap: WorldStateSnapshot| {
                Ok(WorldState {
                    timestamp_ms: snap.timestamp_ms,
                    agents: snap.agents,
                    reveal_mask_ticket: snap.reveal_mask_flight_ticket,
                    map_coverage_ratio: state_clone.get_coverage_ratio(),
                    schema_version: 1,
                })
            },
        );

        Ok(Response::new(
            Box::pin(stream) as Self::SubscribeWorldStateStream
        ))
    }

    /// Unary RPC for a viewer to send commands to the simulation.
    async fn issue_command(
        &self,
        req: Request<IssueCommandRequest>,
    ) -> Result<Response<IssueCommandResponse>, Status> {
        self.metrics.grpc_requests_total.inc();
        let cmd = req
            .into_inner()
            .command
            .ok_or_else(|| Status::invalid_argument("Command is missing"))?;

        match cmd {
            issue_command_request::Command::StartSurvey(_) => {
                tracing::info!("Received StartSurvey command.");
                // TODO: Trigger tasking module
            }
            issue_command_request::Command::ResetSimulation(_) => {
                tracing::info!("Received ResetSimulation command.");
                // TODO: Implement simulation reset logic
            }
        }

        Ok(Response::new(IssueCommandResponse {
            acknowledged: true,
            message: "Command acknowledged".into(),
            schema_version: 1,
        }))
    }
}

/// Configures and runs the main gRPC server.
pub async fn serve_grpc(
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let svc = C2Svc { state, metrics };

    tracing::info!(address = %addr, "Starting gRPC server");

    tonic::transport::Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(20)))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .add_service(SimulationC2Server::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
