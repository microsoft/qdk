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

// Always use 32 threads per workgroup for max concurrency on most current GPU hardware
const THREADS_PER_WORKGROUP: i32 = 32;
const MAX_QUBIT_COUNT: i32 = 27;
const MAX_QUBITS_PER_WORKGROUP: i32 = 22;
const MAX_WORKGROUP_SUM_PARTITIONS: i32 = 1 << u32(MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);

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

struct WorkgroupSums {
    qubits: array<vec2f, MAX_QUBIT_COUNT>, // Each vec2f holds (zero_probability, one_probability)
};

struct WorkgroupCollationBuffer {
    sums: array<WorkgroupSums, MAX_WORKGROUP_SUM_PARTITIONS>,
};

@group(0) @binding(0)
var<storage, read_write> workgroup_collation: WorkgroupCollationBuffer;

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

    duration: f32, // Total duration of the shot so far, used for time-dependent noise modeling and shot estimations
    renormalize: f32, // Value to renormalize the state vector by on next execute (1.0 = no renormalization needed)

    // For quick testing during execution to enable skipping blocks of entries
    // TODO: Actually use these masks during execution to skip unneeded work
    qubit_is_0_mask: u32, // Bitmask for which qubits are currently in |0> state
    qubit_is_1_mask: u32, // Bitmask for which qubits are currently in |1> state

    // Track which qubit probabilities were updated in the last operation (to collate on next prepare_op)
    qubits_updated_last_op_mask: u32,
    // 20 x 4 bytes to this point = 80 bytes

    // Track the per-qubit probabilities for optimization of measurement sampling and noise modeling
    qubit_state: array<QubitState, MAX_QUBIT_COUNT>, // 27 x 16 bytes = 432 bytes
    // 512 bytes to this point

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
    // Entries in the unitary are: 00, 01, 02, 03, 10, 11, 12, 13, 20, ..., 32, 33
    // 1q matrix elements are stored in: 00, 01, 10, 11 (i.e., indices 0, 1, 4, and 5)
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


// *******************************
// PREPARE OP
// This stage prepares the shot state for the next operation to execute (and any updates needed from the prior op)
//
// Each op is prepared by one thread. This is how we deal with some of the challenges with synchronization
// when multiple workgroups with multiple threads are used for a shot in the EXECUTE stage. The 'execute_op'
// does work that is 'embarrassingly parallel' across the state vector amplitudes, but the PREPARE_OP stage
// deal with preparing for that work, and collating results back into the shot state afterwards.
//
// This allows us to use the GPU 'dispatch' mechanism to ensure consistencty across shots without complex,
// synchronization code, as the GPU guarantees that all threads in a dispatch complete before the next dispatch
// starts, and all buffer writes are visible to the next dispatch.
// *******************************


// NOTE: Run with workgroup size of 1 for now, as threads may diverge too much in prepare_op stage causing performance issues.
// TODO: Try to increase later if lack of parallelism is a bottleneck. (Update the dispatch call accordingly).
@compute @workgroup_size(1)
fn prepare_op(@builtin(global_invocation_id) globalId: vec3<u32>) {
    // For the 'prepare_op' stage, each thread dispatched handles one shot, so the globalId.x is the shot index
    let shot_buffer_idx = globalId.x;
    let shot = &shots[shot_buffer_idx];

    // WebGPU guarantees that buffers are zero-initialized, so next_op_idx will correctly be 0 on the first dispatch
    let op_idx = shot.next_op_idx;
    let op = &op[op_idx];

    // *******************************
    // PHASE 1: If the op is a full batch reset (i.e. start of a new batch), clean the state and exit
    // *******************************

      if (op.id == OPID_RESET && op.q1 == ALL_QUBITS) {
        let rng_seed: u32 = op.q2; // The rng seed is passed in q2
        let shot_offset: u32 = op.q3; // The shot offset (e.g. first shot_id in the new batch) is passed in q3

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

        // Initialize all qubit probabilities to 100% |0>
        for (var i: i32 = 0; i < QUBIT_COUNT; i++) {
            shot.qubit_state[i].zero_probability = 1.0;
            shot.qubit_state[i].one_probability = 0.0;
            shot.qubit_state[i].heat = 0.0;
            shot.qubit_state[i].idle_since = 0.0;
        }
        shot.qubit_is_0_mask = (1u << u32(QUBIT_COUNT)) - 1u; // All qubits are |0>
        shot.qubit_is_1_mask = 0u;
        shot.renormalize = 1.0;
        shot.qubits_updated_last_op_mask = 0;

        // Tell the execute_op stage about the op to execute
        shot.op_idx = op_idx;
        shot.op_type = op.id;

        // Advance to the next op for the next 'prepare' dispatch
        shot.next_op_idx = op_idx + 1u;
        return;
    }

    // *******************************
    // PHASE 2: Update the shot state based on the results of the last executed op (if needed)
    // *******************************

    // If any qubits were updated in the last op, we may need to sum workgroup probabilities into the shot state
    // This is only needed if multiple workgroups were used for the shot execution. If not, then the
    // single workgroup for the shot would have written directly to the shot state already.
    if (shot.qubits_updated_last_op_mask != 0 && (QUBIT_COUNT > MAX_QUBITS_PER_WORKGROUP)) {
        // For each qubit that was updated in the last op
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            let qubit_mask: u32 = 1u << q;
            if ((shot.qubits_updated_last_op_mask & qubit_mask) != 0u) {
                // Sum the workgroup collation entries for this qubit into the shot state
                var total_zero: f32 = 0.0;
                var total_one: f32 = 0.0;
                let workgroups_per_shot: u32 = 1u << u32(max(0, QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP));
                // Offset into workgroup collation buffer based on shot index
                let offset = shot_buffer_idx * workgroups_per_shot;
                for (var wkg_idx: u32 = 0u; wkg_idx < workgroups_per_shot; wkg_idx++) {
                    let sums = workgroup_collation.sums[wkg_idx + offset];
                    total_zero = total_zero + sums.qubits[q].x;
                    total_one = total_one + sums.qubits[q].y;
                }
                // Update the shot state with the summed probabilities
                // Round to 0 or 1 if extremely close to mitigate minor floating point errors
                if (total_zero < 0.000001) { total_zero = 0.0; }
                if (total_one < 0.000001) { total_one = 0.0; }
                if (total_zero > 0.999999) { total_zero = 1.0; }
                if (total_one > 0.999999) { total_one = 1.0; }

                shot.qubit_state[q].zero_probability = total_zero;
                shot.qubit_state[q].one_probability = total_one;

                // Update the masks for definite states
                shot.qubit_is_0_mask = select(
                    shot.qubit_is_0_mask & ~qubit_mask,
                    shot.qubit_is_0_mask | qubit_mask,
                    total_zero == 1.0);
                shot.qubit_is_1_mask = select(
                    shot.qubit_is_1_mask & ~qubit_mask,
                    shot.qubit_is_1_mask | qubit_mask,
                    total_one == 1.0);
            }
        }
    }

    // *******************************
    // PHASE 3: Generate the next set of random numbers to use for noise and measurement and apply.
    // *******************************

    shot.rand_pauli = next_rand_f32(shot_buffer_idx);
    shot.rand_damping = next_rand_f32(shot_buffer_idx);
    shot.rand_dephase = next_rand_f32(shot_buffer_idx);
    shot.rand_measure = next_rand_f32(shot_buffer_idx);
    shot.rand_loss = next_rand_f32(shot_buffer_idx);

    // TODO: Apply noise based on probabilities and random numbers. (NOTE: Loss acts like an MResetZ)

    // Handle MResetZ operations
    if (op.id == OPID_MRESETZ) {
        // Choose measurement result based on qubit probabilities and random number
        let qubit = op.q1;
        let result = select(1u, 0u, shot.rand_measure < shot.qubit_state[qubit].zero_probability);

        // Construct the measurement instrument for MResetZ based on the measured result
        // Put the instrument into the shot buffer for the execute_op stage to apply
        shots[shot_buffer_idx].unitary[0] = select(vec2f(1.0, 0.0), vec2f(0.0, 0.0), result == 1u);
        shots[shot_buffer_idx].unitary[1] = select(vec2f(0.0, 0.0), vec2f(1.0, 0.0), result == 1u);
        shots[shot_buffer_idx].unitary[4] = vec2f();
        shots[shot_buffer_idx].unitary[5] = vec2f();

        shot.renormalize = select(
            1.0 / sqrt(shot.qubit_state[qubit].zero_probability),
            1.0 / sqrt(shot.qubit_state[qubit].one_probability),
            result == 1u);

        shot.qubit_state[qubit].zero_probability = select(1.0, 0.0, result == 1u);
        shot.qubit_state[qubit].one_probability = select(0.0, 1.0, result == 1u);

        // Update the qubit masks
        shot.qubit_is_0_mask = select(
            shot.qubit_is_0_mask | (1u << qubit),
            shot.qubit_is_0_mask & ~(1u << qubit),
            result == 1u);
        shot.qubit_is_1_mask = select(
            shot.qubit_is_1_mask & ~(1u << qubit),
            shot.qubit_is_1_mask | (1u << qubit),
            result == 1u);

        // Set the qubits_updated_last_op_mask to all except the measured qubit, and those that were
        // already in a definite state (so we don't waste time updating probabilities that are already known)
        shot.qubits_updated_last_op_mask =
            // // A mask with all qubits set
            ((1u << u32(QUBIT_COUNT)) - 1u)
            // Exclude qubits already in definite states (which will include the just measured qubit)
             & ~(shot.qubit_is_0_mask | shot.qubit_is_1_mask);

        // The workgroup will sum from its threads into the collation buffer (for multi-workgroup shots)
        // or directly into the shot (if single workgroup shots) during execute_op, so no need to zero it here.

        shot.op_idx = op_idx;
        shot.op_type = op.id;
        shot.next_op_idx = op_idx + 1u;
        return;
    }

    // *******************************
    // PHASE 4: Advance the state ready for the next op
    // *******************************
    // TODO: Figure out exactly what to set here for the execute_op stage to pick up and run with
    shot.op_idx = op_idx;
    shot.next_op_idx = op_idx + 1u;

    // Set this so the next prepare_op stage knows which qubits to update probabilities for
    shot.qubits_updated_last_op_mask = 1u << op.q1;
    // Update the below condition list if more 2-qubit gates are added (e.g. Rzz, Swap, etc.)
    if (op.id == OPID_CX || op.id == OPID_CZ) {
        shot.qubits_updated_last_op_mask = shot.qubits_updated_last_op_mask | (1u << op.q2);
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

    // If the shots spans workgroups, then the thread index is not just the workgroup index
    let thread_idx_in_shot: i32 = workgroup_idx_in_shot * THREADS_PER_WORKGROUP + i32(tid);

    // If using multiple workgroups per shot, each workgroup will write its partial sums here for later collation
    // Use -1 as a marker for single workgroup per shot case to indicate no collation needed (in which
    // case we should write directly to the shot).
    let workgroup_collation_idx: i32 = select(-1, i32(workgroupId.x), workgroups_per_shot > 1);

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
        return;
    }
    switch (op.id) {
      case OPID_ID, OPID_X, OPID_Y, OPID_Z, OPID_H,
           OPID_S, OPID_SAJD, OPID_T, OPID_TAJD, OPID_SX, OPID_SXAJD,
           OPID_RX, OPID_RY, OPID_RZ {
        // All these default gates have the matrix in the op data
        let unitary = array<vec2f, 4>(
            op.unitary[0], op.unitary[1],
            op.unitary[4], op.unitary[5]
        );
        apply_1q_unitary(
            shot_state_vector_start,
            entries_per_thread,
            thread_idx_in_shot,
            op.q1,
            tid,
            workgroup_collation_idx,
            shot_buffer_idx,
            false, // No need to update all qubit probabilities
            unitary);
        }
      case OPID_CX {
        apply_cx_cz(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, op.q2, false);
      }
      case OPID_CZ {
        apply_cx_cz(shot_state_vector_start, entries_per_thread, thread_idx_in_shot, op.q1, op.q2, true);
      }
      case OPID_MRESETZ {
        // The MResetZ instrument matrix for the result is stored in the shot buffer
        let instrument = array<vec2f, 4>(
            shot.unitary[0], shot.unitary[1],
            shot.unitary[4], shot.unitary[5]
        );
        apply_1q_unitary(
            shot_state_vector_start,
            entries_per_thread,
            thread_idx_in_shot,
            op.q1,
            tid,
            workgroup_collation_idx,
            shot_buffer_idx,
            true, // Update all qubit probabilities
            instrument);
      }
      default {
        // Oops
      }
    }
}

// For the state vector index and amplitude probability, update all the qubit probabilities for this thread
fn update_qubits_probs(stateVectorIndex: u32, amplitude: vec2f, tid: u32) {
    var mask: u32 = 1u;
    for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
        let is_one: bool = (stateVectorIndex & mask) != 0u;
        let prob: f32 = amplitude.x * amplitude.x + amplitude.y * amplitude.y;
        if (is_one) {
            qubitProbabilities[q].one[tid] += prob;
        } else {
            qubitProbabilities[q].zero[tid] += prob;
        }
        mask = mask << 1u;
    }
}

fn apply_1q_unitary(
        shot_start_offset: i32, // The starting index of the shot in the state vector
        chunk_size: i32,        // The number of amplitudes ths call should process
        chunk_idx: i32,         // The index of this chunk within the shot
        qubit: u32,             // The target qubit to apply the 1-qubit gate to
        tid: u32,               // Workgroup thread index to index into the per workgroup probability storage
        wkg_collation_idx: i32, // If >=0, the index in the workgroup collation buffer to write partial sums to
        shot_buffer_idx: i32,   // The index of the shot in the shot buffer
        update_probs: bool,     // Whether to update all qubit probabilities or not (e.g. on measurement)
        unitary: array<vec2f, 4>) {
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

    let scale = shots[shot_buffer_idx].renormalize;

    // Optimize for phase operations (e.g., Z, S, T, Rz, etc.) where we can skip half the memory writes
    let zero_untouched = unitary[0].x == 1.0 && unitary[0].y == 0.0 &&
                   unitary[1].x == 0.0 && unitary[1].y == 0.0 && scale == 1.0;

    // For now, always recompute the probabilities from scratch when a qubit is acted upon.
    // (Could me a minor optimization to skip if the unitary doesn't change 0/1 probabilities, e.g., Id, S, Rz, etc.)
    var zero_probability: f32 = 0.0;
    var one_probability: f32 = 0.0;

    // This loop is where all the real work happens. Try to keep this tight and efficient.
    for (var i: i32 = 0; i < iterations; i++) {
        let amp0: vec2f = stateVector[offset];
        let amp1: vec2f = stateVector[offset + stride];

        // Apply renormalization scaling from prior measurement/noise ops (will be 1.0 if none needed)
        let new0: vec2f = scale * (cplxmul(amp0, unitary[0]) + cplxmul(amp1, unitary[1]));
        let new1: vec2f = scale * (cplxmul(amp0, unitary[2]) + cplxmul(amp1, unitary[3]));

        if (!zero_untouched) { stateVector[offset] = new0; }
        stateVector[offset + stride] = new1;

        // Update the probabilities for the acted on qubit
        // TODO: Check the float precision here is sufficient
        zero_probability += (new0.x * new0.x + new0.y * new0.y);
        one_probability += (new1.x * new1.x + new1.y * new1.y);

        // If updating all qubit probabilities (e.g., on measurement), do that now
        if (update_probs) {
            update_qubits_probs(u32(offset), new0, tid);
            update_qubits_probs(u32(offset + stride), new1, tid);
        }

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
    qubitProbabilities[qubit].zero[tid] = zero_probability;
    qubitProbabilities[qubit].one[tid] = one_probability;

    workgroupBarrier();

    // After all threads have updated their per-thread probabilities, have the first thread reduce them into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    if (tid == 0) {
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (q == qubit || update_probs) {
                var total_zero: f32 = 0.0;
                var total_one: f32 = 0.0;
                for (var j = 0; j < THREADS_PER_WORKGROUP; j++) {
                    total_zero += qubitProbabilities[q].zero[j];
                    total_one += qubitProbabilities[q].one[j];
                }
                if (wkg_collation_idx >= 0) {
                    // Write to the workgroup collation buffer for later summation into the shot state
                    workgroup_collation.sums[wkg_collation_idx].qubits[q] = vec2f(total_zero, total_one);
                } else {
                    // Single workgroup per shot case - write directly to the shot state
                    shots[shot_buffer_idx].qubit_state[q].zero_probability = total_zero;
                    shots[shot_buffer_idx].qubit_state[q].one_probability = total_one;
                }
            }
        }
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
            // Probability densities don't change for CZ
        } else {
            let old10 = stateVector[offset10];
            stateVector[offset10] = stateVector[offset11];
            stateVector[offset11] = old10;
            // TODO: Update the [target] probabilities
        }
    }

    workgroupBarrier();
    // TODO: Update the probabilities for both qubits (NOTE: Not needed for CZ... just target for CX?)
}

fn cplxmul(a: vec2f, b: vec2f) -> vec2f {
    return vec2f(
        a.x * b.x - a.y * b.y,
        a.x * b.y + a.y * b.x
    );
}

// See https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/
// Use PCG hash function to generate a well-distributed hash from a simple integer input (e.g., shot id)
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
