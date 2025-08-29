use prometheus::{Encoder, TextEncoder, Registry, IntCounter, Gauge};
use axum::{routing::get, Router, response::IntoResponse};

pub struct AgentMetrics {
    pub registry: Registry,
    pub reports_sent_total: IntCounter,
    pub points_discovered_total: IntCounter,
    pub connection_errors_total: IntCounter,
    pub position_x: Gauge,
    pub position_y: Gauge,
    pub position_z: Gauge,
}

impl AgentMetrics {
    pub fn new(agent_id: u64) -> Self {
        let registry = Registry::new_custom(Some("holo_c2_agent".into()), None).unwrap();
        
        macro_rules! reg {
            ($m:expr) => {{
                registry.register(Box::new($m.clone())).unwrap(); 
                $m
            }}
        }
        
        Self {
            reports_sent_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("agent_reports_sent_total", "Total reports sent to orchestrator")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
            points_discovered_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("agent_points_discovered_total", "Total points discovered")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
            connection_errors_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("agent_connection_errors_total", "Total connection errors")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
            position_x: reg!(Gauge::with_opts(
                prometheus::Opts::new("agent_position_x", "Agent X position")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
            position_y: reg!(Gauge::with_opts(
                prometheus::Opts::new("agent_position_y", "Agent Y position")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
            position_z: reg!(Gauge::with_opts(
                prometheus::Opts::new("agent_position_z", "Agent Z position")
                    .const_label("agent_id", &agent_id.to_string())
            ).unwrap()),
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

    pub fn update_position(&self, x: f64, y: f64, z: f64) {
        self.position_x.set(x);
        self.position_y.set(y);
        self.position_z.set(z);
    }
}
