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
const THREADS_PER_WORKGROUP: i32 = 32;
const MAX_QUBIT_COUNT: i32 = 27;
const MAX_QUBITS_PER_WORKGROUP: i32 = 22;

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
    qubit_state: array<QubitState, MAX_QUBIT_COUNT>, // 27 x 16 bytes = 432 bytes
    // 496 bytes to this point

    padding: array<u32, 4>, // Reserve 16 bytes, rounding the struct size up to 512 bytes at this point

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
}; // size: 256 bytes
var<workgroup> qubitProbabilities: array<QubitProbabilityPerThread, QUBIT_COUNT>;
// Workgroup memory size: QUBIT_COUNT (max 27) * 256 = max 6,912 bytes.

// TODO: Run with 1 for now, as threads may diverge too much in prepare_op stage causing performance issues.
// Try to increase later if lack of parallelism is a bottleneck.
@compute @workgroup_size(1)
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
        shot.op_idx = op_idx;
        shot.next_op_idx = op_idx + 1u;
    }
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Workgroups are per shot if 22 or less qubits, else 2 workgroups for 23 qubits, 4 for 24, etc..
    let workgroups_per_shot: i32 = 1 << u32(max(0, QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP));
    let shot_buffer_idx: i32 = i32(workgroupId.x) / workgroups_per_shot;
    let workgroup_idx_in_shot: i32 = i32(workgroupId.x) % workgroups_per_shot;
    let thread_idx_in_shot: i32 = workgroup_idx_in_shot * THREADS_PER_WORKGROUP + i32(tid);

    let shot = &shots[shot_buffer_idx];

    // Here 'entries' refers to complex amplitudes in the state vector
    let entries_per_shot: i32 = 1 << u32(QUBIT_COUNT);
    let entries_per_workgroup: i32 = entries_per_shot / workgroups_per_shot;
    let entries_per_thread: i32 = entries_per_workgroup / THREADS_PER_WORKGROUP;

    let shot_state_vector_start: i32 = shot_buffer_idx * entries_per_shot;
    let thread_start_idx: i32 = shot_state_vector_start +
                                workgroup_idx_in_shot * entries_per_workgroup +
                                i32(tid) * entries_per_thread;

    let op_idx = shot.op_idx;
    let op = &op[op_idx];

    if (shot.op_type == OPID_RESET && op.q1 == ALL_QUBITS) {
        // Set the state vector to |0...0> by zeroing all amplitudes except the first one
        for(var i: i32 = 0; i < entries_per_thread; i++) {
            stateVector[thread_start_idx + i] = vec2f(0.0, 0.0);
        }
        // Set the |0...0> amplitude to 1.0 from the first workgroup & thread for the shot
        if (tid == 0 && workgroup_idx_in_shot == 0) {
            stateVector[thread_start_idx] = vec2f(1.0, 0.0);
        }
    } else if (op.id == OPID_H) {
        var unitary: array<vec2f, 4> = array<vec2f, 4>(
            vec2f(0.70710678, 0.0), vec2f(0.70710678, 0.0),
            vec2f(0.70710678, 0.0), vec2f(-0.70710678, 0.0)
        );
        apply_1q_unitary(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, unitary);
    } else if (op.id == OPID_SX) {
        var unitary: array<vec2f, 4> = array<vec2f, 4>(
            vec2f(0.5, 0.5), vec2f(0.5, -0.5),
            vec2f(0.5, -0.5), vec2f(0.5, 0.5)
        );
        apply_1q_unitary(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, unitary);
    } else if (op.id == OPID_CX) {
        apply_cx_cz(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, op.q2, false);
    } else if (op.id == OPID_CX) {
        apply_cx_cz(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, op.q2, true);
    } else {
        // TODO: Rz
    }
}

fn apply_1q_unitary(shot_start_offset: i32, chunk_size: i32, chunk_idx: i32, qubit: u32, unitary: array<vec2f, 4>) {
    // Each iteration processes 2 amplitudes (the pair affected by the 1-qubit gate), so half as many iterations as chunk size
    let iterations = chunk_size >> 1;

    // Being we are doing half as many iterations for each chunk, what is the start count for this chunk?
    let start_count = chunk_idx * iterations;

    // The corresponding stride between the two amplitudes in each pair (i.e., the distance to jump to
    // get from the |0> amplitude to the |1> amplitude for the target qubit)
    let stride = 1 << qubit;

    // Calculate starting offset for this chunk:
    // - Take start_count and split it into high and low parts at the qubit bit position
    // - Low part: (start_count & ((1 << qubit) - 1)) = bits below the target qubit (stay as-is)
    // - High part: (start_count >> qubit) << (qubit + 1) = bits above the target qubit (shift left by qubit+1 to create the stride gap)
    // - This effectively interleaves the chunks to index only amplitudes where target qubit = 0
    var offset = shot_start_offset + ((start_count >> qubit) << (qubit + 1)) + (start_count & ((1 << qubit) - 1));

    // Optimize for phase operations where we can skip half the memory updates
    let is_phase = unitary[0].x == 1.0 && unitary[0].y == 0.0 &&
                   unitary[1].x == 0.0 && unitary[1].y == 0.0;

    for (var i: i32 = 0; i < iterations; i++) {
        let amp0: vec2f = stateVector[offset];
        let amp1: vec2f = stateVector[offset + stride];

        if (!is_phase) { stateVector[offset] = cplxmul(amp0, unitary[0]) + cplxmul(amp1, unitary[1]); }
        stateVector[offset + stride] = cplxmul(amp0, unitary[2]) + cplxmul(amp1, unitary[3]);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_cx_cz(shot_start_offset: i32, chunk_size: i32, chunk_idx: i32, c: u32, t: u32, is_cz: bool) {
    // Each iteration processes 4 amplitudes (the four affected by the 2-qubit gate), so quarter as many iterations as chunk size
    let iterations = chunk_size >> 2;

    // Being we are doing quarter as many iterations for each chunk, what is the start count for this chunk?
    let start_count = shot_start_offset + chunk_idx * iterations;
    let end_count = start_count + iterations;

    let lowQubit = select(c, t, c > t);
    let hiQubit = select(c, t, c < t);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = u32(MAX_QUBIT_COUNT) - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    for (var i: i32 = start_count; i < end_count; i++) {
        // q1 is the control, q2 is the target
        let offset10: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2) | (1 << c);
        let offset11: i32 = offset10 | (1 << t);

        if (is_cz) {
            let old11 = stateVector[offset11];
            stateVector[offset11] = vec2f(-old11.x, -old11.y);
        } else {
            let old10 = stateVector[offset10];
            stateVector[offset10] = stateVector[offset11];
            stateVector[offset11] = old10;
        }
    }
}

fn cplxmul(a: vec2f, b: vec2f) -> vec2f {
    return vec2f(
        a.x * b.x - a.y * b.y,
        a.x * b.y + a.y * b.x
    );
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
