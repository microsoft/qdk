// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// See https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html for an overview
// See https://www.w3.org/TR/WGSL/ for the details
// See https://webgpu.github.io/webgpu-samples/ for examples

// Coding guidelines:
// - WGSL generally uses 'camelCase' for vars and functions, 'PascalCase' for types and structs, and 'SCREAMING_SNAKE_CASE' for constants.
// - WGSL doesn't have the ternary operator, but does have a built-in function `select` that can be used to achieve similar functionality. See https://www.w3.org/TR/WGSL/#select-builtin
// - The default workgroup memory size for WebGPU is 16KB, so don't exceed this in total across all workgroup variables.

// The number of qubits being simulated is provided as a specialization constant at pipeline creation time
// See https://gpuweb.github.io/gpuweb/wgsl/#pipeline-overridable
override QUBIT_COUNT: u32;
override OP_COUNT: u32;
override RESULT_COUNT: u32;

// Always use 32 threads per workgroup for max concurrency on most current GPU hardware
const THREADS_PER_WORKGROUP: u32 = 32u;
const MAX_QUBIT_COUNT: u32 = 22u;

// Unform buffer containing per-batch execution parameters
// See https://webgpufundamentals.org/webgpu/lessons/webgpu-uniforms.html
struct UniformParams {
    start_shot_id: u32,
    rng_seed: u32,
}
@group(0) @binding(0)
var<uniform> params: UniformParams;

// Buffer containing the state for each shot to execute per kernel dispatch
// An instance of this is tracked on the GPU for every active shot
struct QubitProbabilities {
    zero: f32,
    one: f32,
}

struct ShotData {
    shot_id: u32,
    next_op_idx: u32,
    // The random number generator will use the shot_id and seed to initialize
    rng_seed: u32,
    // The below random numbers will be initialized from the RNG per operation in the 'prepare_op' stage
    // Then the 'execute_op' stage will read these precomputed random numbers for noise modeling
    rand_pauli: f32,
    rand_damping: f32,
    rand_dephase: f32,
    rand_measure: f32,
    rand_loss: f32,

    // Set to something other than 1.0 if the next 'execute' operation should renormalize the state vector
    renormalize_scale: f32,
    // Total duration of the shot so far, used for time-dependent noise modeling and shot estimations
    duration: f32,
    padding: array<u32, 2>,

    // Track the per-qubit probabilities for optimization of measurement sampling and noise modeling
    qubit_probabilities: array<QubitProbabilities, MAX_QUBIT_COUNT>,

    // NOTE: The below 2 are not yet used, but heating and idle time will be used later for advanced noise modeling
    qubit_heat: array<f32, MAX_QUBIT_COUNT>,
    qubit_idle_since: array<f32, MAX_QUBIT_COUNT>,
}
// struct size = 12 x 4 + 22 x 8 + 22 x 4 + 22 x 4 = 400 bytes (which is multiple of 16 for simple alignment)
// See https://www.w3.org/TR/WGSL/#structure-member-layout for alignment rules

@group(0) @binding(1)
var<storage, read_write> shots: array<ShotData>;

// Buffer containing the list of operations (gates and noise) that make up the program to simulate
struct Op {todo: u32}
@group(0) @binding(2)
var<storage, read> op: array<Op>;

// The one large buffer of state vector amplitudes. (Maybe be partitioned into multiple shots)
@group(0) @binding(3)
var<storage, read_write> stateVector: array<vec2f>;

// Buffer for storing measurement results per shot
@group(0) @binding(4)
var<storage, read_write> results: array<u32>;

// For every qubit, each 'execute' kernel thread will update its own workgroup storage location for accumulating probabilities
// The final probabilities will be reduced and written back to the shot state after the parallel execution completes.
struct QubitProbabilityPerThread {
    zero: array<f32, THREADS_PER_WORKGROUP>,
    one: array<f32, THREADS_PER_WORKGROUP>,
};
var<workgroup> qubitProbabilities: array<QubitProbabilityPerThread, QUBIT_COUNT>;
// Workgroup memory size: QUBIT_COUNT (max 22) * THREADS_PER_WORKGROUP (max 32) * 2 * 4 = max 5632 bytes.

// Always run each workgroup with multiple threads for max concurrency, even though for prepare_ops each thread handles a distinct shot.
@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn prepare_op(@builtin(global_invocation_id) globalId: vec3<u32>) {

}

// Each workgroup dispatched is dedicated to a single shot.
@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(@builtin(global_invocation_id) globalId: vec3<u32>) {
    shots[globalId.x].shot_id = params.start_shot_id + globalId.x;
    shots[globalId.x].next_op_idx = 0u;
    shots[globalId.x].rng_seed = params.rng_seed;

    results[globalId.x] = params.rng_seed;
}
