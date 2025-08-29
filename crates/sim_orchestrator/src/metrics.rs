use axum::{response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, Gauge, IntCounter, IntGauge, Registry, TextEncoder};

/// A container for all Prometheus metric collectors for the sim_orchestrator.
///
/// This struct is designed to be wrapped in an `Arc` and shared across
/// all concurrent tasks of the application.
pub struct Metrics {
    pub registry: Registry,
    /// Total number of agents that have ever registered with the orchestrator.
    pub agents_registered_total: IntCounter,
    /// The number of currently active and connected agents.
    pub agents_active: IntGauge,
    /// Total number of unique points revealed across the entire simulation.
    pub points_revealed_total: IntCounter,
    /// The current ratio of revealed points to total points (0.0 to 1.0).
    pub map_coverage_ratio: Gauge,
    /// Total number of gRPC requests handled by the C2 service.
    pub grpc_requests_total: IntCounter,
    /// Total number of Arrow Flight requests handled.
    pub flight_requests_total: IntCounter,
}

impl Metrics {
    /// Creates a new `Metrics` struct, initializing and registering all collectors.
    pub fn new() -> Self {
        // Create a custom registry to avoid conflicts with default metrics.
        let registry = Registry::new_custom(Some("sim_orchestrator".into()), None)
            .expect("Failed to create custom metrics registry");

        // A helper macro to create, register, and return a metric collector.
        macro_rules! reg {
            ($metric:expr) => {{
                let collector = $metric;
                registry
                    .register(Box::new(collector.clone()))
                    .expect("Failed to register metric");
                collector
            }};
        }

        Self {
            agents_registered_total: reg!(IntCounter::new(
                "agents_registered_total",
                "Total number of agents that have ever registered"
            )
            .unwrap()),
            agents_active: reg!(IntGauge::new(
                "agents_active",
                "Number of currently active agents"
            )
            .unwrap()),
            points_revealed_total: reg!(IntCounter::new(
                "points_revealed_total",
                "Total number of unique points revealed by all agents"
            )
            .unwrap()),
            map_coverage_ratio: reg!(Gauge::new(
                "map_coverage_ratio",
                "The ratio of revealed points to total points in the point cloud"
            )
            .unwrap()),
            grpc_requests_total: reg!(IntCounter::new(
                "grpc_requests_total",
                "Total number of gRPC requests received"
            )
            .unwrap()),
            flight_requests_total: reg!(IntCounter::new(
                "flight_requests_total",
                "Total number of Arrow Flight DoGet requests received"
            )
            .unwrap()),
            registry,
        }
    }

    /// Creates an `axum::Router` that serves the metrics on the `/metrics` endpoint.
    pub fn router(&self) -> Router {
        let registry = self.registry.clone();
        Router::new().route(
            "/metrics",
            get(move || {
                let registry = registry.clone();
                async move {
                    let metric_families = registry.gather();
                    let mut buffer = Vec::new();
                    let encoder = TextEncoder::new();
                    encoder
                        .encode(&metric_families, &mut buffer)
                        .expect("Failed to encode metrics");
                    String::from_utf8(buffer)
                        .expect("Metrics buffer is not valid UTF-8")
                        .into_response()
                }
            }),
        )
    }

    /// Sets the value of the map coverage gauge.
    pub fn update_coverage(&self, coverage_ratio: f64) {
        self.map_coverage_ratio.set(coverage_ratio);
    }

    /// Sets the value of the active agents gauge.
    pub fn update_active_agents(&self, count: i64) {
        self.agents_active.set(count);
    }
}
