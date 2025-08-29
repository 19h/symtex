// symtex/crates/link_emulator/src/metrics.rs
use axum::{response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, Gauge, Histogram, IntCounter, IntCounterVec, Registry, TextEncoder};

pub struct EmulatorMetrics {
    pub registry: Registry,
    pub connections_total: IntCounter,
    pub bytes_transferred_total: IntCounterVec,
    pub resets_injected_total: IntCounter,
    pub latency_histogram: Histogram,
    pub active_connections: Gauge,
    pub stall_windows_total: IntCounter,
}

impl EmulatorMetrics {
    pub fn new() -> Self {
        let registry = Registry::new_custom(Some("holo_c2_proxy".into()), None).unwrap();

        macro_rules! reg {
            ($m:expr) => {{
                registry.register(Box::new($m.clone())).unwrap();
                $m
            }};
        }

        Self {
            connections_total: reg!(IntCounter::with_opts(prometheus::Opts::new(
                "proxy_connections_total",
                "Total connections proxied"
            ))
            .unwrap()),
            bytes_transferred_total: reg!(IntCounterVec::new(
                prometheus::Opts::new("proxy_bytes_transferred_total", "Total bytes transferred"),
                &["direction"]
            )
            .unwrap()),
            resets_injected_total: reg!(IntCounter::new(
                "proxy_resets_injected_total",
                "Total number of injected connection resets"
            )
            .unwrap()),
            latency_histogram: reg!(Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "proxy_latency_seconds",
                    "Added latency distribution"
                )
                .buckets(prometheus::exponential_buckets(0.001, 2.0, 15).unwrap())
            )
            .unwrap()),
            active_connections: reg!(Gauge::new(
                "proxy_active_connections",
                "Number of currently active proxied connections"
            )
            .unwrap()),
            stall_windows_total: reg!(IntCounter::new(
                "proxy_stall_windows_total",
                "Total number of injected stall windows"
            )
            .unwrap()),
            registry,
        }
    }

    pub fn router(&self) -> Router {
        let reg = self.registry.clone();
        Router::new().route(
            "/metrics",
            get(move || {
                let reg = reg.clone();
                async move {
                    let mf = reg.gather();
                    let mut buf = Vec::new();
                    TextEncoder::new().encode(&mf, &mut buf).unwrap();
                    String::from_utf8(buf).unwrap().into_response()
                }
            }),
        )
    }
}
