use std::{thread, time::Duration};
use crossbeam_channel::Sender;
use tonic::transport::Endpoint;
use api::gen::api::v1::{
    simulation_c2_client::SimulationC2Client, SubscribeWorldStateRequest, WorldState,
};

pub fn spawn_network(addr: String, tx: Sender<WorldState>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
            
        rt.block_on(async move {
            match run_network_loop(addr, tx).await {
                Ok(()) => tracing::info!("Network thread completed normally"),
                Err(e) => tracing::error!(error = %e, "Network thread error"),
            }
        });
    })
}

async fn run_network_loop(addr: String, tx: Sender<WorldState>) -> anyhow::Result<()> {
    let endpoint = Endpoint::from_shared(addr)?
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(5));
        
    let channel = endpoint.connect().await?;
    let mut client = SimulationC2Client::new(channel);
    
    tracing::info!("Connected to orchestrator, subscribing to world state");
    
    let mut stream = client.subscribe_world_state(SubscribeWorldStateRequest {
        include_initial_snapshot: true, 
        schema_version: 1
    }).await?.into_inner();
    
    while let Some(ws) = stream.message().await.transpose()? {
        // Try to send to render thread; drop if the render thread hasn't consumed previous
        if tx.try_send(ws).is_err() {
            tracing::debug!("Dropped world state update (render thread busy)");
        }
    }
    
    Ok(())
}
