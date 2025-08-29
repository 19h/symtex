// Uniform buffer holding the agent's current state for the scan.
// This data is read-only and consistent across all shader invocations.
@group(0) @binding(0)
var<uniform> agent_pose: AgentPose;

// Input buffer: The entire world's point cloud data.
// Read-only from the shader's perspective.
// Points are stored as vec4<f32> for 16-byte alignment, even though we only use xyz.
@group(0) @binding(1)
var<storage, read> point_cloud: array<vec4<f32>>;

// Output buffer: Will be filled with the indices of discovered points.
// The `count` is an atomic counter to safely track the number of discovered points.
// The `indices` array is runtime-sized.
@group(0) @binding(2)
var<storage, read_write> discovered_points: DiscoveredPoints;

// --- Struct Definitions ---

// Corresponds to the AgentPose uniform buffer object on the CPU side.
struct AgentPose {
    // Agent's current position in ECEF meters.
    position: vec3<f32>,
    // The squared scan range in meters^2. Squaring is done on the CPU
    // to avoid a sqrt() operation per-point in the shader.
    scan_range_sq: f32,
};

// Corresponds to the output buffer on the CPU side.
struct DiscoveredPoints {
    // Atomically incremented counter for the number of points found.
    count: atomic<u32>,
    // Array to store the indices of the points that are within range.
    indices: array<u32>,
};

// --- Compute Shader ---

// The entry point for the compute shader.
// We process 256 points per workgroup, a common size for good performance.
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let point_index = global_id.x;
    let num_points = arrayLength(&point_cloud);

    // Boundary check to ensure we don't read past the end of the buffer.
    // This is important as the number of dispatched workgroups might not be a
    // perfect multiple of the number of points.
    if (point_index >= num_points) {
        return;
    }

    // Get the position of the point this shader invocation is responsible for.
    let point_position = point_cloud[point_index].xyz;

    // Calculate the vector from the agent to the point.
    let offset = point_position - agent_pose.position;

    // Calculate squared distance. This is much faster than calculating the
    // actual distance as it avoids a square root operation.
    let distance_sq = dot(offset, offset);

    // If the point is within the scan radius...
    if (distance_sq <= agent_pose.scan_range_sq) {
        // ...atomically increment the discovery counter and get the index
        // at which to store our result. This prevents race conditions.
        let storage_index = atomicAdd(&discovered_points.count, 1u);

        // Store the index of the discovered point in the output buffer.
        discovered_points.indices[storage_index] = point_index;
    }
}
