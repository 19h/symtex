mod communication;
mod config;
mod metrics;
mod perception;
mod state;

use crate::config::Config;
use crate::state::AgentMachine;
use api::gen::api::v1::{AgentReport, Task};
use clap::Parser;
use metrics::AgentMetrics;
use perception::PerceptionSystem;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing_subscriber::{fmt, EnvFilter};

const AGENT_TICK_RATE_HZ: u64 = 10;
const AGENT_REPORT_INTERVAL_MS: u64 = 500;
const AGENT_SCAN_RANGE_M: f32 = 50.0;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // --- 1. Initialization ---
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();
    let config = Config::parse();
    tracing::info!(config = ?config, "Agent starting with configuration");

    let session_id = uuid::Uuid::new_v4().to_string();

    // Initialize perception system (this can take a moment for GPU setup)
    let perception_system =
        PerceptionSystem::new(AGENT_SCAN_RANGE_M, &config.point_cloud_path).await?;

    // Connect and register with the orchestrator
    let mut comm = communication::Comm::connect(&config.orchestrator_grpc_addr).await?;
    let agent_id = comm.register(&session_id).await?;
    tracing::info!(agent_id, session_id, "Agent registered successfully");

    // Initialize metrics and state machine
    let metrics = Arc::new(AgentMetrics::new(agent_id));
    let mut agent_machine = AgentMachine::new(agent_id);

    // --- 2. Start Metrics Server ---
    let metrics_router = metrics.clone().router();
    let metrics_addr: std::net::SocketAddr = config.metrics_listen_addr.parse()?;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(metrics_addr)
            .await
            .unwrap();
        tracing::info!(addr = %metrics_addr, "Agent metrics server started");
        axum::serve(listener, metrics_router.into_make_service())
            .await
            .unwrap();
    });

    // --- 3. Spawn Communication Task ---
    let (tx_reports, rx_reports) = mpsc::channel::<AgentReport>(32);
    let (tx_tasks, mut rx_tasks) = mpsc::channel::<Task>(32);
    let comm_metrics = metrics.clone();
    tokio::spawn(async move {
        if let Err(e) = comm
            .run_report_stream(comm_metrics, rx_reports, tx_tasks)
            .await
        {
            tracing::error!(error = %e, "Communication task exited with an error.");
        } else {
            tracing::info!("Communication task finished gracefully.");
        }
    });

    // --- 4. Main Control Loop ---
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / AGENT_TICK_RATE_HZ));
    let mut last_tick = Instant::now();
    let mut last_report_time = Instant::now();

    tracing::info!("Starting main control loop...");
    loop {
        tokio::select! {
            // Handle graceful shutdown
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received.");
                break;
            },
            // Handle incoming tasks from the orchestrator
            Some(task) = rx_tasks.recv() => {
                tracing::info!(task_id = ?task.target_waypoint_ecef_m, "Received new task assignment");
                agent_machine.assign_task(task);
            },
            // Handle the main agent tick
            _ = interval.tick() => {
                let now = Instant::now();
                let dt = now.duration_since(last_tick);
                last_tick = now;

                // Update agent physics and state machine
                agent_machine.tick(dt);

                // If the agent is in a state to perceive, run the LiDAR scan
                if agent_machine.mode == state::Mode::Perceiving {
                    match perception_system.run_lidar_scan(&agent_machine.pose) {
                        Ok(discovered) => {
                            if !discovered.is_empty() {
                                agent_machine.discovery_buffer |= &discovered;
                            }
                        },
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to run LiDAR scan");
                        }
                    }
                }

                // Periodically send a report to the orchestrator
                if now.duration_since(last_report_time).as_millis() >= AGENT_REPORT_INTERVAL_MS as u128 {
                    match agent_machine.get_report_and_clear_buffer() {
                        Ok(report) => {
                            let num_discovered = roaring::RoaringBitmap::deserialize_from(&mut report.discovered_point_ids_portable.as_slice()).map_or(0, |rb| rb.len());
                            metrics.set_points_discovered_in_report(num_discovered);

                            if let Err(e) = tx_reports.try_send(report) {
                                tracing::warn!(error = %e, "Failed to send report to comms task; channel may be full.");
                            }
                        },
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to create agent report");
                        }
                    }
                    last_report_time = now;
                }
            }
        }
    }

    tracing::info!("Agent shutting down.");
    Ok(())
}
