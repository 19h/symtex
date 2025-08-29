### **Bundle 4 of 4: For the `link_emulator` Team**

**Subject: ACTION REQUIRED: Final Specification for `link_emulator` - Project "Holographic C2"**

This document contains the complete and final specification required for the `link_emulator` team. It includes the overall project charter, the mandatory shared API contracts and standards, and the detailed specification for your component. This is the single source of truth for your development work.

---
---

### **System Design & Project Charter: Project "Holographic C2" (v1.3.1 - Integration Grade with Bootstrap)**

#### **1. Mission Statement & Project Goal**

**1.1. Mission Statement:**
To design, build, and demonstrate a high-fidelity, distributed simulation of an autonomous multi-agent reconnaissance system. The project will showcase a robust, resilient, and observable backend architecture capable of managing a fleet of autonomous agents operating under realistic, impaired network conditions, with a high-performance, data-driven visualization client for situational awareness.

**1.2. Primary Goal:**
The ultimate goal of this project is to serve as a world-class technical portfolio piece, specifically engineered to demonstrate the full spectrum of skills and architectural thinking required for the "Software Engineer - Backend" role at Helsing. Every design decision, technology choice, and implementation detail is motivated by the desire to directly address the qualifications and challenges outlined in the job description, from distributed systems and networking to security and production-ready operational practices.

**1.3. The "Final System State" Vision:**
The completed project will be a fully operational, containerized application suite. An operator will launch the `holographic-viewer`, connect to the backend, and see a hidden, holographic point cloud of a city. By issuing a command, they will witness a swarm of autonomous drone agents spawn, plan their routes, and begin to "scan" the environment. As agents fly through the city, the point cloud will be revealed in real-time on the viewer's display. The operator will be able to view live performance metrics on a Grafana dashboard and observe the system's graceful degradation as network conditions are deliberately impaired via the `link_emulator` and OS-level tools like `tc netem`.

---

#### **2. Core Principles & Architectural Philosophy**

This section outlines the "why" behind the system's design. Understanding these principles is critical for any developer making implementation decisions within their component.

*   **Principle 1: Absolute Separation of Concerns (The "Thin Client" Model):**
    *   **Motivation:** In real-world C2 systems, the operator's console is a window into the state of the world, not the source of it. Our architecture strictly enforces this. The `holographic-viewer` is a "thin" client; it contains no simulation logic. The `sim_orchestrator` is the "fat" backend; it is the sole authority on the state of the world.
    *   **Implication:** A developer on the `viewer` should never be tempted to implement predictive logic or simulate agent behavior. A developer on the `orchestrator` can assume that it is the only component that can mutate the canonical state.

*   **Principle 2: Process Isolation for Resilience:**
    *   **Motivation:** Distributed systems must be resilient to partial failure. A bug in a single drone's navigation software should not crash the entire swarm or the command server.
    *   **Implication:** Each agent (`sim_agent`) runs in its own OS process. This provides maximum isolation. A panic or unrecoverable error in one agent will terminate that process, but the `orchestrator` and all other agents will continue to function. The orchestrator is designed to detect this failure and can potentially re-spawn the agent, demonstrating fault tolerance.

*   **Principle 3: Asynchronous, Non-Blocking Communication:**
    *   **Motivation:** High-performance network services must never be blocked by slow I/O operations. The system must be able to handle many concurrent connections and high-frequency data streams efficiently.
    *   **Implication:** All components MUST be built on an asynchronous runtime (`tokio`). All network communication, and any I/O-bound task, must be `async`. The viewer's render loop, in particular, must be decoupled from its network client to guarantee a fluid user experience.

*   **Principle 4: Testability and Realism via Emulation:**
    *   **Motivation:** Proving that a system is resilient to network failure is impossible without a way to reliably create and repeat those failures.
    *   **Implication:** The `link_emulator` is a non-negotiable, first-class component of the architecture. It allows us to treat network impairment as a configurable feature. All development and testing should be conducted with the emulator in the loop to ensure the system is being built for the real-world conditions it is meant to simulate.

*   **Principle 5: Observability as a First-Class Citizen:**
    *   **Motivation:** You cannot manage, debug, or trust a system you cannot see. In a complex distributed system, understanding its internal state and performance is critical.
    *   **Implication:** Every component MUST expose metrics in the Prometheus format. Logging MUST be structured (JSON). This is not an afterthought; it is a core requirement. The Grafana dashboard is part of the "Definition of Done."

*   **Principle 6: Data-Driven Visualization:**
    *   **Motivation:** The visualization should be a direct representation of the data it receives. This simplifies the rendering logic and ensures fidelity to the canonical state.
    *   **Implication:** The `viewer`'s primary job is to efficiently synchronize its GPU state with the data streamed from the orchestrator. It does not "create" information; it only displays it.

---

#### **3. System Architecture & Component Roles**

The system is composed of four primary, custom-built components and a supporting observability stack, orchestrated via Docker Compose.

*   **`sim_orchestrator` (The C2 Server):** The central authority. It runs the master simulation loop, maintains the ground truth (who has discovered what), and issues high-level tasks to the agent fleet. It is the hub to which all other components connect.

*   **`sim_agent` (The Autonomous Worker):** The "edge device." Each instance is a headless process representing a single drone. It is responsible for its own low-level navigation and perception, reporting its findings back to the orchestrator. It performs the computationally heavy work.

*   **`holographic-viewer` (The Operator Console):** The human interface. It is a high-performance visualization tool that subscribes to the orchestrator's state stream and renders the common operational picture. It is also the mechanism for the human operator to inject commands into the system.

*   **`link_emulator` (The Network Harness):** The "real world" simulator. A transparent TCP proxy that sits between all components, allowing for deterministic injection of latency, bandwidth caps, and stall windows (for packet loss/duplication/reordering, use OS-level `tc netem`).

*   **Observability Stack (`Prometheus` & `Grafana`):** The "health monitor." Prometheus scrapes metrics from all components, and Grafana provides a web-based dashboard to visualize these metrics in real-time.

---

#### **4. The Simulation Lifecycle & Data Flow (End-to-End)**

This narrative describes the flow of data and control through the system from startup to steady-state operation.

1.  **Phase 1: System Ignition:**
    *   The operator runs `docker-compose up`.
    *   The `sim_orchestrator`, `link_emulator`(s), `prometheus`, and `grafana` containers are created and started.
    *   The `orchestrator` loads the point cloud metadata, initializes its empty state, and begins listening for connections on its gRPC and Arrow Flight ports.
    *   The `orchestrator`'s Agent Manager begins spawning `sim_agent` child processes as defined by its configuration.

2.  **Phase 2: Agent Registration:**
    *   A newly spawned `sim_agent` process starts.
    *   It generates a unique session ID and makes a unary `RegisterAgent` gRPC call to the `orchestrator` (through the `link_emulator`).
    *   The `orchestrator` receives the request, generates a new unique `agent_id`, stores the agent in its registry, and returns the `agent_id` to the `sim_agent`.
    *   The `sim_agent` is now a recognized member of the fleet. It immediately establishes its long-lived `ReportState` stream to the orchestrator.

3.  **Phase 3: Viewer Connection & State Synchronization:**
    *   The operator runs `cargo run -p holographic-viewer`.
    *   The `viewer`'s network thread starts and establishes a `SubscribeWorldState` gRPC stream with the `orchestrator` (through the `link_emulator`).
    *   The `orchestrator` immediately sends the current `WorldState` message, which contains the list of currently registered agents and a ticket for the current (empty) reveal mask.
    *   The `viewer`'s network thread receives the ticket and makes an Arrow Flight `DoGet` call to the `orchestrator` to retrieve the reveal mask data.
    *   The complete `WorldStateSnapshot` is assembled and sent to the viewer's render thread.
    *   The render thread updates its GPU buffers. The user now sees the initial scene with agents present but the point cloud hidden.

4.  **Phase 4: The Core Operational Loop (Steady State):**
    *   **Command:** The user clicks `[Start Survey]` in the `viewer` UI. A unary `IssueCommand` gRPC call is sent: `Viewer -> Emulator -> Orchestrator`.
    *   **Tasking:** The `orchestrator` receives the command. Its `tasking` module runs, identifies exploration frontiers, and assigns tasks to idle agents by including them in `ReportStateResponse` messages.
    *   **Action:** The `sim_agent`s, in their control loop, receive their new tasks. They transition from `AwaitingTask` to `Planning` and then `Navigating`.
    *   **Perception & Reporting:** As an agent moves, its `Perceiving` state runs the LiDAR compute shader. The resulting `RoaringBitmap` of discovered points is added to its local buffer. Periodically, the agent sends this buffer in an `AgentReport` message: `Agent -> Emulator -> Orchestrator`.
    *   **Aggregation:** The `orchestrator` receives the report. It merges the agent's discovery bitmap into the master `reveal_mask`, updates the agent's position in its registry, and logs the update.
    *   **Broadcast:** The update to the canonical state triggers the `watch` channel. A new `WorldState` message (with a new ticket) is automatically broadcast to all subscribed `viewer`s: `Orchestrator -> Emulator -> Viewer`.
    *   **Visualization:** The `viewer`'s network thread receives the new `WorldState`, fetches the updated reveal mask via Arrow Flight, and sends the new snapshot to the render thread. The user sees the point cloud appear in the areas the agents have scanned.
    *   **Monitoring:** Throughout this entire loop, `Prometheus` is continuously scraping the `/metrics` endpoints of all components, providing a live view of the system's health and performance in `Grafana`.

---

#### **5. Definitive Technology Stack & Rationale**

*   **Language:** **Rust** (exclusive).
    *   **Rationale:** Performance, memory safety, and excellent concurrency support make it ideal for building high-performance network services and systems-level applications. It directly aligns with the target company's tech stack.
*   **Asynchronous Runtime:** **Tokio**.
    *   **Rationale:** The de-facto standard for asynchronous programming in Rust. It is mature, highly performant, and has a rich ecosystem of libraries.
*   **RPC Framework:** **gRPC** (via `tonic` and `prost`).
    *   **Rationale:** Provides a schema-first, high-performance, and strongly-typed framework for defining service contracts. Protobuf is language-agnostic and efficient. `tonic` is the premier gRPC implementation in the Rust ecosystem.
*   **Bulk Data Transport:** **Apache Arrow Flight**.
    *   **Rationale:** While gRPC is excellent for metadata and commands, it is inefficient for large, columnar datasets like our reveal mask. Arrow Flight is specifically designed for high-throughput transport of Arrow data, avoiding serialization/deserialization overhead. This demonstrates an understanding of choosing the right tool for the job.
*   **Bitmap Data Structure:** **RoaringBitmap** (via `roaring-rs`).
    *   **Rationale:** A highly compressed and efficient bitmap format. It is significantly more memory-efficient than a traditional bitmap for sparse data and supports extremely fast set operations (like the bitwise OR needed for merging discoveries), making it perfect for the reveal mask.
*   **Containerization:** **Docker & Docker Compose**.
    *   **Rationale:** The industry standard for creating reproducible build and runtime environments. Docker Compose allows the entire multi-component backend to be defined and launched with a single command, simplifying development, testing, and demonstration.
*   **Observability:** **Prometheus & Grafana**.
    *   **Rationale:** The industry standard, open-source monitoring stack. Demonstrating proficiency with it shows an understanding of modern operational best practices (DevOps/SRE).

---

#### **6. The "Definition of Done"**

The project is considered complete when all of the following demonstrable capabilities are met:

1.  **[✓] System Orchestration:** The entire backend (orchestrator, agents, emulator, observability) starts cleanly from a single `docker-compose up` command.
2.  **[✓] Agent Lifecycle:** The orchestrator successfully spawns and registers multiple `sim_agent` processes.
3.  **[✓] Client Connectivity:** The `holographic-viewer` can connect to the running backend, subscribe to the state stream, and correctly render the initial scene (agents visible, point cloud hidden).
4.  **[✓] Core Mission Loop:** Clicking `[Start Survey]` in the viewer successfully initiates the autonomous exploration behavior in the agents.
5.  **[✓] Real-Time Discovery:** As agents move, the point cloud is visibly revealed in the viewer in near real-time, with updates driven by the gRPC/Arrow Flight stream.
6.  **[✓] Network Resilience:** The system continues to operate (though potentially degraded) when the `link_emulator` is configured with moderate latency (≥ 100 ms) and stall windows (e.g., 500 ms every 10 s), and/or when OS-level `tc netem` introduces ≥ 5% packet loss at the interface.
7.  **[✓] Full Observability:** The Grafana dashboard is accessible and displays live, meaningful metrics from all running components (e.g., active agents, map coverage %, gRPC request rates).
8.  **[✓] Code Quality:** All code is formatted with `rustfmt`, passes `clippy` with no warnings, and has a logical module structure with clear documentation.

---
---

### **Part 0: Shared Foundation & Project-Wide Standards**

#### **0.1. The `api` Crate**

*   **Purpose:** This crate is the foundational contract for the entire system. It contains all shared data structures and communication protocols, ensuring compile-time consistency across the workspace. It is the first component to be implemented and must be stable before significant work on other components begins.
*   **Contents:**
    1.  **`api/proto/v1/simulation.proto`:** The single source of truth for all gRPC and Protobuf message definitions. The full, final content is provided below.
    2.  **`api/build.rs`:** A build script utilizing `tonic-build` to generate Rust code from the `.proto` file.
    3.  **Generated Code:** The generated `api.v1.rs` file will be committed to the repository. This is a critical step to ensure that developers working on other components do not need to have the `protoc` compiler and its dependencies installed on their local machines.
*   **Developer "Day 1" Action:** All developers MUST pull the repository and successfully run `cargo build -p api` to ensure the shared contracts are available and compile correctly before starting work on their assigned component.
*   **Serialization Standard:** All `RoaringBitmap` data structures transmitted over the network MUST be serialized as Roaring portable bytes per the Roaring Format Specification.
    *   **Rust Crates:**
        *   `roaring` (pure Rust): Use `serialize_into(&mut Vec<u8>)` to produce portable bytes; deserialize with the corresponding crate deserializer.
        *   `croaring`/`croaring-sys`: Use the portable serializers (e.g., `try_serialize_into`) and the matching portable deserializers.

#### **0.2. `api/proto/v1/simulation.proto` (Definitive Contract)**
```protobuf
syntax = "proto3";

package api.v1;

// All linear distances are in metres (SI).
// All coordinate frames are Earth-Centered, Earth-Fixed (ECEF), EPSG:4978.
// All timestamps are Unix epoch milliseconds (UTC).
// All durations are in milliseconds.
// Trace context: All RPCs MUST propagate W3C traceparent/tracestate in gRPC metadata.
// Schema version fields MUST be 1 unless otherwise negotiated.

// A 3D vector in meters, ECEF frame (EPSG:4978).
message Vec3m {
  double x = 1;
  double y = 2;
  double z = 3;
}

// A 3D vector in meters per second, ECEF frame (EPSG:4978).
message Vec3mps {
  double x = 1;
  double y = 2;
  double z = 3;
}

// An orientation represented as a unit-norm quaternion in the ECEF frame (EPSG:4978).
// Invariants: ||q|| = 1 ± 1e-6, w ≥ 0 (canonical hemisphere).
message UnitQuaternion {
  double w = 1;
  double x = 2;
  double y = 3;
  double z = 4;
}

// The operational mode of an agent.
enum AgentMode {
  AWAITING_TASK = 0;
  PLANNING = 1;
  NAVIGATING = 2;
  PERCEIVING = 3;
  DISCONNECTED = 4;
}

// A snapshot of a single agent's state.
message AgentState {
  uint64 agent_id = 1;
  // Time the pose was sampled.
  int64 timestamp_ms = 2;
  Vec3m position_ecef_m = 3;
  Vec3mps velocity_ecef_mps = 4;
  UnitQuaternion orientation_ecef = 5;
  AgentMode mode = 6;
  // A monotonic sequence number, incremented by the agent for each state update.
  // Wraps modulo 2^32; receivers MUST handle unsigned wraparound.
  uint32 sequence = 7;
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}

// A task assigned by the orchestrator to an agent.
message Task {
  Vec3m target_waypoint_ecef_m = 1;
}

// === RegisterAgent RPC ===
message RegisterAgentRequest {
  // A UUIDv4 generated by the agent on startup, unique to its process lifetime.
  string session_id = 1;
  // A freeform string identifying the agent's software version.
  string sw_version = 2;
  // A freeform string identifying the agent's hardware profile.
  string hw_profile = 3;
}
message RegisterAgentResponse {
  // The unique, monotonic ID assigned by the orchestrator for this agent.
  uint64 agent_id = 1;
  // The server's current time, for clock synchronization hints.
  int64 server_time_ms = 2;
  // The desired interval for the agent to send AgentReport messages.
  uint32 report_interval_ms = 3;
  // The maximum size in bytes for a single AgentReport message.
  uint32 max_report_bytes = 4;
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}

// === ReportState RPC ===
message AgentReport {
  uint64 agent_id = 1;
  // Time the report was enqueued for send.
  int64 timestamp_ms = 2;
  // The agent's full state at the time of this report.
  AgentState state = 3;
  // The result of serializing a RoaringBitmap (portable format) for points
  // discovered since the last successful report.
  bytes discovered_point_ids_portable = 4;
}
message ReportStateResponse {
  // The orchestrator can assign a new task to the agent in this response.
  optional Task assigned_task = 1;
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}

// === SubscribeWorldState RPC ===
message SubscribeWorldStateRequest {
  // If true, the server will immediately send the current state upon subscription.
  bool include_initial_snapshot = 1;
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}
message WorldState {
  // Unix timestamp in milliseconds (UTC).
  int64 timestamp_ms = 1;
  repeated AgentState agents = 2;
  // An opaque ticket to be used in an Arrow Flight DoGet call
  // to retrieve the full reveal mask corresponding to this state.
  // This is NOT a UTF-8 string and must be treated as raw bytes.
  bytes reveal_mask_ticket = 3;
  // The ratio of revealed points to total points, from 0.0 to 1.0.
  double map_coverage_ratio = 4;
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}

// === IssueCommand RPC ===
message IssueCommandRequest {
  oneof command {
    StartSurveyCommand start_survey = 1;
    ResetSimulationCommand reset_simulation = 2;
  }
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}
message StartSurveyCommand {}
message ResetSimulationCommand {}

message IssueCommandResponse {
  bool acknowledged = 1;
  string message = 2; // e.g., "Command acknowledged" or an error message.
  // The version of this schema. MUST be 1.
  uint32 schema_version = 255;
}

// The main C2 Service Definition, exposed by the sim_orchestrator.
service SimulationC2 {
  // Called by a sim_agent on startup to join the simulation.
  rpc RegisterAgent(RegisterAgentRequest) returns (RegisterAgentResponse);

  // A long-lived, bidirectional RPC for a sim_agent to report its state
  // and discoveries, and for the orchestrator to send back tasks.
  rpc ReportState(stream AgentReport) returns (stream ReportStateResponse);

  // A long-lived, server-streaming RPC for a viewer to receive updates
  // on the state of the world.
  rpc SubscribeWorldState(SubscribeWorldStateRequest) returns (stream WorldState);

  // A unary RPC for a viewer to send commands to the simulation.
  rpc IssueCommand(IssueCommandRequest) returns (IssueCommandResponse);
}
```

#### **0.3. Project-Wide Logging Standard**

*   **Standard:** All components MUST use a structured logging framework, specifically `tracing`.
*   **Format:** Logs MUST be emitted as structured JSON to `stdout`. This allows for easy ingestion, parsing, and analysis by external logging systems.
*   **Required Fields:** Every log entry MUST, at a minimum, include:
    *   `timestamp`: ISO 8601 format.
    *   `level`: (e.g., `INFO`, `WARN`, `ERROR`).
    *   `message`: The log message.
    *   `component`: The name of the component (e.g., `sim_orchestrator`, `sim_agent`).
    *   `trace_id`, `span_id`: For distributed tracing context.
*   **Contextual Fields:** Developers MUST add relevant contextual fields to log entries (e.g., `agent_id`, `rpc_method`, `error_details`).
*   **Trace Context Propagation:** All gRPC calls MUST propagate W3C Trace Context (`traceparent`, `tracestate`) via gRPC metadata. The `tracing-opentelemetry` crate can be used to automate this.

#### **0.4. Project-Wide gRPC/HTTP2 Defaults (Normative)**

All gRPC servers and clients MUST configure HTTP/2 keep-alives and timeouts as follows:
*   **Servers (applies to `sim_orchestrator`):**
    *   `http2_keepalive_interval` = 30s
    *   `http2_keepalive_timeout` = 20s
    *   Optional: `tcp_keepalive` = 30s (OS-level)
*   **Clients (applies to `sim_agent` and `holographic-viewer`):**
    *   `keep_alive_while_idle` = `true`
    *   `http2_keepalive_interval` = 30s
    *   `http2_keepalive_timeout` = 20s
    *   Per-RPC default deadline (unary): 5s (viewer commands, registration)
    *   Stream liveness grace (bidi/server-stream): application-level watchdog > 90s (3× keepalive_interval) before declaring failure
*   **Implementation Note (Rust/tonic):** Set these on the transport server/client builders.

---
---

### **Component 4: `link_emulator` (The Network Proxy)**

(This section is the full, final specification for the `link_emulator` team.)

#### **4.1. Mission & Stakes**

*   **Mission:** To provide a realistic and configurable network test harness. Its purpose is to make the resilience and robustness of the distributed system a *demonstrable feature* rather than just a theoretical claim.
*   **Stakes:** **Credibility-Critical.** Without this component, the demonstration cannot effectively prove that the system handles intermittent connectivity or low-bandwidth conditions. It is the key that unlocks the ability to showcase the more advanced distributed systems concepts required by the role.

#### **4.2. Role & Motivation (The "Why")**

*   **Background:** Testing distributed systems against real-world network conditions is notoriously difficult. The `link_emulator` provides a simple, deterministic, and repeatable way to simulate these conditions, directly addressing the job description's points on "intermittent connectivity," "byzantine actors" (via packet loss), and "long-range low-bandwidth radios."
*   **Architectural Motivation:**
    *   **Transparent Proxy:** The choice of a transparent TCP proxy is crucial. It requires **zero code changes** in any of the other components. The viewer and agents are simply configured to point at the emulator's address instead of the orchestrator's. This makes it a modular, non-invasive tool that can be easily added or removed from the system for testing.
    *   **Simplicity and Focus:** The emulator focuses only on link-level impairments. It does not attempt to simulate complex network topologies or routing protocols. This keeps the component simple, robust, and focused on its core mission of testing the resilience of the application-level protocols (gRPC).

#### **4.3. Communication Paths & Dependencies**

*   **Ingress (Inputs):**
    1.  **From any client (`viewer`, `agent`):** Accepts any TCP connection on its listening port.
*   **Egress (Outputs):**
    1.  **To any server (`orchestrator`):** Creates a new TCP connection to its configured target address and forwards the data.
*   **Dependencies:**
    *   It is a dependency for any client that wishes to have its connection impaired.
    *   It depends on the target server being available on the network.

#### **4.4. Scope & Implementation Boundaries (In-Scope vs. Out-of-Scope)**

*   **IN-SCOPE:**
    *   Proxying arbitrary TCP traffic.
    *   Injecting latency, jitter, bandwidth caps (token bucket), and stall windows (periodic read/write pauses).
    *   **NOTE:** For true packet loss, duplication, corruption, or reordering, the operator MUST use an OS-level tool like Linux `tc netem`.
*   **OUT-OF-SCOPE (Explicitly Not To Be Implemented):**
    *   Proxying UDP traffic.
    *   Simulating network partitions between specific clients (all clients get the same impairment).
    *   Simulating DNS resolution issues or higher-level network phenomena.
    *   A dynamic configuration API. Configuration is set once at startup.

#### **4.5. Tooling & Key Dependencies**

*   **`tokio`:** For all asynchronous I/O operations.
*   **`rand`:** For simulating jitter and scheduling randomized stall windows.
*   **`tracing` & `tracing-subscriber`:** For structured, JSON-formatted logging.

#### **4.6. Lifecycle & State Management**

*   **Lifecycle:** The emulator is stateless. It starts, listens, proxies connections, and shuts down. It does not manage any state between connections.
*   **Multi-Instance Deployment Strategy:**
    *   The system architecture requires proxying multiple ports (gRPC and Arrow Flight). The `docker-compose.yml` file will be the definitive source for this configuration. It will define multiple `link_emulator` services, each with a unique name and configuration.
    *   **Example `docker-compose.yml` snippet:**
        ```yaml
        services:
          # ... orchestrator definition ...

          link-emulator-grpc:
            image: link-emulator:latest # Assumes a Docker image is built
            environment:
              - EMULATOR_LISTEN_ADDR=0.0.0.0:60051
              - EMULATOR_TARGET_ADDR=sim_orchestrator:50051
              - EMULATOR_LATENCY_MS=50
              # ... other impairment settings ...
            ports:
              - "60051:60051"

          link-emulator-flight:
            image: link-emulator:latest
            environment:
              - EMULATOR_LISTEN_ADDR=0.0.0.0:60052
              - EMULATOR_TARGET_ADDR=sim_orchestrator:50052
              - EMULATOR_RATE_BPS=131072
              - EMULATOR_BUCKET_BYTES=32768
              # ... other impairment settings ...
            ports:
              - "60052:60052"
        ```
    *   The `holographic-viewer` and `sim_agent` will be configured to connect to the emulator's ports (`60051`, `60052`), not the orchestrator's direct ports.

#### **4.7. Definitive External Contracts**

*   **gRPC API:** None. It is gRPC-agnostic and operates at the TCP layer.

*   **Metrics Exposed (Prometheus format on `/metrics`):**
    *   `proxy_active_connections`: `Gauge` - Number of currently active proxied connections.
    *   `proxy_bytes_transferred_total{direction}`: `Counter` - Total bytes transferred, labeled by direction (`client_to_server` or `server_to_client`).
    *   `proxy_stall_windows_total`: `Counter` - Total number of injected stall windows.
    *   `proxy_resets_injected_total`: `Counter` - Total number of injected connection resets.
    *   **Cardinality:** Per-agent series MUST be deleted on deregistration; follow `_total`/`_seconds`/`_bytes` conventions.

*   **Configuration (Environment Variables):**
    *   `EMULATOR_LISTEN_ADDR`: **Required.** e.g., `0.0.0.0:60051`
    *   `EMULATOR_TARGET_ADDR`: **Required.** e.g., `sim_orchestrator:50051`
    *   `EMULATOR_METRICS_LISTEN_ADDR`: **Required.** e.g., `0.0.0.0:9099`
    *   `EMULATOR_LATENCY_MS`: Default `0`.
    *   `EMULATOR_JITTER_MS`: Default `0`.
    *   `EMULATOR_RATE_BPS`: Default `0` (unlimited).
    *   `EMULATOR_BUCKET_BYTES`: Default `0` (unlimited).
    *   `EMULATOR_STALL_PERIOD_MS`: Default `0` (disabled).
    *   `EMULATOR_STALL_DURATION_MS`: Default `0` (disabled).
