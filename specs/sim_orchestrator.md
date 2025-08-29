### **Bundle 1 of 4: For the `sim_orchestrator` Team**

**Subject: ACTION REQUIRED: Final Specification for `sim_orchestrator` - Project "Holographic C2"**

This document contains the complete and final specification required for the `sim_orchestrator` team. It includes the overall project charter, the mandatory shared API contracts and standards, and the detailed specification for your component. This is the single source of truth for your development work.

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

### **Component 1: `sim_orchestrator` (The Backend C2 Server)**

(This section is the full, final specification for the `sim_orchestrator` team.)

#### **1.1. Mission & Stakes**

*   **Mission:** To serve as the single, authoritative source of truth for the entire distributed simulation. It is the "brain" of the operation, responsible for strategic command and control (C2), state management, and system integrity. Its successful implementation demonstrates the ability to build robust, scalable, and stateful backend services—a core requirement for the target role.
*   **Stakes:** **System-Critical.** Failure or poor performance of the orchestrator results in a total failure of the simulation. A bug in its state management can lead to data corruption or desynchronization across the entire system. Its stability and correctness are paramount.

#### **1.2. Role & Motivation (The "Why")**

*   **Background:** In any real-world multi-agent robotic system, a central C2 node is essential for mission planning, deconfliction, and maintaining a common operational picture (COP). The `sim_orchestrator` emulates this role. It is the digital equivalent of a command post.
*   **Architectural Motivation:**
    *   **Centralized Authority:** A distributed consensus model for world state is vastly more complex and unnecessary for this project's scope. A single, authoritative orchestrator simplifies the architecture, eliminates the possibility of conflicting world states (e.g., two agents claiming to have discovered the same point first), and provides a clear point of control and observation. This is a pragmatic design choice that prioritizes correctness and feasibility.
    *   **Decoupling State from Workers:** The agents (`sim_agent`) are treated as ephemeral, untrusted workers. By keeping the canonical state in the orchestrator, the system becomes resilient to agent crashes. A new agent can be spawned, receive the current state of the world from the orchestrator, and seamlessly resume the mission. This directly addresses the need for reliable and fault-tolerant systems.
    *   **Scalability:** The architecture is designed to be scalable. The orchestrator's core logic (merging bitmaps, updating maps) is lightweight, allowing it to manage a large number of agents. The heavy lifting (perception) is offloaded to the agents themselves.

#### **1.3. Communication Paths & Dependencies**

*   **Ingress (Inputs):**
    1.  **From `holographic-viewer`:** Receives high-level user commands (e.g., `StartSurveyArea`) via unary gRPC calls.
    2.  **From `holographic-viewer`:** Accepts long-lived gRPC connections for `SubscribeWorldState` streams.
    3.  **From `sim_agent`:** Accepts gRPC `RegisterAgent` calls.
    4.  **From `sim_agent`:** Accepts long-lived, bidirectional gRPC connections for `ReportState`.
    5.  **From `Prometheus`:** Responds to HTTP scrape requests on its `/metrics` endpoint.
*   **Egress (Outputs):**
    1.  **To `holographic-viewer`:** Streams `WorldState` messages containing agent positions and Arrow Flight tickets.
    2.  **To `holographic-viewer`:** Streams serialized `RoaringBitmap` data via the Arrow Flight `DoGet` endpoint.
    3.  **To `sim_agent`:** Spawns agent processes and passes configuration via command-line arguments.
    4.  **To `sim_agent`:** Sends tasking commands via the `ReportStateResponse` message on the bidirectional stream.
*   **Dependencies:**
    *   **Runtime:** Requires the `sim_agent` binary to be accessible at the configured path.
    *   **Data:** Requires access to the `.hypc` point cloud file(s) to load metadata on startup.
    *   **Network:** Relies on all clients (`viewer`, `agent`) being able to establish a network connection to its listening ports.

#### **1.4. Scope & Implementation Boundaries (In-Scope vs. Out-of-Scope)**

*   **IN-SCOPE:**
    *   Managing the lifecycle of agents it spawns itself.
    *   Maintaining a single, global `RoaringBitmap` for the reveal mask.
    *   Implementing a simple, greedy task allocation algorithm (e.g., assign idle agent to nearest frontier).
    *   Broadcasting the *entire* state to all viewers.
    *   Providing a robust, well-defined gRPC and Arrow Flight API.
*   **OUT-OF-SCOPE (Explicitly Not To Be Implemented):**
    *   Complex, academic task allocation algorithms like CBBA, unless time permits as a stretch goal. The focus is on the *architecture* that enables tasking, not the algorithm itself.
    *   Persistence. The simulation state is entirely in-memory and is lost on restart.
    *   Dynamic agent registration from arbitrary, non-spawned processes (for security and simplicity).
    *   Area-of-interest (AOI) filtering for state updates. All viewers get the same global state.
    *   Authentication or authorization on the API endpoints.

#### **1.5. Tooling & Key Dependencies**

*   **`tokio`:** The core asynchronous runtime.
*   **`tonic` & `prost`:** For the gRPC server implementation.
*   **`arrow-flight` & `arrow-rs`:** For the high-performance data streaming service.
*   **`roaring-rs`:** For the core `RoaringBitmap` data structure.
*   **`dashmap`:** For concurrent, fine-grained access to the agent registry.
*   **`prometheus` (crate):** For exposing metrics in the required format.
*   **`tracing` & `tracing-subscriber`:** For structured, JSON-formatted logging.

#### **1.6. Lifecycle & State Management**

*   **Startup Sequence:**
    1.  Read configuration from environment variables.
    2.  Initialize `tracing` with a JSON formatter to `stdout`.
    3.  Load point cloud metadata (`.hypc` header) to get total point count and other global parameters.
    4.  Instantiate the `CanonicalState` object, initializing the `reveal_mask` `RoaringBitmap` to be empty.
    5.  Spawn the Agent Manager task.
    6.  Spawn the gRPC server task, which begins listening on the configured gRPC port.
    7.  Spawn the Arrow Flight server task, which begins listening on its configured port.
    8.  Spawn the Prometheus metrics server task.
    9.  The orchestrator is now fully operational.
*   **Shutdown Sequence:**
    1.  Upon receiving a `SIGINT` or `SIGTERM` signal, begin graceful shutdown.
    2.  The gRPC server stops accepting new connections but allows existing streams to finish for a short grace period (e.g., 2 seconds).
    3.  For graceful shutdown, the Agent Manager will use `nix::sys::signal::kill` to send a `SIGTERM` signal to each child process, wait for a 2-second grace period, and then issue a `SIGKILL` via `Child::start_kill()` to any remaining processes.
    4.  All Tokio tasks are gracefully aborted.
    5.  The process exits.
*   **Concurrency & State Model:**
    *   The core of the application is a single `Arc<RwLock<CanonicalState>>` instance, shared across all asynchronous tasks.
    *   **Agent Registry (`agents`):** A `DashMap<u64, AgentRuntimeInfo>` is used for fine-grained, high-performance concurrent access to individual agent records without locking the entire map.
    *   **Reveal Mask (`reveal_mask`):** A `RoaringBitmap` wrapped in a `RwLock`. Writes are expected to be frequent but fast (merging small bitmaps from agents). Reads happen only when a client requests a new Arrow Flight snapshot.
    *   **State Broadcasting (`world_state_tx`):** A `tokio::sync::watch` channel is used for state broadcasting. This is a single-producer, multi-consumer channel ideal for "latest value" state distribution. The gRPC `SubscribeWorldState` task holds the single `Sender`. Each new subscriber receives a `Receiver` clone. This avoids complex fan-out logic.

#### **1.7. Internal Module Contracts**

*   **`state.rs` Data Models:**
    ```rust
    use roaring::RoaringBitmap;
    use dashmap::DashMap;
    use std::time::Instant;
    use tokio::sync::{watch, RwLock};
    use std::sync::atomic::{AtomicU64, Ordering};
    use crate::api::v1 as pb; // Protobuf generated types

    // The single source of truth, wrapped in Arc<RwLock<...>>
    pub struct CanonicalState {
        pub agents: DashMap<u64, AgentRuntimeInfo>,
        pub reveal_mask: RwLock<RoaringBitmap>,
        pub point_cloud_metadata: PointCloudMetadata,
        pub world_state_tx: watch::Sender<WorldStateSnapshot>,
        next_agent_id: AtomicU64,
        // A mapping of valid tickets to their corresponding data snapshots.
        pub valid_flight_tickets: RwLock<HashMap<Vec<u8>, Arc<RoaringBitmap>>>,
    }

    pub struct AgentRuntimeInfo {
        pub last_seen: Instant,
        pub current_state: pb::AgentState,
        pub process_handle: Option<tokio::process::Child>,
    }

    #[derive(Clone)]
    pub struct WorldStateSnapshot {
        pub timestamp_ms: i64,
        pub agents: Vec<pb::AgentState>,
        pub reveal_mask_flight_ticket: Vec<u8>,
    }

    pub struct PointCloudMetadata {
        pub total_points: u64,
    }
    ```
*   **`tasking.rs` Module Interface:**
    ```rust
    use std::collections::HashMap;
    use crate::state::{CanonicalState};
    use crate::api::v1 as pb;

    // This function is non-blocking and reads the current world state to produce new tasks.
    pub fn allocate_tasks(state: &CanonicalState) -> HashMap<u64, pb::Task>;
    ```

#### **1.8. Definitive External Contracts**

*   **gRPC API (Provides `api.v1.SimulationC2` service):**
    *   **`rpc RegisterAgent(RegisterAgentRequest) returns (RegisterAgentResponse)`:**
        *   **Pre-conditions:** The `RegisterAgentRequest` MUST contain a `session_id` (e.g., a UUID generated by the agent) that is unique for that agent's process lifetime.
        *   **Processing Logic:**
            1.  Generate a new, unique, monotonic `agent_id` from the `next_agent_id` atomic counter.
            2.  Create a new `AgentRuntimeInfo` struct.
            3.  Insert the new agent info into the `agents` DashMap using the `agent_id` as the key.
            4.  Log the registration event with both `session_id` and the newly assigned `agent_id`.
        *   **Post-conditions (Success):** The orchestrator MUST return a `RegisterAgentResponse` containing the newly assigned `agent_id`.
        *   **Error Handling:** If the request is malformed, return `INVALID_ARGUMENT`. If the orchestrator is in a shutdown state, return `UNAVAILABLE`.
    *   **`rpc ReportState(stream AgentReport) returns (stream ReportStateResponse)`:**
        *   **Pre-conditions:** The client MUST be a registered agent. The `agent_id` in each `AgentReport` message MUST be valid. The stream MUST remain open for the agent's lifetime.
        *   **Processing Logic (per message):**
            1.  Acquire a write lock on the agent's entry in the `agents` DashMap.
            2.  Update `last_seen` to `Instant::now()`.
            3.  Update `current_state` with the state from the report.
            4.  Acquire a write lock on the global `reveal_mask`.
            5.  Deserialize a temporary `RoaringBitmap` from the `discovered_point_ids_portable` field in the report.
            6.  Perform a bitwise OR operation: `global_mask |= local_mask`.
            7.  Release all locks.
            8.  Trigger a broadcast of the new `WorldStateSnapshot` via the `world_state_tx` watch channel.
            9.  Call `tasking::allocate_tasks` to determine if a new task should be assigned to this agent.
            10. Send a `ReportStateResponse` back to the agent, including the `assigned_task` if one was generated.
        *   **Post-conditions (Success):** The orchestrator's canonical state is updated. A state broadcast is triggered. Upon stream termination (client disconnects), the orchestrator MUST mark the agent as "disconnected" and eventually remove it via the health-check task.
        *   **Error Handling:** If an `agent_id` is received that is not in the registry, return `NOT_FOUND` and terminate the stream. The server enforces protobuf decoding limits; exceeding `max_report_bytes` yields `RESOURCE_EXHAUSTED`.
    *   **`rpc SubscribeWorldState(SubscribeWorldStateRequest) returns (stream WorldState)`:**
        *   **Contract:** A viewer connects to this endpoint to receive state updates. The orchestrator MUST immediately send the current state and then continue to send updates whenever the world state changes. The `WorldState` message contains an Arrow Flight ticket (`reveal_mask_flight_ticket`) which the client MUST use to fetch the full reveal mask.
    *   **`rpc IssueCommand(IssueCommandRequest) returns (IssueCommandResponse)`:**
        *   **Contract:** The orchestrator will process the command (e.g., trigger the tasking module for all agents on `StartSurvey`, or clear all state on `ResetSimulation`) and return an acknowledgement.

*   **Arrow Flight Service (Provides):**
    *   **`DoGet(Ticket)`:**
        *   **Pre-conditions:** The `Ticket` payload MUST be the exact opaque bytes previously issued by the orchestrator in a `WorldState` message.
        *   **Ticket Lifecycle:** On each update to the canonical `reveal_mask`, the orchestrator will generate a new UUIDv4 and use its 16 raw bytes as the `reveal_mask_flight_ticket`. The orchestrator will maintain a mapping of this ticket to a snapshot (`Arc<RoaringBitmap>`) of the reveal mask at that moment in time. Tickets are valid for a short period (e.g., 10 seconds) and the orchestrator will maintain a maximum of 256 live tickets.
        *   **Arrow Schema Contract:** The returned Arrow stream MUST adhere to the following schema: `Field::new("roaring_portable", DataType::LargeBinary, false).with_metadata(HashMap::from([("content_type".into(), "application/x-roaring".into()), ("version".into(), "1".into())]))`
        *   **Serialization Contract:**
            1.  Look up the ticket in the `valid_flight_tickets` map.
            2.  Serialize the corresponding `RoaringBitmap` snapshot to a `Vec<u8>` as Roaring portable bytes per the Roaring Format Specification. In Rust (`roaring` crate), call `serialize_into(&mut Vec<u8>)`.
            3.  Create an Arrow `LargeBinaryArray` from this vector.
            4.  Create a `RecordBatch` containing this single array.
            5.  Send this `RecordBatch` to the client.
        *   **Error Handling:** If the ticket is invalid or expired, the stream MUST be closed with an `INVALID_ARGUMENT` error.

*   **Metrics Exposed (Prometheus format on `/metrics`):**
    *   `sim_agents_registered_total`: `Counter` - Total number of agents that have ever registered.
    *   `sim_agents_active`: `Gauge` - Current number of agents considered active and healthy.
    *   `sim_points_revealed_total`: `Counter` - Total number of unique points revealed across all agents.
    *   `sim_map_coverage_ratio`: `Gauge` - The ratio of `points_revealed / total_points`.
    *   `sim_grpc_requests_total{rpc_method, status}`: `Counter` - Total number of gRPC requests, labeled by method and status.
    *   **Cardinality:** Per-agent series MUST be deleted on deregistration; follow `_total`/`_seconds`/`_bytes` conventions.

*   **Configuration (Environment Variables):**
    *   `ORCHESTRATOR_GRPC_LISTEN_ADDR`: e.g., `0.0.0.0:50051`
    *   `ORCHESTRATOR_FLIGHT_LISTEN_ADDR`: e.g., `0.0.0.0:50052`
    *   `ORCHESTRATOR_METRICS_LISTEN_ADDR`: e.g., `0.0.0.0:9091`
    *   `ORCHESTRATOR_PUBLIC_GRPC_ADDR`: The address agents should connect to, e.g., `link-emulator-grpc:60051`.
    *   `AGENT_BINARY_PATH`: Path to the `sim_agent` executable to spawn.
    *   `POINT_CLOUD_PATH`: Path to the `.hypc` file(s) for metadata loading.
    *   `AGENT_HEALTH_TIMEOUT_MS`: Timeout for considering an agent stale.
    *   `AGENT_METRICS_PORT_RANGE_START`: The starting port for assigning to agents, e.g., `9100`.

***
