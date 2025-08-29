mod communication; mod metrics; mod perception;

use std::{sync::Arc, time::Duration};
use tracing_subscriber::{fmt, EnvFilter};
use tokio::sync::mpsc;
use api::gen::api::v1::{AgentReport, AgentState, AgentMode, Vec3m, Vec3mps, UnitQuaternion};
use metrics::AgentMetrics;
use perception::PerceptionSystem;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).json().init();

    let grpc = std::env::var("ORCHESTRATOR_PUBLIC_GRPC_ADDR")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".into());
    let metrics_port: u16 = std::env::var("AGENT_METRICS_PORT")
        .unwrap_or_else(|_| "0".into()) // 0 = random port
        .parse()?;
    
    let session_id = uuid::Uuid::new_v4().to_string();

    // Connect and register
    let mut comm = communication::Comm::connect(&grpc).await?;
    let agent_id = comm.register(&session_id).await?;
    
    tracing::info!(agent_id, session_id, "Agent registered successfully");

    // Initialize metrics and perception
    let metrics = Arc::new(AgentMetrics::new(agent_id));
    let perception = PerceptionSystem::new(50.0, 8, 0.1); // 50m range, 8x8x8 grid, 10% noise

    // Start metrics server if port specified
    if metrics_port > 0 {
        let router = metrics.router();
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], metrics_port));
        tokio::spawn(async move {
            axum::Server::bind(&addr)
                .serve(router.into_make_service())
                .await
                .unwrap();
        });
        tracing::info!(port = metrics_port, "Agent metrics server started");
    }

    let (tx, rx) = mpsc::channel::<AgentReport>(16);

    // Simulation loop - generate agent behavior and reports
    let sim_metrics = metrics.clone();
    tokio::spawn(async move {
        let mut sequence = 0u64;
        let mut position = Vec3m { x: 0.0, y: 0.0, z: 0.0 };
        
        loop {
            // Simple circular motion simulation
            let time = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
            position.x = 20.0 * (time * 0.1).cos();
            position.y = 20.0 * (time * 0.1).sin();
            position.z = 5.0 * (time * 0.05).sin();
            
            let velocity = Vec3mps {
                x: -2.0 * (time * 0.1).sin(),
                y: 2.0 * (time * 0.1).cos(),
                z: 0.25 * (time * 0.05).cos(),
            };

            let agent_state = AgentState {
                agent_id,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                position_ecef_m: Some(position.clone()),
                velocity_ecef_mps: Some(velocity),
                orientation_ecef: Some(UnitQuaternion { w: 1.0, x: 0.0, y: 0.0, z: 0.0 }),
                mode: AgentMode::Scanning as i32,
                sequence,
                schema_version: 1,
            };

            // Update metrics
            sim_metrics.update_position(position.x, position.y, position.z);

            // Simulate point discovery
            let discovered_points = match perception.discover_points(&agent_state) {
                Ok(points) => {
                    // Count discovered points for metrics
                    if !points.is_empty() {
                        let bitmap = roaring::RoaringBitmap::deserialize_from(&mut points.as_slice()).unwrap_or_default();
                        sim_metrics.points_discovered_total.inc_by(bitmap.len());
                    }
                    points
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to discover points");
                    Vec::new()
                }
            };

            let report = AgentReport {
                agent_id,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                state: Some(agent_state),
                discovered_point_ids_portable: discovered_points,
            };

            if tx.send(report).await.is_err() { 
                tracing::warn!("Report channel closed, stopping simulation");
                break; 
            }
            
            sim_metrics.reports_sent_total.inc();
            sequence += 1;

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    // Handle responses from orchestrator
    let response_metrics = metrics.clone();
    let response_handler = move |resp: api::gen::api::v1::ReportStateResponse| {
        if let Some(task) = resp.assigned_task {
            tracing::info!(task_id = task.task_id, task_type = ?task.task_type, "Received task assignment");
            // TODO: Implement task execution logic
        }
    };

    // Run the bidirectional communication
    match comm.run_report_state(rx, response_handler).await {
        Ok(()) => tracing::info!("Communication completed normally"),
        Err(e) => {
            tracing::error!(error = %e, "Communication error");
            response_metrics.connection_errors_total.inc();
        }
    }

    Ok(())
}
