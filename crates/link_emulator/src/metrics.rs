use prometheus::{Encoder, TextEncoder, Registry, IntCounter, Histogram, Gauge};
use axum::{routing::get, Router, response::IntoResponse};

pub struct EmulatorMetrics {
    pub registry: Registry,
    pub connections_total: IntCounter,
    pub bytes_transferred_total: IntCounter,
    pub bytes_dropped_total: IntCounter,
    pub latency_histogram: Histogram,
    pub current_connections: Gauge,
    pub stalls_total: IntCounter,
}

impl EmulatorMetrics {
    pub fn new() -> Self {
        let registry = Registry::new_custom(Some("holo_c2_proxy".into()), None).unwrap();
        
        macro_rules! reg {
            ($m:expr) => {{
                registry.register(Box::new($m.clone())).unwrap(); 
                $m
            }}
        }
        
        Self {
            connections_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("proxy_connections_total", "Total connections proxied")
            ).unwrap()),
            bytes_transferred_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("proxy_bytes_transferred_total", "Total bytes transferred")
                    .variable_label("direction")
            ).unwrap()),
            bytes_dropped_total: reg!(IntCounter::with_opts(
                prometheus::Opts::new("proxy_bytes_dropped_total", "Total bytes dropped")
            ).unwrap()),
            latency_histogram: reg!(Histogram::with_opts(
                prometheus::HistogramOpts::new("proxy_latency_seconds", "Added latency distribution")
                    .buckets(prometheus::exponential_buckets(0.001, 2.0, 15).unwrap())
            ).unwrap()),
            current_connections: reg!(Gauge::new("proxy_current_connections", "Current active connections").unwrap()),
            stalls_total: reg!(IntCounter::new("proxy_stalls_total", "Total network stalls applied").unwrap()),
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
}
