mod state; mod grpc; mod flight; mod metrics; mod tasking;

use std::{net::SocketAddr, sync::Arc};
use tokio::signal;
use tracing_subscriber::{fmt, EnvFilter};
use metrics::Metrics;
use state::CanonicalState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env())
        .json().init();

    let grpc_addr: SocketAddr = std::env::var("ORCHESTRATOR_GRPC_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into()).parse()?;
    let flight_addr: SocketAddr = std::env::var("ORCHESTRATOR_FLIGHT_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50052".into()).parse()?;
    let metrics_addr: SocketAddr = std::env::var("ORCHESTRATOR_METRICS_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9091".into()).parse()?;

    let metrics = Arc::new(Metrics::new());
    let (state, _rx) = CanonicalState::new(1_000_000); // replace with .hypc header total

    let grpc = {
        let s = state.clone(); let m = metrics.clone();
        tokio::spawn(async move { grpc::serve_grpc(s, m, grpc_addr).await.unwrap() })
    };

    let flight = {
        use tonic::transport::Server;
        let svc = flight::make_server(state.clone(), metrics.clone());
        tokio::spawn(async move {
            Server::builder()
                .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
                .http2_keepalive_timeout(Some(std::time::Duration::from_secs(20)))
                .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
                .add_service(svc)
                .serve(flight_addr).await.unwrap()
        })
    };

    let metrics_srv = {
        use axum::Router;
        let router = metrics.router();
        tokio::spawn(async move { axum::Server::bind(&metrics_addr).serve(router.into_make_service()).await.unwrap() })
    };

    tokio::select! {
        _ = grpc => {}
        _ = flight => {}
        _ = metrics_srv => {}
        _ = signal::ctrl_c() => {}
    }

    Ok(())
}

