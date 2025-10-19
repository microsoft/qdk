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
override QUBIT_COUNT: i32;
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

// If the application of noise results in a custom matrix, it will have been stored in the shot buffer
// These OPIDs indicate to use that matrix and for how many qubits. (The qubit ids are in the original Op)
const OPID_SHOT_BUFF_1Q = 256u;
const OPID_SHOT_BUFF_2Q = 257u;
const OPID_SHOT_BUFF_3Q = 258u;

// The below is used when an operation is to be applied to all qubits - such as a system reset.
const ALL_QUBITS: u32 = 0xFFFFFFFFu;

// Unform buffer containing per-batch execution parameters
// See https://webgpufundamentals.org/webgpu/lessons/webgpu-uniforms.html
struct UniformParams {
    // Nothing for now. Reserving for later.
    reserved: u32,
}
@group(0) @binding(0)
var<uniform> params: UniformParams;

struct QubitState {
    zero_probability: f32,
    one_probability: f32,
    heat: f32,
    idle_since: f32,
}

struct xorwow_state {
    counter: u32,
    x: array<u32, 5>
}

// Buffer containing the state for each shot to execute per kernel dispatch
// An instance of this is tracked on the GPU for every active shot
struct ShotData {
    shot_id: u32,
    next_op_idx: u32,
    // The below random numbers will be initialized from the RNG per operation in the 'prepare_op' stage
    // Then the 'execute_op' stage will read these precomputed random numbers for noise modeling
    rng_state: xorwow_state, // 6 x u32
    rand_pauli: f32,
    rand_damping: f32,
    rand_dephase: f32,
    rand_measure: f32,
    rand_loss: f32,

    // The type of the next operation to execute. This will be OPID_SHOT_BUFF_* if it should use the unitary from the op buffer
    op_type: u32,
    op_idx: u32,

    // Total duration of the shot so far, used for time-dependent noise modeling and shot estimations
    duration: f32,
    // 16 x 4 bytes to this point = 64 bytes

    // Track the per-qubit probabilities for optimization of measurement sampling and noise modeling
    qubit_state: array<QubitState, MAX_QUBIT_COUNT>, // 22 x 16 bytes = 352 bytes
    // 416 bytes to this point

    padding: array<u32, 24>, // Reserve 96 bytes, rounding the struct size up to 512 bytes at this point

    // Making this large enough to hold a 3-qubit gate (8x8 matrix) such as Toffoli for when we add it.
    unitary: array<vec2f, 64>, // For MAT1Q and MAT2Q ops. 64 x 8 = 512 bytes
}
// Total struct size = 1024 bytes
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
} // size: 6 * 4 + 16 * 8 + 4 + 25 * 4 = 256 bytes
@group(0) @binding(2)
var<storage, read> op: array<Op>;

// The one large buffer of state vector amplitudes. (Partitioned into multiple shots)
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
        let rng_seed: u32 = op.q2; // The seed is passed in q2
        let shot_offset: u32 = op.q3; // The shot offset is passed in q3

        // Zero init all the existing shot data
        *shot = ShotData();
        // Set the shot_id and rng_state based on the op data
        shot.shot_id = shot_offset + shot_buffer_idx;
        shot.rng_state.x[0] = rng_seed ^ hash_pcg(shot.shot_id);
        shot.rng_state.x[1] = rng_seed ^ hash_pcg(shot.shot_id + 1);
        shot.rng_state.x[2] = rng_seed ^ hash_pcg(shot.shot_id + 2);
        shot.rng_state.x[3] = rng_seed ^ hash_pcg(shot.shot_id + 3);
        shot.rng_state.x[4] = rng_seed ^ hash_pcg(shot.shot_id + 4);
        shot.duration = 0.0;
        for (var i: i32 = 0; i < QUBIT_COUNT; i = i + 1) {
            shot.qubit_state[i].zero_probability = 1.0; // After reset, all qubits are in |0>
            shot.qubit_state[i].one_probability = 0.0;
            shot.qubit_state[i].heat = 0.0;
            shot.qubit_state[i].idle_since = 0.0;
        }
        // Tell the execute_op stage about the op to execute
        shot.op_idx = op_idx;
        shot.op_type = op.id;
        // Advance to the next op for the next dispatch
        shot.next_op_idx = op_idx + 1u;
    } else {
        shot.rand_pauli = next_rand_f32(shot_buffer_idx);
        shot.rand_damping = next_rand_f32(shot_buffer_idx);
        shot.rand_dephase = next_rand_f32(shot_buffer_idx);
        shot.rand_measure = next_rand_f32(shot_buffer_idx);
        shot.rand_loss = next_rand_f32(shot_buffer_idx);

        // TODO Handle supported op types
        // Advance to the next op for the next dispatch (e.g., skip over noise that got processed here)
        shot.next_op_idx = op_idx + 1u;
    }
}

// Each workgroup dispatched is dedicated to a single shot. So the workgroup_id.x indicates the shot index.
// The local_invocation_id.x indicates the thread within the workgroup, with each responsible for a different part of the state vector.
@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_id) localId: vec3<u32>) {
    let shot_buffer_idx = i32(workgroupId.x);
    let shot = &shots[shot_buffer_idx];
    let tid = localId.x;

    let op_idx = shot.op_idx;
    let op = &op[op_idx];

    // Calculate the start index in the state vector for this shot and thread
    let amplitudes_per_shot: i32 = 1 << u32(QUBIT_COUNT);
    let amplitudes_per_thread: i32 = amplitudes_per_shot / i32(THREADS_PER_WORKGROUP);
    let shot_start_idx: i32 = shot_buffer_idx * amplitudes_per_shot;
    let thread_start_idx: i32 = shot_start_idx + i32(tid) * amplitudes_per_thread;

    if (shot.op_type == OPID_RESET && op.q1 == ALL_QUBITS) {
        // Set the state vector to |0...0> by zeroing all amplitudes except the first one
        for(var i: i32 = 0; i < amplitudes_per_thread; i = i + 1) {
            stateVector[thread_start_idx + i] = vec2f(0.0, 0.0);
        }
        // Set the |0...0> amplitude to 1.0 from thread 0
        if (tid == 0u) {
            stateVector[shot_start_idx] = vec2f(1.0, 0.0);
        }
    } else {
        // TODO
    }
}

// See https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/
fn hash_pcg(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn next_rand_f32(shot_idx: u32) -> f32 {
    // Based on https://en.wikipedia.org/wiki/Xorshift
    let rng_state = &shots[shot_idx].rng_state;

    var t: u32 = rng_state.x[4];
    let s: u32 = rng_state.x[0];
    rng_state.x[4] = rng_state.x[3];
    rng_state.x[3] = rng_state.x[2];
    rng_state.x[2] = rng_state.x[1];
    rng_state.x[1] = s;

    t = t ^ (t >> 2u);
    t = t ^ (t << 1u);
    t = t ^ s ^ (s << 4u);
    rng_state.x[0] = t;
    rng_state.counter = rng_state.counter + 362437u;
    let rand_u32: u32 = t + rng_state.counter;

    // Convert the 32 random bits to a float in the [0.0, 1.0) range

    // Keep only the lower 23 bits (the fraction portion of a float) with a 0 exponent biased to 127
    let rand_f32_bits = (rand_u32 & 0x7FFFFF) | (127 << 23);
    // Bitcast to an f32 in the [1.0, 2.0) range
    let f: f32 = bitcast<f32>(rand_f32_bits);
    // And decrement by 1 to return values from [0..1)
    return f - 1.0;
}
