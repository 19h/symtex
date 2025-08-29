mod net;
mod render;
mod ui;

use crossbeam_channel::bounded;
use render::RenderSystem;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let c2_grpc_addr = std::env::args()
        .position(|arg| arg == "--c2-grpc-addr")
        .and_then(|pos| std::env::args().nth(pos + 1))
        .unwrap_or_else(|| "http://127.0.0.1:50051".to_string());

    tracing::info!(addr = %c2_grpc_addr, "Starting holographic viewer");

    // Create bounded channel for world state updates
    let (tx, rx) = bounded(2); // Small buffer to prevent excessive memory use

    // Spawn network thread
    let network_handle = net::spawn_network(c2_grpc_addr, tx);

    // Run render loop on main thread
    let mut render_system = RenderSystem::new();
    
    match render_system.run_render_loop(rx) {
        Ok(()) => tracing::info!("Render loop completed"),
        Err(e) => {
            tracing::error!(error = %e, "Render loop error");
            return Err(e);
        }
    }

    // Wait for network thread to complete
    if let Err(e) = network_handle.join() {
        tracing::error!(error = ?e, "Network thread panicked");
    }

    Ok(())
}
