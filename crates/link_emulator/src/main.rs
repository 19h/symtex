mod metrics;

use crate::metrics::EmulatorMetrics;
use anyhow::{anyhow, bail};
use std::{sync::Arc, time::SystemTime};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{sleep, Duration, Instant},
};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Clone, Debug)]
struct Config {
    listen: String,
    target: String,
    latency_ms: u64,
    jitter_ms: u64,
    rate_bps: u64,
    bucket_bytes: usize,
    stall_period_ms: u64,
    stall_duration_ms: u64,
    reset_chance_percent: u8,
    metrics_listen_addr: String,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let reset_chance_percent: u8 = std::env::var("EMULATOR_RESET_CHANCE_PERCENT")
            .unwrap_or_else(|_| "0".into())
            .parse()?;
        if reset_chance_percent > 100 {
            bail!("EMULATOR_RESET_CHANCE_PERCENT must be between 0 and 100");
        }

        Ok(Self {
            listen: std::env::var("EMULATOR_LISTEN_ADDR")
                .map_err(|_| anyhow!("EMULATOR_LISTEN_ADDR required"))?,
            target: std::env::var("EMULATOR_TARGET_ADDR")
                .map_err(|_| anyhow!("EMULATOR_TARGET_ADDR required"))?,
            metrics_listen_addr: std::env::var("EMULATOR_METRICS_LISTEN_ADDR")
                .map_err(|_| anyhow!("EMULATOR_METRICS_LISTEN_ADDR required"))?,
            latency_ms: std::env::var("EMULATOR_LATENCY_MS")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            jitter_ms: std::env::var("EMULATOR_JITTER_MS")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            rate_bps: std::env::var("EMULATOR_RATE_BPS")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            bucket_bytes: std::env::var("EMULATOR_BUCKET_BYTES")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            stall_period_ms: std::env::var("EMULATOR_STALL_PERIOD_MS")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            stall_duration_ms: std::env::var("EMULATOR_STALL_DURATION_MS")
                .unwrap_or_else(|_| "0".into())
                .parse()?,
            reset_chance_percent,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = Config::from_env()?;
    tracing::info!(config = ?cfg, "Starting link emulator");

    let metrics = Arc::new(EmulatorMetrics::new());

    // Start metrics server
    let router = metrics.router();
    let metrics_addr: std::net::SocketAddr = cfg.metrics_listen_addr.parse()?;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
        tracing::info!(addr = %metrics_addr, "Metrics server started");
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });

    let listener = TcpListener::bind(&cfg.listen).await?;
    tracing::info!(addr = cfg.listen, target = cfg.target, "Link emulator listening");

    loop {
        let (inbound, client_addr) = listener.accept().await?;
        let cfg_clone = cfg.clone();
        let metrics_clone = metrics.clone();

        tokio::spawn(async move {
            metrics_clone.connections_total.inc();
            metrics_clone.active_connections.inc();

            if let Err(e) = handle_connection(inbound, cfg_clone, metrics_clone.clone()).await {
                tracing::warn!(error = %e, client = %client_addr, "Connection ended with error");
            }

            metrics_clone.active_connections.dec();
        });
    }
}

async fn handle_connection(
    mut inbound: TcpStream,
    cfg: Config,
    metrics: Arc<EmulatorMetrics>,
) -> anyhow::Result<()> {
    let mut outbound = TcpStream::connect(&cfg.target).await?;
    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let c1 = impair_copy(
        &mut ri,
        &mut wo,
        &cfg,
        metrics.clone(),
        "client_to_server",
    );
    let c2 = impair_copy(
        &mut ro,
        &mut wi,
        &cfg,
        metrics.clone(),
        "server_to_client",
    );

    tokio::try_join!(c1, c2)?;
    Ok(())
}

async fn impair_copy<R: AsyncReadExt + Unpin, W: AsyncWriteExt + Unpin>(
    r: &mut R,
    w: &mut W,
    cfg: &Config,
    metrics: Arc<EmulatorMetrics>,
    direction: &str,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 16 * 1024];
    let mut bucket = cfg.bucket_bytes;
    let mut last_refill = Instant::now();
    let refill_interval = Duration::from_millis(10);
    let bytes_per_interval = if cfg.rate_bps == 0 {
        usize::MAX
    } else {
        std::cmp::max(1, cfg.rate_bps as usize / 100) // 100 intervals per second
    };

    let mut next_stall = if cfg.stall_period_ms > 0 {
        Instant::now() + Duration::from_millis(cfg.stall_period_ms)
    } else {
        Instant::now() + Duration::from_secs(3600 * 24) // Far future
    };

    loop {
        // Token bucket refill
        if last_refill.elapsed() >= refill_interval {
            bucket = std::cmp::min(
                bucket + bytes_per_interval,
                cfg.bucket_bytes.max(bytes_per_interval),
            );
            last_refill = Instant::now();
        }

        // Scheduled stall window
        if Instant::now() >= next_stall && cfg.stall_period_ms > 0 {
            if cfg.stall_duration_ms > 0 {
                tracing::debug!(duration_ms = cfg.stall_duration_ms, "Applying network stall");
                metrics.stall_windows_total.inc();
                sleep(Duration::from_millis(cfg.stall_duration_ms)).await;
            }
            next_stall += Duration::from_millis(cfg.stall_period_ms);
        }

        let n = r.read(&mut buf).await?;

        // Inject connection reset based on probability
        if n > 0 && cfg.reset_chance_percent > 0 {
            let roll = rand::random::<u8>() % 100;
            if roll < cfg.reset_chance_percent {
                metrics.resets_injected_total.inc();
                tracing::warn!(
                    chance = cfg.reset_chance_percent,
                    "Injecting connection reset"
                );
                return Err(anyhow!("injected connection reset"));
            }
        }

        if n == 0 {
            let _ = w.shutdown().await;
            return Ok(());
        }

        // Apply latency + jitter
        let jitter = if cfg.jitter_ms > 0 {
            rand::random::<u64>() % (cfg.jitter_ms + 1)
        } else {
            0
        };
        let total_delay = cfg.latency_ms + jitter;

        if total_delay > 0 {
            let delay_start = SystemTime::now();
            sleep(Duration::from_millis(total_delay)).await;
            let actual_delay = delay_start.elapsed().unwrap_or_default().as_secs_f64();
            metrics.latency_histogram.observe(actual_delay);
        }

        // Rate limiting via token bucket
        let mut sent = 0;
        while sent < n {
            // Wait for tokens if rate limiting is enabled
            if cfg.rate_bps > 0 && bucket == 0 {
                sleep(refill_interval).await;
                // Refill happens at the top of the loop, so we must continue here.
                bucket = std::cmp::min(
                    bucket + bytes_per_interval,
                    cfg.bucket_bytes.max(bytes_per_interval),
                );
                last_refill = Instant::now();
                continue;
            }

            let chunk_size = if cfg.rate_bps == 0 {
                n - sent
            } else {
                std::cmp::min(n - sent, bucket)
            };

            w.write_all(&buf[sent..sent + chunk_size]).await?;
            sent += chunk_size;

            // Deduct from token bucket
            if cfg.rate_bps > 0 {
                bucket = bucket.saturating_sub(chunk_size);
            }

            // Update metrics
            metrics
                .bytes_transferred_total
                .with_label_values(&[direction])
                .inc_by(chunk_size as u64);
        }
    }
}
