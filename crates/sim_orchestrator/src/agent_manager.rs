// symtex/crates/sim_orchestrator/src/agent_manager.rs
use crate::state::{AgentRuntimeInfo, CanonicalState};
use anyhow::Context;
use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::{
    process::Command,
    sync::watch,
    task::JoinHandle,
    time::sleep,
};

/// Configuration for the AgentManager.
#[derive(Debug, Clone)]
pub struct AgentManagerConfig {
    pub num_agents: u32,
    pub agent_binary_path: String,
    pub orchestrator_public_grpc_addr: String,
    pub agent_metrics_port_range_start: u16,
    pub health_check_interval: Duration,
    pub agent_health_timeout: Duration,
}

/// Manages the lifecycle of `sim_agent` child processes.
pub struct AgentManager {
    config: AgentManagerConfig,
    state: Arc<CanonicalState>,
    next_metrics_port: AtomicU16,
}

impl AgentManager {
    /// Creates a new AgentManager and spawns its background tasks.
    pub fn spawn(
        config: AgentManagerConfig,
        state: Arc<CanonicalState>,
        mut shutdown_rx: watch::Receiver<()>,
    ) -> JoinHandle<anyhow::Result<()>> {
        let next_metrics_port =
            AtomicU16::new(config.agent_metrics_port_range_start);

        let manager = Arc::new(AgentManager {
            config,
            state,
            next_metrics_port,
        });

        tokio::spawn(async move {
            tracing::info!("AgentManager started.");

            let manager_clone = manager.clone();
            let run_handle = tokio::spawn(async move {
                manager_clone.run().await
            });

            // Wait for either a shutdown signal or the main loop to exit.
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    tracing::info!("Shutdown signal received, terminating agents.");
                }
                res = run_handle => {
                    match res {
                        Ok(Ok(())) => tracing::info!("AgentManager run loop completed."),
                        Ok(Err(e)) => tracing::error!(error = %e, "AgentManager run loop failed."),
                        Err(e) => tracing::error!(error = %e, "AgentManager run loop panicked."),
                    }
                }
            }

            // Perform final cleanup.
            manager.terminate_all_agents().await;
            tracing::info!("AgentManager has shut down.");
            Ok(())
        })
    }

    /// Runs the initial agent spawning and the health check loop.
    async fn run(&self) -> anyhow::Result<()> {
        for i in 0..self.config.num_agents {
            if let Err(e) = self.spawn_agent().await {
                tracing::error!(agent_index = i, error = %e, "Failed to spawn initial agent");
            }
        }

        self.health_check_loop().await;
        Ok(())
    }

    /// Spawns a single `sim_agent` child process.
    async fn spawn_agent(&self) -> anyhow::Result<()> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let metrics_port = self
            .next_metrics_port
            .fetch_add(1, Ordering::Relaxed);

        tracing::info!(session_id, metrics_port, "Spawning new agent process");

        let mut command = Command::new(&self.config.agent_binary_path);
        command
            .env(
                "ORCHESTRATOR_PUBLIC_GRPC_ADDR",
                &self.config.orchestrator_public_grpc_addr,
            )
            .env("AGENT_SESSION_ID", &session_id)
            .env("AGENT_METRICS_PORT", metrics_port.to_string())
            .env("RUST_LOG", "info,h2=warn,hyper=warn,tower=warn") // Sensible defaults
            .kill_on_drop(true);

        let child = command
            .spawn()
            .with_context(|| format!("Failed to spawn agent binary at '{}'", self.config.agent_binary_path))?;

        // Insert the process handle into the pending map. The gRPC service will move it
        // to the main agents map upon successful registration.
        self.state.pending_registrations.insert(session_id, child);

        Ok(())
    }

    /// Periodically checks for stale or terminated agents and cleans them up.
    async fn health_check_loop(&self) {
        loop {
            sleep(self.config.health_check_interval).await;
            tracing::debug!("Running agent health check...");

            let mut agents_to_remove = Vec::new();

            for mut entry in self.state.agents.iter_mut() {
                let agent_id = *entry.key();
                let agent_info = entry.value_mut();

                // Check if the process has already exited
                if let Some(handle) = agent_info.process_handle.as_mut() {
                    match handle.try_wait() {
                        Ok(Some(status)) => {
                            tracing::warn!(agent_id, exit_status = %status, "Agent process terminated unexpectedly.");
                            agents_to_remove.push(agent_id);
                            continue; // Skip further checks for this agent
                        }
                        Ok(None) => { // Process is still running
                        }
                        Err(e) => {
                            tracing::error!(agent_id, error = %e, "Error checking agent process status.");
                            agents_to_remove.push(agent_id);
                            continue;
                        }
                    }
                }

                // Check if the agent is stale (hasn't reported in a while)
                if agent_info.last_seen.elapsed() > self.config.agent_health_timeout {
                    tracing::warn!(agent_id, "Agent is stale. Terminating.");
                    self.terminate_agent(agent_info).await;
                    agents_to_remove.push(agent_id);
                }
            }

            // Remove the dead/stale agents from the main state map
            for agent_id in agents_to_remove {
                if self.state.agents.remove(&agent_id).is_some() {
                    tracing::info!(agent_id, "Removed agent from state.");
                    self.state.broadcast_world_state();
                }
            }
        }
    }

    /// Terminates a single agent's process, gracefully at first, then forcefully.
    async fn terminate_agent(&self, agent_info: &mut AgentRuntimeInfo) {
        if let Some(mut child) = agent_info.process_handle.take() {
            if let Some(pid) = child.id() {
                tracing::debug!(pid, "Sending SIGTERM to agent process.");
                // Use nix::sys::signal for process group signaling if needed, but for now, this is fine.
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGTERM,
                );

                // Wait for a grace period
                tokio::select! {
                    _ = sleep(Duration::from_secs(2)) => {
                        tracing::warn!(pid, "Agent did not terminate gracefully. Sending SIGKILL.");
                        if let Err(e) = child.start_kill() {
                            tracing::error!(pid, error = %e, "Failed to SIGKILL agent process.");
                        }
                    }
                    _ = child.wait() => {
                        tracing::debug!(pid, "Agent terminated gracefully.");
                    }
                }
            }
        }
    }

    /// Terminates all managed agent processes during shutdown.
    async fn terminate_all_agents(&self) {
        tracing::info!("Terminating all managed agent processes...");
        for mut entry in self.state.agents.iter_mut() {
            self.terminate_agent(entry.value_mut()).await;
        }
        self.state.agents.clear();

        // For pending agents, just clearing the map is enough due to kill_on_drop(true)
        let pending_count = self.state.pending_registrations.len();
        if pending_count > 0 {
            tracing::info!("Terminating {} pending (unregistered) agents...", pending_count);
            self.state.pending_registrations.clear();
        }

        tracing::info!("All agent processes terminated.");
    }
}
