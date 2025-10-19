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
// override OP_COUNT: u32;
// override RESULT_COUNT: u32;

// Always use 32 threads per workgroup for max concurrency on most current GPU hardware
const THREADS_PER_WORKGROUP: u32 = 32u;
const MAX_QUBIT_COUNT: u32 = 22u;

// Operation IDs
const OPID_ID      = 0u;
const OPID_RESET   = 1u;
const OPID_X       = 2u;
const OPID_Y       = 3u;
const OPID_Z       = 4u;
const OPID_H       = 5u;
const OPID_S       = 6u;
const OPID_SAJD    = 7u;
const OPID_T       = 8u;
const OPID_TAJD    = 9u;
const OPID_SX      = 10u;
const OPID_SXAJD   = 11u;
const OPID_RX      = 12u;
const OPID_RY      = 13u;
const OPID_RZ      = 14u;
const OPID_CX      = 15u;
const OPID_CZ      = 16u;
const OPID_RXX     = 17u;
const OPID_RYY     = 18u;
const OPID_RZZ     = 19u;
const OPID_CCX     = 20u;
const OPID_MZ      = 21u;
const OPID_MRESETZ = 22u;
const OPID_MEVERYZ = 23u;
const OPID_SWAP    = 24u;
const OPID_MAT1Q   = 25u;
const OPID_MAT2Q   = 26u;
const OPID_SAMPLE  = 27u;

const ALL_QUBITS: u32 = 0xFFFFFFFFu;

// Unform buffer containing per-batch execution parameters
// See https://webgpufundamentals.org/webgpu/lessons/webgpu-uniforms.html
struct UniformParams {
    start_shot_id: u32,
}
@group(0) @binding(0)
var<uniform> params: UniformParams;

// Buffer containing the state for each shot to execute per kernel dispatch
// An instance of this is tracked on the GPU for every active shot
struct QubitProbabilities {
    zero: f32,
    one: f32,
}

struct xorwow_state {
    counter: u32,
    x: array<u32, 5>
}

struct ShotData {
    shot_id: u32,
    next_op_idx: u32,
    // The below random numbers will be initialized from the RNG per operation in the 'prepare_op' stage
    // Then the 'execute_op' stage will read these precomputed random numbers for noise modeling
    rng_state: xorwow_state,
    rand_pauli: f32,
    rand_damping: f32,
    rand_dephase: f32,
    rand_measure: f32,
    rand_loss: f32,

    // Set to something other than 1.0 if the next 'execute' operation should renormalize the state vector
    renormalize_scale: f32,
    // Total duration of the shot so far, used for time-dependent noise modeling and shot estimations
    duration: f32,
    padding: array<u32, 1>,

    // Track the per-qubit probabilities for optimization of measurement sampling and noise modeling
    qubit_probabilities: array<QubitProbabilities, MAX_QUBIT_COUNT>,

    // NOTE: The below 2 are not yet used, but heating and idle time will be used later for advanced noise modeling
    qubit_heat: array<f32, MAX_QUBIT_COUNT>,
    qubit_idle_since: array<f32, MAX_QUBIT_COUNT>,
}
// struct size = 16 x 4 + 22 x 8 + 22 x 4 + 22 x 4 = 416 bytes (which is multiple of 16 for simple alignment)
// See https://www.w3.org/TR/WGSL/#structure-member-layout for alignment rules

@group(0) @binding(1)
var<storage, read_write> shots: array<ShotData>;

// Buffer containing the list of operations (gates and noise) that make up the program to simulate
struct Op {
    id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    rzr: f32,
    rzi: f32,
    unitary: array<vec2f, 16>,
    angle: f32,
    padding: array<u32, 25>,
} // size: 6 * 4 + 16 * 8 + 4 + 100 = 256 bytes
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
    // Get a reference to the shot and op to prepare
    let shot_buffer_idx = globalId.x;
    let shot = &shots[shot_buffer_idx];
    // WebGPU guarantees that buffers are zero-initialized, so next_op_idx will be 0 on first run
    let op_idx = shot.next_op_idx;
    let op = &op[op_idx];

    if (op.id == OPID_RESET && op.q1 == ALL_QUBITS) {
        // Zero init all the existing shot data
        *shot = ShotData();
        // Set the shot_id and rng_state based on the uniform params and op data
        shot.shot_id = params.start_shot_id + shot_buffer_idx;
        shot.rng_state.x[0] = op.q2; // The seed is passed in q2
        shot.rng_state.x[1] = shot.shot_id;
        shot.next_op_idx = op_idx + 1u;
        shot.rand_pauli = next_rand_f32(shot_buffer_idx);
        shot.rand_damping = next_rand_f32(shot_buffer_idx);
        shot.rand_dephase = next_rand_f32(shot_buffer_idx);
        shot.rand_measure = next_rand_f32(shot_buffer_idx);
        shot.rand_loss = next_rand_f32(shot_buffer_idx);
        shot.renormalize_scale = 1.0;
        shot.duration = 0.0;
        // TODO: Probabilities, heating, and idle since.
    } else {
        // TODO
    }
}

// Each workgroup dispatched is dedicated to a single shot.
@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(@builtin(global_invocation_id) globalId: vec3<u32>) {
    //shots[globalId.x].shot_id = params.start_shot_id + globalId.x;
    //shots[globalId.x].next_op_idx = 0u;
}

fn next_rand_f32(shot_idx: u32) -> f32 {
    // Based on https://en.wikipedia.org/wiki/Xorshift
    var t: u32 = shots[shot_idx].rng_state.x[4];
    let s: u32 = shots[shot_idx].rng_state.x[0];
    shots[shot_idx].rng_state.x[4] = shots[shot_idx].rng_state.x[3];
    shots[shot_idx].rng_state.x[3] = shots[shot_idx].rng_state.x[2];
    shots[shot_idx].rng_state.x[2] = shots[shot_idx].rng_state.x[1];
    shots[shot_idx].rng_state.x[1] = s;

    t = t ^ (t >> 2u);
    t = t ^ (t << 1u);
    t = t ^ s ^ (s << 4u);
    shots[shot_idx].rng_state.x[0] = t;
    shots[shot_idx].rng_state.counter = shots[shot_idx].rng_state.counter + 362437u;
    let rand_u32: u32 = t + shots[shot_idx].rng_state.counter;

    // Convert the 32 random bits to a float in the [0.0, 1.0) range

    // Keep only the lower 23 bits (the fraction portion of a float) with a 0 exponent biased to 127
    let rand_f32_bits = (rand_u32 & 0x7FFFFF) | (127 << 23);
    // Bitcast to f32 in the [1.0, 2.0) range
    let f: f32 = bitcast<f32>(rand_f32_bits);
    // And decrement by 1 to return values from [0..1)
    return f - 1.0;
}
