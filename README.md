# Project Holographic C2

A distributed simulation system implementing the Holographic C2 v1.3 specification with gRPC communication, Arrow Flight data streaming, and comprehensive network emulation capabilities.

## Architecture

This project implements a complete distributed simulation ecosystem:

- **sim_orchestrator**: Central coordination service with gRPC API, Arrow Flight server, and state management
- **sim_agent**: Autonomous agents with bidirectional streaming and perception simulation  
- **holographic-viewer**: Thin-client viewer for real-time visualization
- **link_emulator**: Network impairment proxy with latency, jitter, rate limiting, and stall simulation
- **Observability**: Prometheus metrics collection and Grafana dashboards

## Protocol Specification

The system implements the v1.3 Holographic C2 protocol featuring:

- Transport-level liveness with HTTP/2 keep-alive defaults
- Portable Roaring bitmap serialization for point cloud reveal masks
- Arrow Flight integration for high-performance data distribution
- Bidirectional streaming for real-time agent communication
- Opaque ticket-based access control for data streams

## Quick Start

### Prerequisites

- Rust 1.75+ with stable toolchain
- Docker and Docker Compose
- Protocol Buffer compiler (`protobuf-compiler`)

### Local Development

1. **Build the workspace**:
```bash
cargo build --workspace
```

2. **Run the orchestrator locally**:
```bash
cargo run --bin sim_orchestrator
```

3. **Run an agent (in another terminal)**:
```bash
ORCHESTRATOR_PUBLIC_GRPC_ADDR=http://127.0.0.1:50051 cargo run --bin sim_agent
```

4. **Run the viewer**:
```bash
cargo run --bin holographic-viewer -- --c2-grpc-addr http://127.0.0.1:50051
```

### Docker Deployment

1. **Start the full stack**:
```bash
docker-compose up --build
```

2. **Access the services**:
   - Grafana dashboard: http://localhost:3000 (admin/admin)
   - Prometheus: http://localhost:9090
   - Orchestrator gRPC: localhost:50051 (direct) or localhost:60051 (via emulator)
   - Orchestrator Flight: localhost:50052 (direct) or localhost:60052 (via emulator)

3. **Run agents against the emulated network**:
```bash
ORCHESTRATOR_PUBLIC_GRPC_ADDR=http://127.0.0.1:60051 cargo run --bin sim_agent
```

4. **Run the viewer against the emulated network**:
```bash
cargo run --bin holographic-viewer -- --c2-grpc-addr http://127.0.0.1:60051 --c2-flight-addr http://127.0.0.1:60052
```

## Components

### Orchestrator (`sim_orchestrator`)

The central coordination service providing:
- gRPC API for agent registration and bidirectional streaming
- Arrow Flight server for reveal mask distribution
- Canonical state management with agent tracking
- Prometheus metrics endpoint

Environment variables:
- `ORCHESTRATOR_GRPC_LISTEN_ADDR` (default: 0.0.0.0:50051)
- `ORCHESTRATOR_FLIGHT_LISTEN_ADDR` (default: 0.0.0.0:50052)  
- `ORCHESTRATOR_METRICS_LISTEN_ADDR` (default: 0.0.0.0:9091)

### Agent (`sim_agent`)

Autonomous simulation agents featuring:
- Bidirectional gRPC streaming with keep-alive
- Perception system with point discovery simulation
- Circular motion pattern for demonstration
- Individual Prometheus metrics

Environment variables:
- `ORCHESTRATOR_PUBLIC_GRPC_ADDR` (default: http://127.0.0.1:50051)
- `AGENT_METRICS_PORT` (default: 0 = disabled)

### Viewer (`holographic-viewer`)

Thin-client visualization system with:
- Network thread for world state subscriptions
- Render loop with agent position tracking
- Placeholder for 3D graphics integration

Command-line options:
- `--c2-grpc-addr` (default: http://127.0.0.1:50051)

### Link Emulator (`link_emulator`)

Network impairment proxy supporting:
- **Latency**: Fixed delay with optional jitter
- **Rate limiting**: Token bucket algorithm  
- **Stalls**: Periodic connection freezes
- **Metrics**: Comprehensive network performance tracking

Environment variables:
- `EMULATOR_LISTEN_ADDR` (required)
- `EMULATOR_TARGET_ADDR` (required)
- `EMULATOR_LATENCY_MS` (default: 0)
- `EMULATOR_JITTER_MS` (default: 0)
- `EMULATOR_RATE_BPS` (default: 0 = unlimited)
- `EMULATOR_BUCKET_BYTES` (default: 65536)
- `EMULATOR_STALL_PERIOD_MS` (default: 0 = disabled)
- `EMULATOR_STALL_DURATION_MS` (default: 0)

## Monitoring

### Metrics

All services expose Prometheus metrics:

- **Orchestrator** (port 9091):
  - `holo_c2_sim_agents_active`
  - `holo_c2_sim_map_coverage_ratio`
  - `holo_c2_sim_points_revealed_total`
  - `holo_c2_sim_grpc_requests_total`

- **Link Emulator** (port 9098/9099):
  - `holo_c2_proxy_latency_seconds`
  - `holo_c2_proxy_bytes_transferred_total{direction}`
  - `holo_c2_proxy_connections_total`
  - `holo_c2_proxy_stalls_total`

- **Agent** (configurable port):
  - `holo_c2_agent_reports_sent_total{agent_id}`
  - `holo_c2_agent_position_{x,y,z}{agent_id}`
  - `holo_c2_agent_points_discovered_total{agent_id}`

### Grafana Dashboard

The included dashboard visualizes:
- System health and agent activity
- Map coverage progression
- Network performance and latency
- Agent positions and discovery rates

## Development

### Code Structure

```
crates/
├── api/                    # Protocol definitions and generated code
│   ├── proto/v1/          
│   └── src/
├── sim_orchestrator/       # Central coordination service
│   └── src/
├── sim_agent/             # Autonomous simulation agents  
│   └── src/
├── holographic-viewer/    # Visualization client
│   └── src/
└── link_emulator/         # Network impairment proxy
    └── src/
```

### Testing

```bash
# Format check
cargo fmt --all -- --check

# Linting  
cargo clippy --workspace --all-targets -- -D warnings

# Build and test
cargo build --workspace --locked
cargo test --workspace --locked
```

### Adding New Features

1. Update the protocol definition in `crates/api/proto/v1/simulation.proto`
2. Implement service logic in the orchestrator
3. Add client-side handling in agents/viewers
4. Update metrics and monitoring as needed
5. Test with network emulation scenarios

## License

MIT License - see LICENSE file for details.

## Citation

This implementation follows the "Project Holographic C2 Protocol Specification v1.3" with normative transport-level defaults and portable data serialization formats.
