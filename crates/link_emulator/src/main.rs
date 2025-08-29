mod metrics;

use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, time::{sleep, Duration, Instant}};
use tracing_subscriber::{fmt, EnvFilter};
use std::{sync::Arc, time::SystemTime};
use metrics::EmulatorMetrics;

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
    metrics_port: u16,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            listen: std::env::var("EMULATOR_LISTEN_ADDR")
                .map_err(|_| anyhow::anyhow!("EMULATOR_LISTEN_ADDR required"))?,
            target: std::env::var("EMULATOR_TARGET_ADDR")
                .map_err(|_| anyhow::anyhow!("EMULATOR_TARGET_ADDR required"))?,
            latency_ms: std::env::var("EMULATOR_LATENCY_MS").unwrap_or_else(|_| "0".into()).parse()?,
            jitter_ms: std::env::var("EMULATOR_JITTER_MS").unwrap_or_else(|_| "0".into()).parse()?,
            rate_bps: std::env::var("EMULATOR_RATE_BPS").unwrap_or_else(|_| "0".into()).parse()?,
            bucket_bytes: std::env::var("EMULATOR_BUCKET_BYTES").unwrap_or_else(|_| "65536".into()).parse()?,
            stall_period_ms: std::env::var("EMULATOR_STALL_PERIOD_MS").unwrap_or_else(|_| "0".into()).parse()?,
            stall_duration_ms: std::env::var("EMULATOR_STALL_DURATION_MS").unwrap_or_else(|_| "0".into()).parse()?,
            metrics_port: std::env::var("EMULATOR_METRICS_PORT").unwrap_or_else(|_| "9099".into()).parse()?,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).json().init();
    
    let cfg = Config::from_env()?;
    tracing::info!(config = ?cfg, "Starting link emulator");

    let metrics = Arc::new(EmulatorMetrics::new());

    // Start metrics server
    if cfg.metrics_port > 0 {
        let router = metrics.router();
        let metrics_addr = std::net::SocketAddr::from(([0, 0, 0, 0], cfg.metrics_port));
        tokio::spawn(async move {
            axum::Server::bind(&metrics_addr)
                .serve(router.into_make_service())
                .await
                .unwrap();
        });
        tracing::info!(port = cfg.metrics_port, "Metrics server started");
    }

    let listener = TcpListener::bind(&cfg.listen).await?;
    tracing::info!(addr = cfg.listen, target = cfg.target, "Link emulator listening");

    loop {
        let (inbound, client_addr) = listener.accept().await?;
        let cfg_clone = cfg.clone();
        let metrics_clone = metrics.clone();
        
        tokio::spawn(async move {
            metrics_clone.connections_total.inc();
            metrics_clone.current_connections.inc();
            
            if let Err(e) = handle_connection(inbound, cfg_clone, metrics_clone.clone()).await {
                tracing::warn!(error = %e, client = %client_addr, "Connection ended with error");
            }
            
            metrics_clone.current_connections.dec();
        });
    }
}

async fn handle_connection(mut inbound: TcpStream, cfg: Config, metrics: Arc<EmulatorMetrics>) -> anyhow::Result<()> {
    let mut outbound = TcpStream::connect(cfg.target).await?;
    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let c1 = impair_copy(&mut ri, &mut wo, &cfg, metrics.clone(), "client_to_server");
    let c2 = impair_copy(&mut ro, &mut wi, &cfg, metrics.clone(), "server_to_client");
    
    tokio::try_join!(c1, c2)?;
    Ok(())
}

async fn impair_copy<R: AsyncReadExt + Unpin, W: AsyncWriteExt + Unpin>(
    r: &mut R, 
    w: &mut W, 
    cfg: &Config,
    metrics: Arc<EmulatorMetrics>,
    direction: &str
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

    let direction_label = prometheus::Labels::from(vec![("direction", direction)]);

    loop {
        // Token bucket refill
        if last_refill.elapsed() >= refill_interval {
            bucket = std::cmp::min(bucket + bytes_per_interval, cfg.bucket_bytes.max(bytes_per_interval));
            last_refill = Instant::now();
        }

        // Scheduled stall window
        if Instant::now() >= next_stall && cfg.stall_period_ms > 0 {
            if cfg.stall_duration_ms > 0 {
                tracing::debug!(duration_ms = cfg.stall_duration_ms, "Applying network stall");
                metrics.stalls_total.inc();
                sleep(Duration::from_millis(cfg.stall_duration_ms)).await;
            }
            next_stall += Duration::from_millis(cfg.stall_period_ms);
        }

        let n = r.read(&mut buf).await?;
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
            metrics.bytes_transferred_total
                .with(&direction_label)
                .inc_by(chunk_size as u64);
        }
    }
}
