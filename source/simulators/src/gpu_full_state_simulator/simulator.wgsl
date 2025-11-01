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
override RESULT_COUNT: u32;
override WORKGROUPS_PER_SHOT: i32;
override ENTRIES_PER_THREAD: i32;

const DBG = true; // Enable to add extra checks
const OPT_SKIP_DEFINITE_STATES = true; // Enable to skip processing state vector entries that are definitely 0.0 due to other qubits being in definite states

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

const OPID_PAULI_NOISE_1Q = 128u;
const OPID_PAULI_NOISE_2Q = 129u;
const OPID_LOSS_NOISE = 130u;

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

    // Map this to the Op structure for ease of use
    unitary: array<vec2f, 16>, // For MAT1Q and MAT2Q ops.
    padding: array<vec2f, 48>, // Including the above array, 64 x 8 = 512 bytes
}
// Total struct size = 1024 bytes (including 384 of padding at the end)
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
var<storage, read> ops: array<Op>;

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

fn reset_all(shot_idx: u32, op_idx: u32) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];

    let rng_seed: u32 = op.q2; // The rng seed is passed in q2
    let shot_offset: u32 = op.q3; // The shot offset (e.g. first shot_id in the new batch) is passed in q3

    // Zero init all the existing shot data
    *shot = ShotData();
    // Set the shot_id and rng_state based on the op data
    shot.shot_id = shot_offset + shot_idx;
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
    shot.qubits_updated_last_op_mask = 0;

    // Tell the execute_op stage about the op to execute
    shot.op_idx = op_idx;
    shot.op_type = op.id;

    // Advance to the next op for the next 'prepare' dispatch
    shot.next_op_idx = op_idx + 1u;
}

fn update_qubit_state(shot_idx: u32) {
    let shot = &shots[shot_idx];

    // For each qubit that was updated in the last op
    for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
        let qubit_mask: u32 = 1u << q;
        if ((shot.qubits_updated_last_op_mask & qubit_mask) != 0u) {
            // Sum the workgroup collation entries for this qubit into the shot state
            var total_zero: f32 = 0.0;
            var total_one: f32 = 0.0;

            if (WORKGROUPS_PER_SHOT > 1) {
                // Offset into workgroup collation buffer based on shot index
                let offset = shot_idx * u32(WORKGROUPS_PER_SHOT);
                for (var wkg_idx: u32 = 0u; wkg_idx < u32(WORKGROUPS_PER_SHOT); wkg_idx++) {
                    let sums = workgroup_collation.sums[wkg_idx + offset];
                    total_zero = total_zero + sums.qubits[q].x;
                    total_one = total_one + sums.qubits[q].y;
                }
            } else {
                // Single workgroup per shot case - just read directly from the shot
                total_zero = shot.qubit_state[q].zero_probability;
                total_one = shot.qubit_state[q].one_probability;
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

fn prep_mresetz(shot_idx: u32, op_idx: u32) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];

    // Choose measurement result based on qubit probabilities and random number
    let qubit = op.q1;
    let result_id = op.q2; // Result id to store the measurement result in is stored in q2
    let result = select(1u, 0u, shot.rand_measure < shot.qubit_state[qubit].zero_probability);

    results[(shot_idx * RESULT_COUNT) + result_id] = result;

    // Construct the measurement instrument for MResetZ based on the measured result
    // Put the instrument into the shot buffer for the execute_op stage to apply
    shot.unitary[0] = select(vec2f(1.0, 0.0), vec2f(0.0, 0.0), result == 1u);
    shot.unitary[1] = select(vec2f(0.0, 0.0), vec2f(1.0, 0.0), result == 1u);
    shot.unitary[4] = vec2f();
    shot.unitary[5] = vec2f();

    shot.renormalize = select(
        1.0 / sqrt(shot.qubit_state[qubit].zero_probability),
        1.0 / sqrt(shot.qubit_state[qubit].one_probability),
        result == 1u);

    // Set the qubits_updated_last_op_mask to all except those that were already in a definite
    // state (so we don't waste time updating probabilities that are already known). Note that
    // next 'prepare_op' should set the just measured qubit into a definite 0 or 1 state.
    shot.qubits_updated_last_op_mask =
        // // A mask with all qubits set
        ((1u << u32(QUBIT_COUNT)) - 1u)
        // Exclude qubits already in definite states
            & ~(shot.qubit_is_0_mask | shot.qubit_is_1_mask);

    // The workgroup will sum from its threads into the collation buffer (for multi-workgroup shots)
    // or directly into the shot (if single workgroup shots) during execute_op, so no need to zero it here.

    shot.op_idx = op_idx;
    shot.op_type = op.id;
    shot.next_op_idx = op_idx + 1u;
}

// NOTE: Run with workgroup size of 1 for now, as threads may diverge too much in prepare_op stage causing performance issues.
// TODO: Try to increase later if lack of parallelism is a bottleneck. (Update the dispatch call accordingly).
@compute @workgroup_size(1)
fn prepare_op(@builtin(global_invocation_id) globalId: vec3<u32>) {
    // For the 'prepare_op' stage, each thread dispatched handles one shot, so the globalId.x is the shot index
    let shot_idx = globalId.x;
    let shot = &shots[shot_idx];

    // WebGPU guarantees that buffers are zero-initialized, so next_op_idx will correctly be 0 on the first dispatch
    let op_idx = shot.next_op_idx;
    let op = &ops[op_idx];

    // Default to 1.0 renormalization (i.e., no renormalization needed). MResetZ or noise affecting the
    // overall probability distribution (e.g. loss or amplitude damping) will update this if needed.
    shot.renormalize = 1.0;

    // *******************************
    // PHASE 1: If the op is a full batch reset (i.e. start of a new batch), clean the state and exit
    // *******************************
    if (op.id == OPID_RESET && op.q1 == ALL_QUBITS) {
        reset_all(shot_idx, op_idx);
        return;
    }

    // *******************************
    // PHASE 2: Update the shot state based on the results of the last executed op (if needed)
    // *******************************

    // If any qubits were updated in the last op, we may need to sum workgroup probabilities into the shot state
    // This is only needed if multiple workgroups were used for the shot execution. If not, then the
    // single workgroup for the shot would have written directly to the shot state already.
    if (shot.qubits_updated_last_op_mask != 0) {
        update_qubit_state(shot_idx);
    }

    // *******************************
    // PHASE 3: Generate the next set of random numbers to use for noise and measurement and apply.
    // *******************************

    shot.rand_pauli = next_rand_f32(shot_idx);
    shot.rand_damping = next_rand_f32(shot_idx);
    shot.rand_dephase = next_rand_f32(shot_idx);
    shot.rand_measure = next_rand_f32(shot_idx);
    shot.rand_loss = next_rand_f32(shot_idx);

    // Handle MResetZ operations. These have unique handling and no associated noise ops.
    if (op.id == OPID_MRESETZ) {
        prep_mresetz(shot_idx, op_idx);
        return;
    }

    // *****
    // PHASE 4: Add any noise to the next op
    // *****
    if (arrayLength(&ops) > (op_idx + 1) && ops[op_idx + 1].id == OPID_PAULI_NOISE_1Q) {
        let noise_op = &ops[op_idx + 1];
        // NOTE: Assumes that whatever prepared the program ensured that noise_op.q1 matches op.q1 and that op is a 1-qubit gate

        // Apply 1-qubit Pauli noise based on the probabilities in the op data, which are stored in
        // the real part (x) of the first 3 vec2 entries of the unitary array.
        let p_x = noise_op.unitary[0].x;
        let p_y = noise_op.unitary[1].x;
        let p_z = noise_op.unitary[2].x;

        // Copy the matrix of the original op into the shot buffer for execute_op to use

        let rand = shot.rand_pauli;
        if (rand < p_x) {
            // Apply the X permutation (basically swap the rows)
            shots[shot_idx].unitary[0] = op.unitary[4];
            shots[shot_idx].unitary[1] = op.unitary[5];
            shots[shot_idx].unitary[4] = op.unitary[0];
            shots[shot_idx].unitary[5] = op.unitary[1];
        } else if (rand < (p_x + p_y)) {
            // Apply the Y permutation (swap rows with negated |0> state)
            shots[shot_idx].unitary[0] = cplxNeg(op.unitary[4]);
            shots[shot_idx].unitary[1] = cplxNeg(op.unitary[5]);
            shots[shot_idx].unitary[4] = op.unitary[0];
            shots[shot_idx].unitary[5] = op.unitary[1];
        } else if (rand < (p_x + p_y + p_z)) {
            // Apply Z error (negate |1> state)
            shots[shot_idx].unitary[0] = op.unitary[0];
            shots[shot_idx].unitary[1] = op.unitary[1];
            shots[shot_idx].unitary[4] = cplxNeg(op.unitary[4]);
            shots[shot_idx].unitary[5] = cplxNeg(op.unitary[5]);
        } else {
            // No error to apply. Skip the noise op by advancing the op index and return
            shots[shot_idx].unitary = op.unitary;
        }

        shot.op_type = OPID_SHOT_BUFF_1Q; // Indicate to use the matrix in the shot buffer
        shot.op_idx = op_idx;
        shot.next_op_idx = op_idx + 2u; // Skip over the noise op next time
        shot.qubits_updated_last_op_mask = 1u << op.q1;
        // TODO: What about multiple noise ops in a row? Loop somehow
        return;
    }
    // 2 qubit Pauli noise
    if (arrayLength(&ops) > (op_idx + 1) && ops[op_idx + 1].id == OPID_PAULI_NOISE_2Q) {
        let noise_op = &ops[op_idx + 1];

        // Non correlated noise for now. Just apply the 1Q noise to each qubit in turn
        let p_x = noise_op.unitary[0].x;
        let p_y = noise_op.unitary[1].x;
        let p_z = noise_op.unitary[2].x;

        // If doing a 2 qubit gate, we're not doing a measurement, so 'steal' that random number for qubit 2
        let q1_rand = shot.rand_pauli;
        let q2_rand = shot.rand_measure;

        // Only apply noise if needed
        if (q1_rand < (p_x + p_y + p_z ) || q2_rand < (p_x + p_y + p_z )) {
            // Get the rows of the 2 qubit unitary
            var op_row_0 = getOpRow(op_idx, 0);
            var op_row_1 = getOpRow(op_idx, 1);
            var op_row_2 = getOpRow(op_idx, 2);
            var op_row_3 = getOpRow(op_idx, 3);

            // Apply the Paulis to the matrices. Note this is just permuting the rows, and appliction
            // commutes, so we can apply them in any order. High order bit is q1. Low order bit is q2.
            //   X on q1 is rows  2<>0 and  3<>1, X on q2 is rows  1<>0 and  3<>2, etc.
            //   Y on q1 is rows -2<>0 and -3<>1, Y on q2 is rows -1<>0 and -3<>2
            //   Z on q1 is -2 and -3, Z on q2 is -1 and -3

            // Apply the q1 permutations as needed
            if (q1_rand < p_x) {
                // Apply the X permutation
                let old_row_0 = op_row_0;
                let old_row_1 = op_row_1;
                op_row_0 = op_row_2;
                op_row_1 = op_row_3;
                op_row_2 = old_row_0;
                op_row_3 = old_row_1;
            } else if (q1_rand < (p_x + p_y)) {
                // Apply the Y permutation
                let old_row_0 = op_row_0;
                let old_row_1 = op_row_1;
                op_row_0 = rowNeg(op_row_2);
                op_row_1 = rowNeg(op_row_3);
                op_row_2 = old_row_0;
                op_row_3 = old_row_1;
            } else if (q1_rand < (p_x + p_y + p_z)) {
                // Apply Z permutation
                op_row_2 = rowNeg(op_row_2);
                op_row_3 = rowNeg(op_row_3);
            }
            // Apply the q2 permutations as needed
            if (q2_rand < p_x) {
                // Apply the X permutation
                let old_row_0 = op_row_0;
                let old_row_2 = op_row_2;
                op_row_0 = op_row_1;
                op_row_2 = op_row_3;
                op_row_1 = old_row_0;
                op_row_3 = old_row_2;
            } else if (q2_rand < (p_x + p_y)) {
                // Apply the Y permutation
                let old_row_0 = op_row_0;
                let old_row_2 = op_row_2;
                op_row_0 = rowNeg(op_row_1);
                op_row_2 = rowNeg(op_row_3);
                op_row_1 = old_row_0;
                op_row_3 = old_row_2;
            } else if (q2_rand < (p_x + p_y + p_z)) {
                // Apply Z permutation
                op_row_1 = rowNeg(op_row_1);
                op_row_3 = rowNeg(op_row_3);
            }
            // Write the rows back to the shot buffer unitary
            setUnitaryRow(shot_idx, 0u, op_row_0);
            setUnitaryRow(shot_idx, 1u, op_row_1);
            setUnitaryRow(shot_idx, 2u, op_row_2);
            setUnitaryRow(shot_idx, 3u, op_row_3);

            shot.op_type = OPID_SHOT_BUFF_2Q; // Indicate to use the matrix in the shot buffer
            // TODO: What about multiple noise ops in a row? Loop somehow
        } else {
            // No noise to apply. Skip the noise op by advancing the op index and return
            shot.op_type = op.id;
        }
        shot.op_idx = op_idx;
        shot.next_op_idx = op_idx + 2u; // Skip over the noise op next time
        shot.qubits_updated_last_op_mask = (1u << op.q1 ) | (1u << op.q2);
        return;
    }


    // *******************************
    // PHASE 5: Advance the state ready for the next op
    // *******************************
    // TODO: Figure out exactly what to set here for the execute_op stage to pick up and run with
    shot.op_idx = op_idx;
    shot.next_op_idx = op_idx + 1u;
    shot.op_type = op.id;

    // Turn any Rxx, Ryy, or Rzz gates into a gate from the shot buffer
    // NOTE: Should probably just do this for all gates
    if (op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ) {
        shots[shot_idx].unitary = op.unitary;
        shot.op_type = OPID_SHOT_BUFF_2Q; // Indicate to use the matrix in the shot buffer
    }

    // Set this so the next prepare_op stage knows which qubits to update probabilities for
    shot.qubits_updated_last_op_mask = 1u << op.q1;
    // Update the below condition list if more 2-qubit gates are added (e.g. Rzz, Swap, etc.)
    if (op.id == OPID_CX || op.id == OPID_CZ || shot.op_type == OPID_SHOT_BUFF_2Q) {
        shot.qubits_updated_last_op_mask = shot.qubits_updated_last_op_mask | (1u << op.q2);
    }
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Workgroups are per shot if 22 or less qubits, else 2 workgroups for 23 qubits, 4 for 24, etc..
    let shot_idx: i32 = i32(workgroupId.x) / WORKGROUPS_PER_SHOT;
    let workgroup_idx_in_shot: i32 = i32(workgroupId.x) % WORKGROUPS_PER_SHOT;

    // If the shots spans workgroups, then the thread index is not just the workgroup index
    let thread_idx_in_shot: i32 = workgroup_idx_in_shot * THREADS_PER_WORKGROUP + i32(tid);

    // If using multiple workgroups per shot, each workgroup will write its partial sums here for later collation
    // Use -1 as a marker for single workgroup per shot case to indicate no collation needed (in which
    // case we should write directly to the shot).
    let workgroup_collation_idx: i32 = select(-1, i32(workgroupId.x), WORKGROUPS_PER_SHOT > 1);

    let shot = &shots[shot_idx];

    // Here 'entries' refers to complex amplitudes in the state vector
    let entries_per_shot: i32 = 1 << u32(QUBIT_COUNT);
    let entries_per_workgroup: i32 = entries_per_shot / WORKGROUPS_PER_SHOT;

    let shot_state_vector_start: i32 = shot_idx * entries_per_shot;
    let thread_start_idx: i32 = shot_state_vector_start +
                                workgroup_idx_in_shot * entries_per_workgroup +
                                i32(tid) * ENTRIES_PER_THREAD;

    let op_idx = shot.op_idx;
    let op = &ops[op_idx];

    if (shot.op_type == OPID_RESET && op.q1 == ALL_QUBITS) {
        // Set the state vector to |0...0> by zeroing all amplitudes except the first one
        for(var i: i32 = 0; i < ENTRIES_PER_THREAD; i++) {
            stateVector[thread_start_idx + i] = vec2f(0.0, 0.0);
        }
        // Set the |0...0> amplitude to 1.0 from the first workgroup & thread for the shot
        if (tid == 0 && workgroup_idx_in_shot == 0) {
            stateVector[thread_start_idx] = vec2f(1.0, 0.0);
        }
        return;
    }
    switch (shot.op_type) {
      case OPID_ID, OPID_X, OPID_Y, OPID_Z, OPID_H,
           OPID_S, OPID_SAJD, OPID_T, OPID_TAJD, OPID_SX, OPID_SXAJD,
           OPID_RX, OPID_RY, OPID_RZ, OPID_SHOT_BUFF_1Q, OPID_MRESETZ {
        // All these default gates have the matrix in the op data
        var unitary = array<vec2f, 4>(
            op.unitary[0], op.unitary[1],
            op.unitary[4], op.unitary[5]
        );
        if (shot.op_type == OPID_SHOT_BUFF_1Q || shot.op_type == OPID_MRESETZ) {
            // For transformed gates, use the matrix stored in the shot buffer
            unitary = array<vec2f, 4>(
                shot.unitary[0], shot.unitary[1],
                shot.unitary[4], shot.unitary[5]
            );
        }
        apply_1q_unitary(
            shot_state_vector_start,
            thread_idx_in_shot,
            op.q1,
            tid,
            workgroup_collation_idx,
            shot_idx,
            shot.op_type == OPID_MRESETZ, // Update all probabilities on MRESETZ
            shot.op_type,
            unitary);
        }
      case OPID_CX, OPID_CZ, OPID_SHOT_BUFF_2Q {
        apply_2q_unitary(
            shot_state_vector_start,
            thread_idx_in_shot,
            op.q1,
            op.q2,
            tid,
            workgroup_collation_idx,
            shot_idx,
            shot.op_type);
      }
      default {
        // Oops
      }
    }
}

// For the state vector index and amplitude probability, update all the qubit probabilities for this thread
fn update_all_qubit_probs(stateVectorIndex: u32, amplitude: vec2f, tid: u32) {
    var mask: u32 = 1u;
    for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
        let is_one: bool = (stateVectorIndex & mask) != 0u;
        let prob: f32 = cplxMag2(amplitude);
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
        chunk_idx: i32,         // The index of this chunk within the shot
        qubit: u32,             // The target qubit to apply the 1-qubit gate to
        tid: u32,               // Workgroup thread index to index into the per workgroup probability storage
        wkg_collation_idx: i32, // If >=0, the index in the workgroup collation buffer to write partial sums to
        shot_idx: i32,   // The index of the shot in the shot buffer
        update_probs: bool,     // Whether to update all qubit probabilities or not (e.g. on measurement)
        opid: u32,              // The operation ID (to check for ID gate optimization)
        unitary: array<vec2f, 4>) {
    // First, exit early if we don't need to do any work. If the operation is an ID, there is no renormalization,
    // and we are not updating probabilities, then there is nothing to do.
    let scale = shots[shot_idx].renormalize;
    if (opid == OPID_ID && scale == 1.0 && !update_probs) {
        return;
    }


    // Each iteration processes 2 amplitudes (the pair affected by the 1-qubit gate), so half as many iterations as chunk size
    let iterations = ENTRIES_PER_THREAD >> 1;

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
    var offset = ((start_count >> qubit) << (qubit + 1)) + (start_count & ((1 << qubit) - 1));

    // Optimize for phase operations (e.g., Z, S, T, Rz, etc.) where we can skip half the memory writes
    let zero_untouched = unitary[0].x == 1.0 && unitary[0].y == 0.0 &&
                   unitary[1].x == 0.0 && unitary[1].y == 0.0 && scale == 1.0;

    // For now, always recompute the probabilities from scratch when a qubit is acted upon.
    // (Could me a minor optimization to skip if the unitary doesn't change 0/1 probabilities, e.g., Id, S, Rz, etc.)
    var zero_probability: f32 = 0.0;
    var one_probability: f32 = 0.0;

    let qubit_is_0: u32 = shots[shot_idx].qubit_is_0_mask;
    let qubit_is_1: u32 = shots[shot_idx].qubit_is_1_mask;

    // This loop is where all the real work happens. Try to keep this tight and efficient.
    for (var i: i32 = 0; i < iterations; i++) {
        // See if we can skip doing any work for this pair, because the state vector entries to processes
        // are both definitely 0.0, as we know they are for states where other qubits are in definite opposite state.
        let skip_processing = OPT_SKIP_DEFINITE_STATES &&
            ((u32(offset) & qubit_is_0) != 0) ||
            ((~(u32(offset) | (1u << qubit)) & qubit_is_1) != 0);

        if (!skip_processing) {
            let amp0: vec2f = stateVector[shot_start_offset + offset];
            let amp1: vec2f = stateVector[shot_start_offset + offset + stride];

            // Apply renormalization scaling from prior measurement/noise ops (will be 1.0 if none needed)
            let new0: vec2f = scale * (cplxMul(amp0, unitary[0]) + cplxMul(amp1, unitary[1]));
            let new1: vec2f = scale * (cplxMul(amp0, unitary[2]) + cplxMul(amp1, unitary[3]));

            if (!zero_untouched) { stateVector[shot_start_offset + offset] = new0; }
            stateVector[shot_start_offset + offset + stride] = new1;

            // Update the probabilities for the acted on qubit
            // TODO: Check the float precision here is sufficient
            zero_probability += cplxMag2(new0);
            one_probability += cplxMag2(new1);

            // If updating all qubit probabilities (e.g., on measurement), do that now
            if (update_probs) {
                update_all_qubit_probs(u32(offset), new0, tid);
                update_all_qubit_probs(u32(offset + stride), new1, tid);
            }
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
                sum_thread_totals_to_shot(q, shot_idx, wkg_collation_idx);
            }
        }
    }
}

fn apply_2q_unitary(
        shot_start_offset: i32,
        chunk_idx: i32,
        c: u32,
        t: u32,
        tid: u32,
        workgroup_collation_idx: i32,
        shot_idx: i32,
        opid: u32) {
    // Each iteration processes 4 amplitudes (the four affected by the 2-qubit gate), so quarter as many iterations as chunk size
    let iterations = ENTRIES_PER_THREAD >> 2;

    // Calculate masks to split the index into low, mid, and high bits around the two qubits
    let lowQubit = select(c, t, c > t);
    let hiQubit = select(c, t, c < t);

    // Number of bits in each section
    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = u32(QUBIT_COUNT) - hiQubit - 1;

    // The masks below help extract the low, mid, and high bits from the counter to use around the two qubits locations
    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << u32(QUBIT_COUNT)) - 1 - midMask - lowMask;

    // The counter is the monotonic index for all the iterations. Each iteration processes 4 amplitudes.
    // As this is divided into chunks, we need to offset by the chunk start count.
    let counter  = chunk_idx * iterations;
    let end_count = counter + iterations;

    var c_zero_probability: f32 = 0.0;
    var c_one_probability: f32 = 0.0;
    var t_zero_probability: f32 = 0.0;
    var t_one_probability: f32 = 0.0;

    // Not needed for CZ and CX, but will be for general 2-qubit unitaries
    // And it's outside the loop so cheap(ish) to read
    let row0 = getUnitaryRow(shot_idx, 0);
    let row1 = getUnitaryRow(shot_idx, 1);
    let row2 = getUnitaryRow(shot_idx, 2);
    let row3 = getUnitaryRow(shot_idx, 3);

    let qubit_is_0: u32 = shots[shot_idx].qubit_is_0_mask;
    let qubit_is_1: u32 = shots[shot_idx].qubit_is_1_mask;

    for (var i: i32 = counter; i < end_count; i++) {
        // q1 is the control, q2 is the target
        let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << t);
        let offset10: i32 = offset00 | (1 << c);
        let offset11: i32 = offset10 | (1 << t);

        let can_skip_processing = OPT_SKIP_DEFINITE_STATES &&
            ((u32(offset00) & qubit_is_0) != 0) ||
            ((~(u32(offset11)) & qubit_is_1) != 0);
        if (can_skip_processing) { continue; }

        let scale = shots[shot_idx].renormalize;
        let amp00: vec2f = scale * stateVector[shot_start_offset + offset00];
        let amp01: vec2f = scale * stateVector[shot_start_offset + offset01];
        let amp10: vec2f = scale * stateVector[shot_start_offset + offset10];
        let amp11: vec2f = scale * stateVector[shot_start_offset + offset11];

        // Initialize result as per CZ (as likely most common)
        var result00: vec2f = amp00;
        var result01: vec2f = amp01;
        var result10: vec2f = amp10;
        var result11: vec2f = vec2f(-amp11.x, -amp11.y);

        if (opid == OPID_CZ) {
            if (scale != 1.0) {
                // Only update first 3 states if scaling is needed
                stateVector[shot_start_offset + offset00] = result00;
                stateVector[shot_start_offset + offset01] = result01;
                stateVector[shot_start_offset + offset10] = result10;
            }
            // This was already negated above
            stateVector[shot_start_offset + offset11] = result11;
        } else if (opid == OPID_CX) {
            result10 = amp11;
            result11 = amp10;
            if (scale != 1.0) {
                stateVector[shot_start_offset + offset00] = result00;
                stateVector[shot_start_offset + offset01] = result01;
            }
            stateVector[shot_start_offset + offset10] = result10;
            stateVector[shot_start_offset + offset11] = result11;
        } else {
            // Assume OPID_SHOT_BUFF_2Q
            let states = array<vec2f,4>(amp00, amp01, amp10, amp11);
            result00 = innerProduct(row0, states);
            result01 = innerProduct(row1, states);
            result10 = innerProduct(row2, states);
            result11 = innerProduct(row3, states);
            stateVector[shot_start_offset + offset00] = result00;
            stateVector[shot_start_offset + offset01] = result01;
            stateVector[shot_start_offset + offset10] = result10;
            stateVector[shot_start_offset + offset11] = result11;
        }

        // Update the probabilities for the acted on qubits
        c_zero_probability += cplxMag2(result00) + cplxMag2(result01);
        c_one_probability  += cplxMag2(result10) + cplxMag2(result11);
        t_zero_probability += cplxMag2(result00) + cplxMag2(result10);
        t_one_probability  += cplxMag2(result01) + cplxMag2(result11);
    }
    // Update this thread's totals for the two qubits in the workgroup storage
    qubitProbabilities[c].zero[tid] = c_zero_probability;
    qubitProbabilities[c].one[tid] = c_one_probability;
    qubitProbabilities[t].zero[tid] = t_zero_probability;
    qubitProbabilities[t].one[tid] = t_one_probability;

    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    if (tid == 0) {
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (q == c || q == t) {
                sum_thread_totals_to_shot(q, shot_idx, workgroup_collation_idx);
            }
        }
    }
}

fn sum_thread_totals_to_shot(q: u32, shot_idx: i32, wkg_collation_idx: i32) {
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
        shots[shot_idx].qubit_state[q].zero_probability = total_zero;
        shots[shot_idx].qubit_state[q].one_probability = total_one;
    }
}

// Complex number utilities

// Get the magnitude squared of a complex number
fn cplxMag2(a: vec2f) -> f32 {
    return (a.x * a.x + a.y * a.y);
}

// Complex multiplication
fn cplxMul(a: vec2f, b: vec2f) -> vec2f {
    return vec2f(
        a.x * b.x - a.y * b.y,
        a.x * b.y + a.y * b.x
    );
}

// Complex negation
fn cplxNeg(a: vec2f) -> vec2f {
    return vec2f(-a.x, -a.y);
}

// Negate all elements in a 4-element row of complex numbers
fn rowNeg(a: array<vec2f, 4>) -> array<vec2f, 4> {
    return array<vec2f, 4>(
        cplxNeg(a[0]),
        cplxNeg(a[1]),
        cplxNeg(a[2]),
        cplxNeg(a[3]));
}

// Compute the inner product of two 4-element rows of complex numbers
fn innerProduct(a: array<vec2f, 4>, b: array<vec2f, 4>) -> vec2f {
    var result: vec2f = vec2f(0.0, 0.0);
    for (var i: u32 = 0u; i < 4u; i++) {
        result += cplxMul(a[i], b[i]);
    }
    return result;
}

fn getOpRow(op_idx: u32, row: u32) -> array<vec2f, 4> {
    let op = &ops[op_idx];
    return array<vec2f, 4>(
        op.unitary[row * 4 + 0],
        op.unitary[row * 4 + 1],
        op.unitary[row * 4 + 2],
        op.unitary[row * 4 + 3]);
}

fn getUnitaryRow(shot_idx: i32, row: u32) -> array<vec2f, 4> {
    let shot = &shots[shot_idx];
    return array<vec2f, 4>(
        shot.unitary[row * 4 + 0],
        shot.unitary[row * 4 + 1],
        shot.unitary[row * 4 + 2],
        shot.unitary[row * 4 + 3]);
}

fn setUnitaryRow(shot_idx: u32, row: u32, newRow: array<vec2f, 4>) {
    let shot = &shots[shot_idx];
    shot.unitary[row * 4 + 0] = newRow[0];
    shot.unitary[row * 4 + 1] = newRow[1];
    shot.unitary[row * 4 + 2] = newRow[2];
    shot.unitary[row * 4 + 3] = newRow[3];
}

// Hash and random number generation functions

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
