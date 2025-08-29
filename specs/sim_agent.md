IMPLEMENTED

### **Bundle 2 of 4: For the `sim_agent` Team**

**Subject: ACTION REQUIRED: Final Specification for `sim_agent` - Project "Holographic C2"**

This document contains the complete and final specification required for the `sim_agent` team. It includes the overall project charter, the mandatory shared API contracts and standards, and the detailed specification for your component. This is the single source of truth for your development work.

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

### **Component 2: `sim_agent` (The Worker Process)**

(This section is the full, final specification for the `sim_agent` team.)

#### **2.1. Mission & Stakes**

*   **Mission:** To function as an autonomous, embodied agent operating at the edge. It is the "hands and eyes" of the system, responsible for local perception and navigation. Its successful implementation demonstrates an understanding of edge computing constraints, autonomy, and the ability to build efficient, resource-aware applications.
*   **Stakes:** **Mission-Capability-Critical.** The failure of a single agent results in a partial degradation of the system's mission capability (e.g., slower map discovery). A systemic failure across all agents renders the system useless. Its performance directly impacts the overall speed and efficiency of the simulation.

#### **2.2. Role & Motivation (The "Why")**

*   **Background:** This component models a real-world unmanned aerial vehicle (UAV) or similar robotic platform. These platforms have their own on-board compute (often resource-constrained), run their own software stack, and communicate over unreliable links back to a central controller.
*   **Architectural Motivation:**
    *   **Process Isolation:** Running each agent in its own process is a deliberate choice that mirrors reality. It ensures that a crash in one agent's code (e.g., a panic in its planning logic) does not affect the orchestrator or any other agent. This builds a highly resilient and fault-tolerant system.
    *   **Computational Offload:** The most computationally expensive task is perception (simulated LiDAR). By placing this logic within the agent, we offload this work from the central server, allowing the orchestrator to focus on lightweight state management and scale to a much larger number of agents.
    *   **Headless GPU Compute:** The agent's use of a headless `wgpu` context is a key feature. It demonstrates the ability to leverage GPU acceleration for non-graphical tasks (GPGPU), a critical capability in modern AI and robotics systems, especially for deployment on servers or embedded devices without a display.

#### **2.3. Communication Paths & Dependencies**

*   **Ingress (Inputs):**
    1.  **From `sim_orchestrator`:** Receives its configuration (ID, etc.) via command-line arguments at launch.
    2.  **From `sim_orchestrator`:** Receives tasks via the `ReportStateResponse` message on the bidirectional stream.
*   **Egress (Outputs):**
    1.  **To `sim_orchestrator`:** Sends a unary `RegisterAgent` gRPC call on startup.
    2.  **To `sim_orchestrator`:** Maintains a continuous bidirectional `ReportState` gRPC connection to report its status and discoveries.
    3.  **To `Prometheus`:** Responds to HTTP scrape requests on its `/metrics` endpoint.
*   **Dependencies:**
    *   **Runtime:** Depends on the `sim_orchestrator` being available on the network to register and report state.
    *   **Data:** Requires access to the same `.hypc` point cloud file as the orchestrator to run its perception simulation.

#### **2.4. Scope & Implementation Boundaries (In-Scope vs. Out-of-Scope)**

*   **IN-SCOPE:**
    *   Implementing the full state machine (AwaitingTask, Planning, Navigating, Perceiving).
    *   Using `wgpu` in a headless mode to run a compute shader for perception.
    *   Simple kinematic physics (updating position based on velocity and time delta).
    *   Buffering discoveries locally and reporting them periodically.
    *   Implementing a robust reconnect strategy for the gRPC connection to the orchestrator.
*   **OUT-OF-SCOPE (Explicitly Not To Be Implemented):**
    *   Complex physics simulation (e.g., aerodynamics, collision physics). The agent is a point mass that moves through space.
    *   A complex sensor model. The LiDAR is perfect: no noise, no dropouts, just pure geometric intersection.
    *   Inter-agent communication (peer-to-peer). All communication is strictly hierarchical through the orchestrator.
    *   On-board persistence of its state.

#### **2.5. Tooling & Key Dependencies**

*   **`tokio`:** For the main control loop and communication tasks.
*   **`wgpu`:** For headless GPU compute.
*   **`tonic` & `prost`:** For the gRPC client implementation.
*   **`roaring-rs`:** For managing the local discovery buffer.
*   **`nalgebra`:** (Recommended) For handling 3D transformations, poses, and physics calculations cleanly.
*   **`uuid`:** For generating the initial `session_id`.
*   **`tracing` & `tracing-subscriber`:** For structured, JSON-formatted logging.

#### **2.6. Lifecycle & State Management**

*   **Startup & Registration Sequence:**
    1.  **Startup:**
        1.  Parse command-line arguments (`--orchestrator-grpc-addr`, etc.).
        2.  Generate a unique `session_id` (UUID v4).
        3.  Initialize headless `wgpu` context. This MUST be done by requesting an adapter with `force_fallback_adapter: true` if no physical GPU is present, ensuring it can run on servers.
        4.  Load the entire point cloud into a read-only `wgpu::Buffer` on the selected `wgpu` device. The path to the point cloud data MUST be provided via configuration.
        5.  Create the perception subsystem (compute pipeline).
        6.  Connect to the orchestrator's gRPC service.
    2.  **Registration:**
        1.  Call the `RegisterAgent` RPC, sending its `session_id`.
        2.  Await the response. Store the received `agent_id`. This ID MUST be used in all subsequent communications.
        3.  If registration fails, log the error and exit with a non-zero status code.
    4.  **Operation:**
        1.  Establish the bidirectional `ReportState` RPC connection.
        2.  Begin the main Agent Control Loop.
*   **Shutdown Sequence:**
    1.  Upon receiving a `SIGINT`/`SIGTERM`, the handler will set a flag to initiate graceful shutdown.
    2.  The main loop will finish its current iteration, gracefully close the `ReportState` stream, and exit the process.
*   **Agent Control Loop (State Machine):**
    *   The agent's main loop MUST implement the following state machine:
    *   **`State::AwaitingTask`**: The agent is idle. It continues to send periodic "heartbeat" `AgentReport` messages with its current position but an empty `discovered_point_ids` list. **Transition:** Moves to `State::Planning` when a new task is received in a `ReportStateResponse`.
    *   **`State::Planning`**: The agent's navigation module computes a path or series of waypoints to execute the current task. **Transition:** Moves to `State::Navigating` upon successful plan generation. Moves to `State::AwaitingTask` if planning fails.
    *   **`State::Navigating`**: The agent updates its physics to move along the planned path. At each time step, it transitions to `State::Perceiving`. **Transition:** Moves to `State::AwaitingTask` when the destination is reached.
    *   **`State::Perceiving`**: The perception subsystem is invoked to run a simulated LiDAR scan from the agent's current pose. The resulting `RoaringBitmap` of newly discovered points is merged into the agent's local `discovery_buffer`. **Transition:** Always transitions back to `State::Navigating` to continue movement.

#### **2.7. Internal Module Contracts**

*   **`perception.rs` Subsystem:**
    *   **Contract:** MUST provide a function `run_lidar_scan(&self, pose: Isometry3<f64>) -> RoaringBitmap`. This function is responsible for setting the compute shader's uniform buffer with the current pose, dispatching the compute job, and reading the results back from the GPU into a CPU-side `RoaringBitmap`. The result MUST only contain points discovered in this specific scan.
*   **`communication.rs` Subsystem:**
    *   **Contract:** MUST manage the `ReportState` stream in a background Tokio task. It will receive `AgentReport` data from the main control loop via an `async` channel (e.g., `tokio::sync::mpsc`). It MUST handle connection drops and implement an exponential backoff retry strategy to re-establish the stream. It MUST NOT block the main control loop. It MUST check each `ReportStateResponse` for an `assigned_task` and forward it to the main control loop. The agent MUST concurrently read `ReportStateResponse` while writing `AgentReport` on the same bidirectional stream (e.g., via two dedicated tasks sharing the stream handle).

#### **2.8. Definitive External Contracts**

*   **gRPC API (Consumes `api.v1.SimulationC2` service):**
    *   **`RegisterAgent`:**
        *   **Contract:** MUST be called exactly once on startup. The agent MUST use the `agent_id` returned by the orchestrator for all subsequent communication. If this call fails, the agent MUST exit with an error.
    *   **`ReportState`:**
        *   **Contract:** The agent MUST establish and maintain this stream for its entire lifetime. It MUST send `AgentReport` messages at a regular interval. The `discovered_point_ids_portable` field MUST only contain points discovered since the last report. Upon successful transmission, the agent MUST clear its local `discovery_buffer`.

*   **Metrics Exposed (Prometheus format on `/metrics`):**
    *   `agent_planning_loop_duration_seconds{agent_id}`: `Gauge` - Duration of the last planning loop in seconds.
    *   `agent_points_discovered_per_report{agent_id}`: `Gauge` - Number of points in the last discovery report.
    *   `agent_grpc_connection_status{agent_id}`: `Gauge` - `1` for connected, `0` for disconnected.
    *   **Cardinality:** Per-agent series MUST be deleted on deregistration; follow `_total`/`_seconds`/`_bytes` conventions.

*   **Configuration (Command-Line Arguments):**
    *   `--orchestrator-grpc-addr <ADDR>`: **Required.** Address of the orchestrator.
    *   `--metrics-listen-addr <ADDR>`: **Required.** Address for its own metrics server.
    *   `--point-cloud-path <PATH>`: **Required.** Filesystem path to the `.hypc` file to load.

***
