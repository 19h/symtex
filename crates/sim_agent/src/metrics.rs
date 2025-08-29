use axum::{response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, Gauge, Registry, TextEncoder};

/// A container for all Prometheus metrics exposed by the agent.
///
/// This struct initializes and registers metrics with a unique `agent_id` label,
/// and provides methods to update them and expose them via an HTTP endpoint.
pub struct AgentMetrics {
    pub registry: Registry,
    pub planning_loop_duration_seconds: Gauge,
    pub points_discovered_per_report: Gauge,
    pub grpc_connection_status: Gauge,
}

impl AgentMetrics {
    /// Creates and registers a new set of metrics for a given agent ID.
    pub fn new(agent_id: u64) -> Self {
        let registry = Registry::new_custom(Some("sim_agent".into()), None).unwrap();
        let agent_id_str = agent_id.to_string();

        macro_rules! reg_gauge {
            ($name:expr, $help:expr) => {{
                let gauge = Gauge::with_opts(
                    prometheus::Opts::new($name, $help)
                        .const_label("agent_id", &agent_id_str),
                )
                .unwrap();
                registry.register(Box::new(gauge.clone())).unwrap();
                gauge
            }};
        }

        Self {
            planning_loop_duration_seconds: reg_gauge!(
                "agent_planning_loop_duration_seconds",
                "Duration of the last planning loop in seconds."
            ),
            points_discovered_per_report: reg_gauge!(
                "agent_points_discovered_per_report",
                "Number of points in the last discovery report."
            ),
            grpc_connection_status: reg_gauge!(
                "agent_grpc_connection_status",
                "1 for connected, 0 for disconnected."
            ),
            registry,
        }
    }

    /// Creates an Axum router that serves the metrics on the /metrics endpoint.
    pub fn router(&self) -> Router {
        let registry = self.registry.clone();
        Router::new().route(
            "/metrics",
            get(move || {
                let reg = registry.clone();
                async move {
                    let metric_families = reg.gather();
                    let mut buffer = Vec::new();
                    let encoder = TextEncoder::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();
                    String::from_utf8(buffer).unwrap().into_response()
                }
            }),
        )
    }

    /// Sets the gRPC connection status metric.
    pub fn set_connection_status(&self, is_connected: bool) {
        self.grpc_connection_status
            .set(if is_connected { 1.0 } else { 0.0 });
    }

    /// Sets the planning loop duration metric.
    pub fn set_planning_duration(&self, duration_secs: f64) {
        self.planning_loop_duration_seconds.set(duration_secs);
    }

    /// Sets the points discovered per report metric.
    pub fn set_points_discovered_in_report(&self, count: u64) {
        self.points_discovered_per_report.set(count as f64);
    }
}
