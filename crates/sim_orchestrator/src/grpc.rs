use std::{pin::Pin, sync::Arc, time::Duration};
use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use futures::Stream;
use api::gen::api::v1::{
    simulation_c2_server::{SimulationC2, SimulationC2Server},
    *,
};
use crate::state::{CanonicalState, WorldStateSnapshot};
use crate::metrics::Metrics;

pub struct C2Svc {
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
}

#[tonic::async_trait]
impl SimulationC2 for C2Svc {
    async fn register_agent(
        &self,
        req: Request<RegisterAgentRequest>,
    ) -> Result<Response<RegisterAgentResponse>, Status> {
        let _md = req.metadata(); // propagate trace context per spec
        self.metrics.grpc_requests_total.inc();
        
        let now = chrono::Utc::now().timestamp_millis();
        let agent_id = self.state.next_agent_id();
        
        self.metrics.agents_registered_total.inc();
        self.metrics.update_active_agents(self.state.agents.len() as i64 + 1);
        
        let resp = RegisterAgentResponse {
            agent_id,
            server_time_ms: now,
            report_interval_ms: 500, // example; derive from config
            max_report_bytes: 1024 * 1024,
            schema_version: 1,
        };
        
        tracing::info!(agent_id, "Agent registered successfully");
        Ok(Response::new(resp))
    }

    type ReportStateStream = Pin<Box<dyn Stream<Item = Result<ReportStateResponse, Status>> + Send + 'static>>;
    
    async fn report_state(
        &self,
        req: Request<tonic::Streaming<AgentReport>>,
    ) -> Result<Response<Self::ReportStateStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<ReportStateResponse, Status>>(16);
        let mut stream = req.into_inner();

        let state = self.state.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            while let Some(msg) = stream.message().await.transpose() {
                match msg {
                    Ok(report) => {
                        // Update agent state
                        if let Some(agent_state) = report.state {
                            state.update_agent_state(report.agent_id, agent_state);
                        }

                        // Process discovered points
                        if !report.discovered_point_ids_portable.is_empty() {
                            match state.merge_discovered_points(&report.discovered_point_ids_portable) {
                                Ok(new_points) => {
                                    if new_points > 0 {
                                        metrics.points_revealed_total.inc_by(new_points);
                                        metrics.update_coverage(state.get_coverage_ratio());
                                        
                                        // Broadcast updated world state
                                        state.broadcast_world_state();
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, agent_id = report.agent_id, "Failed to process discovered points");
                                }
                            }
                        }

                        // For now, no task assignment logic - just acknowledge
                        let resp = ReportStateResponse { 
                            assigned_task: None, 
                            schema_version: 1 
                        };
                        
                        if tx.send(Ok(resp)).await.is_err() { 
                            break; 
                        }
                    }
                    Err(e) => { 
                        let _ = tx.send(Err(e)).await; 
                        break; 
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx)) as Self::ReportStateStream))
    }

    type SubscribeWorldStateStream = Pin<Box<dyn Stream<Item = Result<WorldState, Status>> + Send + 'static>>;
    
    async fn subscribe_world_state(
        &self,
        _req: Request<SubscribeWorldStateRequest>,
    ) -> Result<Response<Self::SubscribeWorldStateStream>, Status> {
        self.metrics.grpc_requests_total.inc();
        
        // Convert watch::Receiver<WorldStateSnapshot> to a Stream<WorldState>
        let rx = self.state.world_state_tx.subscribe();
        let coverage_ratio = self.state.get_coverage_ratio();
        
        let stream = tokio_stream::wrappers::WatchStream::new(rx).map(move |snap: WorldStateSnapshot| {
            Ok(WorldState {
                timestamp_ms: snap.timestamp_ms,
                agents: snap.agents,
                reveal_mask_ticket: snap.reveal_mask_flight_ticket,
                map_coverage_ratio: coverage_ratio,
                schema_version: 1,
            })
        });
        
        Ok(Response::new(Box::pin(stream) as Self::SubscribeWorldStateStream))
    }

    async fn issue_command(
        &self,
        _req: Request<IssueCommandRequest>,
    ) -> Result<Response<IssueCommandResponse>, Status> {
        self.metrics.grpc_requests_total.inc();
        
        // TODO: Implement command processing logic
        Ok(Response::new(IssueCommandResponse {
            acknowledged: true, 
            message: "Command acknowledged".into(), 
            schema_version: 1
        }))
    }
}

pub async fn serve_grpc(
    state: Arc<CanonicalState>, 
    metrics: Arc<Metrics>, 
    addr: std::net::SocketAddr
) -> anyhow::Result<()> {
    use tonic::transport::Server;
    
    let svc = C2Svc { state, metrics };
    
    tracing::info!(address = %addr, "Starting gRPC server");
    
    Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(20)))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .add_service(SimulationC2Server::new(svc))
        .serve(addr)
        .await?;
    
    Ok(())
}
