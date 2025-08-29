use clap::Parser;
use std::path::PathBuf;

/// `sim_agent` - A headless autonomous agent for the Holographic C2 project.
///
/// This process simulates a single autonomous agent, responsible for its own
/// perception and navigation. It connects to a central `sim_orchestrator` to
/// receive tasks and report its findings.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Config {
    /// The gRPC address of the `sim_orchestrator` service.
    ///
    /// The agent will connect to this endpoint to register itself and report
    /// its state.
    #[arg(long, env = "ORCHESTRATOR_GRPC_ADDR")]
    pub orchestrator_grpc_addr: String,

    /// The listen address for the agent's own Prometheus metrics server.
    ///
    /// The agent exposes its internal metrics on this address in a format
    /// that can be scraped by a Prometheus instance.
    #[arg(long, env = "AGENT_METRICS_LISTEN_ADDR")]
    pub metrics_listen_addr: String,

    /// The filesystem path to the `.hypc` point cloud data file.
    ///
    /// This file is loaded into GPU memory at startup and is used by the
    /// perception system to simulate LiDAR scans.
    #[arg(long, env = "POINT_CLOUD_PATH")]
    pub point_cloud_path: PathBuf,
}
