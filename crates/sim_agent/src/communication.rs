use api::gen::api::v1::{
    simulation_c2_client::SimulationC2Client, AgentReport, RegisterAgentRequest, Task,
};
use crate::metrics::AgentMetrics;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

#[derive(Error, Debug)]
pub enum Error {
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC status error: {0}")]
    Status(#[from] Status),
    #[error("Failed to send task to main loop; receiver dropped.")]
    TaskSendFailed,
}

/// Manages the gRPC connection and communication protocol with the orchestrator.
pub struct Comm {
    client: SimulationC2Client<Channel>,
}

impl Comm {
    /// Establishes the initial gRPC connection to the orchestrator.
    pub async fn connect(grpc_addr: &str) -> Result<Self, Error> {
        let endpoint = Endpoint::from_shared(grpc_addr.to_owned())?
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5));

        let channel = endpoint.connect().await?;
        Ok(Self {
            client: SimulationC2Client::new(channel),
        })
    }

    /// Performs the unary `RegisterAgent` RPC call.
    pub async fn register(&mut self, session_id: &str) -> Result<u64, Status> {
        let resp = self
            .client
            .register_agent(Request::new(RegisterAgentRequest {
                session_id: session_id.into(),
                sw_version: "dev".into(),
                hw_profile: "sim".into(),
            }))
            .await?
            .into_inner();
        Ok(resp.agent_id)
    }

    /// Runs the long-lived bidirectional report stream.
    /// This function will run until the stream is closed or an error occurs,
    /// at which point it will terminate and return.
    pub async fn run_report_stream(
        mut self,
        metrics: Arc<AgentMetrics>,
        rx_reports: mpsc::Receiver<AgentReport>,
        tx_tasks: mpsc::Sender<Task>,
    ) -> Result<(), Error> {
        metrics.set_connection_status(false);
        tracing::info!("Connecting report stream...");

        let outbound_stream = ReceiverStream::new(rx_reports);
        let response = self.client.report_state(outbound_stream).await?;
        let mut inbound = response.into_inner();

        tracing::info!("Report stream connected successfully.");
        metrics.set_connection_status(true);

        // Process incoming messages from the orchestrator.
        while let Some(msg) = inbound.message().await? {
            if let Some(task) = msg.assigned_task {
                if tx_tasks.send(task).await.is_err() {
                    tracing::warn!("Main loop task receiver dropped. Shutting down comms task.");
                    break;
                }
            }
        }

        tracing::info!("Report stream closed by server or local shutdown.");
        metrics.set_connection_status(false);
        Ok(())
    }
}
