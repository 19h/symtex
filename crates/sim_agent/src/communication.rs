use std::time::Duration;
use tokio::{sync::mpsc, task::JoinSet};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};
use api::gen::api::v1::{
    simulation_c2_client::SimulationC2Client, AgentReport, ReportStateResponse, RegisterAgentRequest,
};

pub struct Comm { 
    client: SimulationC2Client<Channel> 
}

impl Comm {
    pub async fn connect(grpc_addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let endpoint = Endpoint::from_shared(grpc_addr.to_owned())?
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5));
        let channel = endpoint.connect().await?;
        Ok(Self { client: SimulationC2Client::new(channel) })
    }

    pub async fn register(&mut self, session_id: &str) -> Result<u64, Status> {
        let resp = self.client.register_agent(Request::new(RegisterAgentRequest {
            session_id: session_id.into(), 
            sw_version: "dev".into(), 
            hw_profile: "sim".into()
        })).await?.into_inner();
        Ok(resp.agent_id)
    }

    pub async fn run_report_state<F>(
        mut self,
        mut tx_reports: mpsc::Receiver<AgentReport>,
        mut handle_response: F,
    ) -> Result<(), Status> 
    where
        F: FnMut(ReportStateResponse) + Send + 'static,
    {
        let outbound = ReceiverStream::new(tx_reports);
        let response = self.client.report_state(outbound).await?;
        let mut inbound = response.into_inner();

        // Reader runs concurrently with writer (the sender task producing AgentReport)
        let mut set = JoinSet::new();
        set.spawn(async move {
            while let Some(msg) = inbound.message().await.transpose()? {
                handle_response(msg);
            }
            Ok::<_, Status>(())
        });

        // Await reader completion; the writer side is owned by the caller via tx_reports.
        while let Some(res) = set.join_next().await {
            res??;
        }
        Ok(())
    }
}
