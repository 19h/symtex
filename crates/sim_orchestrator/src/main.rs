// symtex/crates/sim_orchestrator/src/main.rs
mod agent_manager;
mod flight;
mod grpc;
mod metrics;
mod state;
mod tasking;

use crate::agent_manager::{AgentManager, AgentManagerConfig};
use crate::metrics::Metrics;
use crate::state::CanonicalState;
use anyhow::Context;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::watch;
use tracing_subscriber::{fmt, EnvFilter};

/// Holds all configuration for the sim_orchestrator application.
#[derive(Debug, Clone)]
struct Config {
    grpc_listen_addr: SocketAddr,
    flight_listen_addr: SocketAddr,
    metrics_listen_addr: SocketAddr,
    orchestrator_public_grpc_addr: String,
    agent_binary_path: String,
    num_agents: u32,
    agent_health_timeout: Duration,
    agent_metrics_port_range_start: u16,
    point_cloud_total_points: u64,
}

impl Config {
    /// Parses configuration from environment variables.
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            grpc_listen_addr: std::env::var("ORCHESTRATOR_GRPC_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:50051".into())
                .parse()
                .context("Failed to parse ORCHESTRATOR_GRPC_LISTEN_ADDR")?,
            flight_listen_addr: std::env::var("ORCHESTRATOR_FLIGHT_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:50052".into())
                .parse()
                .context("Failed to parse ORCHESTRATOR_FLIGHT_LISTEN_ADDR")?,
            metrics_listen_addr: std::env::var("ORCHESTRATOR_METRICS_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:9091".into())
                .parse()
                .context("Failed to parse ORCHESTRATOR_METRICS_LISTEN_ADDR")?,
            orchestrator_public_grpc_addr: std::env::var("ORCHESTRATOR_PUBLIC_GRPC_ADDR")
                .context("ORCHESTRATOR_PUBLIC_GRPC_ADDR must be set (e.g., 'http://127.0.0.1:60051')")?,
            agent_binary_path: std::env::var("AGENT_BINARY_PATH")
                .context("AGENT_BINARY_PATH must be set")?,
            num_agents: std::env::var("NUM_AGENTS")
                .unwrap_or_else(|_| "3".into())
                .parse()
                .context("Failed to parse NUM_AGENTS")?,
            agent_health_timeout: Duration::from_millis(
                std::env::var("AGENT_HEALTH_TIMEOUT_MS")
                    .unwrap_or_else(|_| "10000".into())
                    .parse()
                    .context("Failed to parse AGENT_HEALTH_TIMEOUT_MS")?,
            ),
            agent_metrics_port_range_start: std::env::var("AGENT_METRICS_PORT_RANGE_START")
                .unwrap_or_else(|_| "9100".into())
                .parse()
                .context("Failed to parse AGENT_METRICS_PORT_RANGE_START")?,
            // TODO: Load this from .hypc header per specification.
            point_cloud_total_points: 1_000_000,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let config = Config::from_env()?;
    tracing::info!(config = ?config, "Loaded configuration");

    let (shutdown_tx, shutdown_rx) = watch::channel(());

    let metrics = Arc::new(Metrics::new());
    let (state, _world_state_rx) = CanonicalState::new(config.point_cloud_total_points);

    // Spawn the Agent Manager
    let agent_manager_config = AgentManagerConfig {
        num_agents: config.num_agents,
        agent_binary_path: config.agent_binary_path.clone(),
        orchestrator_public_grpc_addr: config.orchestrator_public_grpc_addr.clone(),
        agent_metrics_port_range_start: config.agent_metrics_port_range_start,
        health_check_interval: Duration::from_secs(5),
        agent_health_timeout: config.agent_health_timeout,
    };
    let agent_manager_handle =
        AgentManager::spawn(agent_manager_config, state.clone(), shutdown_rx.clone());

    // Spawn the gRPC server
    let grpc_handle = {
        let s = state.clone();
        let m = metrics.clone();
        let addr = config.grpc_listen_addr;
        tokio::spawn(async move { grpc::serve_grpc(s, m, addr).await })
    };

    // Spawn the Arrow Flight server
    let flight_handle = {
        let svc = flight::make_server(state.clone(), metrics.clone());
        let addr = config.flight_listen_addr;
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(svc)
                .serve(addr)
                .await
                .context("Flight server failed")
        })
    };

    // Spawn the metrics server
    let metrics_handle = {
        let router = metrics.router();
        let addr = config.metrics_listen_addr;
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, router.into_make_service()).await?;
            Ok::<(), anyhow::Error>(())
        })
    };

    tracing::info!("All services started. Awaiting shutdown signal...");

    // Wait for shutdown signal
    shutdown_signal().await;

    tracing::info!("Shutdown signal received. Terminating services...");
    // The drop of the sender will cause all receivers to receive the shutdown signal.
    drop(shutdown_tx);

    // Await all tasks to ensure clean shutdown
    let (agent_res, grpc_res, flight_res, metrics_res) =
        tokio::join!(agent_manager_handle, grpc_handle, flight_handle, metrics_handle);

    if let Err(e) = agent_res {
        tracing::error!(error = %e, "Agent manager task failed.");
    }
    if let Err(e) = grpc_res {
        tracing::error!(error = %e, "gRPC server task failed.");
    }
    if let Err(e) = flight_res {
        tracing::error!(error = %e, "Flight server task failed.");
    }
    if let Err(e) = metrics_res {
        tracing::error!(error = %e, "Metrics server task failed.");
    }

    tracing::info!("Orchestrator shut down gracefully.");
    Ok(())
}

/// Listens for OS shutdown signals (SIGINT, SIGTERM) and resolves when one is received.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
