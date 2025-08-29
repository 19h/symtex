use prometheus::{Encoder, TextEncoder, Registry, IntCounter, IntGauge, Gauge};
use axum::{routing::get, Router, response::IntoResponse};

pub struct Metrics {
    pub registry: Registry,
    pub agents_registered_total: IntCounter,
    pub agents_active: IntGauge,
    pub points_revealed_total: IntCounter,
    pub map_coverage_ratio: Gauge,
    pub grpc_requests_total: IntCounter,
    pub flight_requests_total: IntCounter,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new_custom(Some("holo_c2".into()), None).unwrap();
        
        macro_rules! reg {
            ($m:expr) => {{
                registry.register(Box::new($m.clone())).unwrap(); 
                $m
            }}
        }
        
        Self {
            agents_registered_total: reg!(IntCounter::new("sim_agents_registered_total", "Total agents registered").unwrap()),
            agents_active: reg!(IntGauge::new("sim_agents_active", "Currently active agents").unwrap()),
            points_revealed_total: reg!(IntCounter::new("sim_points_revealed_total", "Total points revealed").unwrap()),
            map_coverage_ratio: reg!(Gauge::new("sim_map_coverage_ratio", "Map coverage ratio").unwrap()),
            grpc_requests_total: reg!(IntCounter::new("sim_grpc_requests_total", "Total gRPC requests").unwrap()),
            flight_requests_total: reg!(IntCounter::new("sim_flight_requests_total", "Total Arrow Flight requests").unwrap()),
            registry
        }
    }

    pub fn router(&self) -> Router {
        let reg = self.registry.clone();
        Router::new().route("/metrics", get(move || {
            let reg = reg.clone();
            async move {
                let mf = reg.gather();
                let mut buf = Vec::new();
                TextEncoder::new().encode(&mf, &mut buf).unwrap();
                String::from_utf8(buf).unwrap().into_response()
            }
        }))
    }

    pub fn update_coverage(&self, coverage_ratio: f64) {
        self.map_coverage_ratio.set(coverage_ratio);
    }

    pub fn update_active_agents(&self, count: i64) {
        self.agents_active.set(count);
    }
}
