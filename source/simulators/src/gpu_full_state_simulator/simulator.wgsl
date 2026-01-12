// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// See https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html for an overview
// See https://www.w3.org/TR/WGSL/ for the details
// See https://webgpu.github.io/webgpu-samples/ for examples

// WGSL has pipeline overridables, but they're a pain and limited, so just string replace constants here
const QUBIT_COUNT: i32 = {{QUBIT_COUNT}};
const RESULT_COUNT: u32 = {{RESULT_COUNT}};
const WORKGROUPS_PER_SHOT: i32 = {{WORKGROUPS_PER_SHOT}};
const ENTRIES_PER_THREAD: i32 = {{ENTRIES_PER_THREAD}};
const THREADS_PER_WORKGROUP: i32 = {{THREADS_PER_WORKGROUP}};
const MAX_QUBIT_COUNT: i32 = {{MAX_QUBIT_COUNT}};
const MAX_QUBITS_PER_WORKGROUP: i32 = {{MAX_QUBITS_PER_WORKGROUP}};

const ERR_INVALID_PROBS = 1u;
const ERR_INVALID_THREAD_TOTAL = 2u;

const PROB_THRESHOLD: f32 = 0.0001; // Tolerance for probabilities to sum to 1.0


// Always use 32 threads per workgroup for max concurrency on most current GPU hardware
const MAX_WORKGROUP_SUM_PARTITIONS: i32 = 1 << u32(MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);

// Operation IDs
const OPID_ID      = 0u;
const OPID_RESET   = 1u;
const OPID_S       = 6u;
const OPID_SAdj    = 7u;
const OPID_T       = 8u;
const OPID_TAdj    = 9u;
const OPID_RZ      = 14u;
const OPID_CX      = 15u;
const OPID_CZ      = 16u;
const OPID_RXX     = 17u;
const OPID_RYY     = 18u;
const OPID_RZZ     = 19u;
const OPID_MRESETZ = 22u;
const OPID_SWAP    = 24u;
const OPID_MAT1Q   = 25u;
const OPID_MAT2Q   = 26u;

const OPID_PAULI_NOISE_1Q = 128u;
const OPID_PAULI_NOISE_2Q = 129u;
const OPID_LOSS_NOISE = 130u;
const OPID_CORRELATED_NOISE = 131u;

// If the application of noise results in a custom matrix, it will have been stored in the shot buffer
// These OPIDs indicate to use that matrix and for how many qubits. (The qubit ids are in the original Op)
const OPID_SHOT_BUFF_1Q = 256u;
const OPID_SHOT_BUFF_2Q = 257u;

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
// Around 128 max partitions times 27 qubits times 8 bytes = 27 KB max size

struct QubitState {
    zero_probability: f32,
    one_probability: f32,
    heat: f32, // -1.0 = lost
    idle_since: f32,
}

// Used to track state for the random number generator per shot. See `next_rand_f32` later for details.
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
}
// Total struct size = 640 bytes (which is aligned to 128 bytes)
// See https://www.w3.org/TR/WGSL/#structure-member-layout for alignment rules

@group(0) @binding(1)
var<storage, read_write> shots: array<ShotData>;

// Buffer containing the list of operations (gates and noise) that make up the program to simulate
struct Op {
    id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    // Entries in the unitary are: 00, 01, 02, 03, 10, 11, 12, 13, 20, ..., 32, 33
    // 1q matrix elements are stored in: 00, 01, 10, 11 (i.e., indices 0, 1, 4, and 5)
    unitary: array<vec2f, 16>,
} // Struct size: 4 * 4 + 16 * 8 = 144 bytes (which is aligned to 16 bytes)

@group(0) @binding(2)
var<storage, read> ops: array<Op>;

// The one large buffer of state vector amplitudes. (Partitioned into multiple shots)
@group(0) @binding(3)
var<storage, read_write> stateVector: array<vec2f>;

// Buffer for storing measurement results per shot
@group(0) @binding(4)
var<storage, read_write> results: array<atomic<u32>>;

// When an error occurs, the below diagnostic data structure is used to store information about the error
struct DiagnosticData {
    error_code: atomic<u32>,
    extra1: u32,
    extra2: f32,
    extra3: f32,
    shot: ShotData, // 640 bytes
    op: Op,         // 144 bytes
    // Below is usually 6,912 bytes (size = THREADS_PER_WORKGROUP (32) * (8 * MAX_QUBIT_COUNT (27))
    workgroup_probabilities: array<QubitProbabilityPerThread, THREADS_PER_WORKGROUP>,
    // Below is usually 27,648 bytes (1 << u32(MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP)) * (8 * MAX_QUBIT_COUNT) bytes
    collation_buffer: WorkgroupCollationBuffer,
};

@group(0) @binding(5)
var<storage, read_write> diagnostics: DiagnosticData;

struct Uniforms {
    batch_start_shot_id: i32,
    rng_seed: u32,
}

@group(0) @binding(6)
var<uniform> uniforms: Uniforms;

struct NoiseTableMetadata {
    /// The total probability of any noise (i.e. sum of all noise entries) in `Q1.63` format
    noise_probability_lo: u32,
    noise_probability_hi: u32,
    /// The start offset of this table's entries in the global `NoiseTableEntry` array
    start_offset: u32,
    /// The number of entries in this noise table
    entry_count: u32,
}

struct NoiseTableEntry {
    /// The correlated pauli string as bits (2 bits per qubit). If bit 0 is set, then it has bit-flip
    /// noise, and if bit 1 is set then it has phase-flip noise. e.g., `110001 == "YIX"`
    paulis_lo: u32,
    paulis_hi: u32,
    /// The probability of the noise occurring in `Q1_63` format. This is a float format where the high
    /// order bit (bit 63) has the value 1.0 (`2^0 / 1`), bit 62 has the value 0.5 (`2^1 / 1`), etc.
    /// all the way to bit 63 with a value of approx 1.0842e-19 (`2^63 / 1`). This gives a range of
    /// values from [0..2) with equal spacing of 1.0842e-19 between values (unlike float or double),
    /// which makes it more suitable for random numbers used to select between a large number of small
    /// probability entries.
    probability_lo: u32,
    probability_hi: u32,
}

@group(0) @binding(7)
var<storage, read> correlated_noise_tables: array<NoiseTableMetadata>;

@group(0) @binding(8)
var<storage, read> correlated_noise_entries: array<NoiseTableEntry>;

// For every qubit, each 'execute' kernel thread will update its own workgroup storage location for accumulating probabilities
// The final probabilities will be reduced and written back to the shot state after the parallel execution completes.
struct QubitProbabilityPerThread {
    zero: array<f32, MAX_QUBIT_COUNT>,
    one: array<f32, MAX_QUBIT_COUNT>,
}; // size: 216 bytes

var<workgroup> qubitProbabilities: array<QubitProbabilityPerThread, THREADS_PER_WORKGROUP>;
// Workgroup memory size: THREADS_PER_WORKGROUP (32) * 216 = 6,912 bytes.

fn is_1q_phase_gate(op_id: u32) -> bool {
    return (op_id == OPID_S || op_id == OPID_SAdj || op_id == OPID_T || op_id == OPID_TAdj || op_id == OPID_RZ);
}

fn shot_init_per_op(shot_idx: u32) {
    let shot = &shots[shot_idx];

    // Default to 1.0 renormalization (i.e., no renormalization needed). MResetZ or noise affecting the
    // overall probability distribution (e.g. loss or amplitude damping) will update this if needed.
    shot.renormalize = 1.0;
    shot.qubits_updated_last_op_mask = 0u;

    // Generate the next set of random numbers to use for noise and measurement
    shot.rand_pauli = next_rand_f32(shot_idx);
    shot.rand_damping = next_rand_f32(shot_idx);
    shot.rand_dephase = next_rand_f32(shot_idx);
    shot.rand_measure = next_rand_f32(shot_idx);
    shot.rand_loss = next_rand_f32(shot_idx);
}

fn reset_all(shot_idx: i32) {
    let shot = &shots[shot_idx];

    // One of the main goals of the shot_id is to seed the RNG state uniquely per shot
    let rng_seed = uniforms.rng_seed;
    let shot_id = u32(uniforms.batch_start_shot_id + shot_idx);

    // Due to DX12 backend issues, we can't just assign a zeroed struct, so manually reset all fields
    // DX12-start-strip
    *shot = ShotData();
    // DX12-end-strip
    shot.shot_id = shot_id;

    // After init, start execution from the first op
    shot.next_op_idx = 0u;

    shot.rng_state.x[0] = rng_seed ^ hash_pcg(shot_id);
    shot.rng_state.x[1] = rng_seed ^ hash_pcg(shot_id + 1);
    shot.rng_state.x[2] = rng_seed ^ hash_pcg(shot_id + 2);
    shot.rng_state.x[3] = rng_seed ^ hash_pcg(shot_id + 3);
    shot.rng_state.x[4] = rng_seed ^ hash_pcg(shot_id + 4);

    shot.op_type = 0;
    shot.op_idx = 0;

    // rand_* will be initialized in shot_init_per_op when preparing the first op
    shot.duration = 0.0;
    shot.renormalize = 1.0;

    shot.qubit_is_0_mask = (1u << u32(QUBIT_COUNT)) - 1u; // All qubits are |0>
    shot.qubit_is_1_mask = 0u;
    shot.qubits_updated_last_op_mask = 0;

    // Initialize all qubit probabilities to 100% |0>
    for (var i: i32 = 0; i < QUBIT_COUNT; i++) {
        shot.qubit_state[i].zero_probability = 1.0;
        shot.qubit_state[i].one_probability = 0.0;
        shot.qubit_state[i].heat = 0.0;
        shot.qubit_state[i].idle_since = 0.0;
    }

    // unitary will be set in prepare_op
}

fn update_qubit_state(shot_idx: u32) {
    let shot = &shots[shot_idx];

    // If any qubits were updated in the last op, we may need to sum workgroup probabilities into the shot state
    // This is only needed if multiple workgroups were used for the shot execution. If not, then the
    // single workgroup for the shot would have written directly to the shot state already.

    // For each qubit that was updated in the last op
    for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
        let qubit_mask: u32 = 1u << q;
        if ((shot.qubits_updated_last_op_mask & qubit_mask) != 0u) {
            // Sum the workgroup collation entries for this qubit into the shot state
            // Note: We ignore the fact a qubit may be 'lost' here. It should already be
            // in the |0> state if lost, so summing the probabilities is still valid.
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
            // TODO: Use PROB_THRESHOLD constant here?
            if (total_zero < 0.000001) { total_zero = 0.0; }
            if (total_one < 0.000001) { total_one = 0.0; }
            if (total_zero > 0.999999) { total_zero = 1.0; }
            if (total_one > 0.999999) { total_one = 1.0; }

            shot.qubit_state[q].zero_probability = total_zero;
            shot.qubit_state[q].one_probability = total_one;

            // NOTE: Any kind of operation with a NaN float value results in a NaN, or false for logical comparisons
            // So beware of conditions that may not behave as expected if NaN values are possible.
            let within_threshold = abs(1.0 - (total_zero + total_one)) < PROB_THRESHOLD;
            if !within_threshold {
                // Populate the diagnostics buffer, if not already set
                let old_value = atomicCompareExchangeWeak(
                    &diagnostics.error_code,
                    0u,
                    ERR_INVALID_PROBS);
                if old_value.exchanged {
                    // This is the first error - fill in the details
                    diagnostics.extra1 = q;
                    diagnostics.extra2 = total_zero;
                    diagnostics.extra3 = total_one;
                    // DX12 backend has issues assigning structs. See https://github.com/gfx-rs/wgpu/issues/8552
                    // DX12-start-strip
                    diagnostics.shot = *shot;
                    diagnostics.op = ops[shot.op_idx];
                    // DX12-end-strip
                }
                // Store the error value (if none set already)
                let err_index = (shot_idx + 1) * RESULT_COUNT - 1;
                atomicCompareExchangeWeak(
                    &results[err_index],
                    0u,
                    ERR_INVALID_PROBS);
            }

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

fn prep_mresetz(shot_idx: u32, op_idx: u32, is_loss: bool) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];

    // Choose measurement result based on qubit probabilities and random number
    let qubit = op.q1;
    let result = select(1u, 0u, shot.rand_measure < shot.qubit_state[qubit].zero_probability);

    // If this is being called due to loss noise, we don't write the result back to the results buffer
    // Instead, mark the qubit as lost by setting the heat to -1.0
    if !is_loss {
        let result_id = op.q2; // Result id to store the measurement result in is stored in q2

        // If the qubit is already marked as lost, just report that and exit. It's already in the zero
        // state so nothing to update or renormalize. The execute op shoud be a no-op (ID)
        if shot.qubit_state[qubit].heat == -1.0 {
            atomicStore(&results[(shot_idx * RESULT_COUNT) + result_id], 2u);
            shot.op_type = OPID_ID;
            shot.op_idx = op_idx;
            // Qubit get reloaded after a Measurement, so set the heat back to 0.0
            shot.qubit_state[qubit].heat = 0.0;
            return;
        } else {
            atomicStore(&results[(shot_idx * RESULT_COUNT) + result_id], result);
        }
    } else {
        shot.qubit_state[qubit].heat = -1.0;
    }

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

    // We don't want the measurement pass to skip over this qubit, so ensure it's marked as not in a definite state
    shot.qubit_is_1_mask = shot.qubit_is_1_mask & ~(1u << qubit);
    shot.qubit_is_0_mask = shot.qubit_is_0_mask & ~(1u << qubit);

    // Set the qubits_updated_last_op_mask to all except those that were already in a definite
    // state (so we don't waste time updating probabilities that are already known). Note that
    // next 'prepare_op' should set the just measured qubit into a definite 0 or 1 state.
    shot.qubits_updated_last_op_mask =
        // // A mask with all qubits set
        ((1u << u32(QUBIT_COUNT)) - 1u)
        // Exclude qubits already in definite states
            & ~(shot.qubit_is_0_mask | shot.qubit_is_1_mask);

    shot.op_idx = op_idx;
    shot.op_type = OPID_MRESETZ;
}

// Starting from the given index, return the next index if pauli noise, else 0
fn get_pauli_noise_idx(op_idx: u32) -> u32 {
    if (arrayLength(&ops) > (op_idx + 1)) {
        let op = &ops[op_idx + 1];
        if (op.id == OPID_PAULI_NOISE_1Q || op.id == OPID_PAULI_NOISE_2Q) {
            return op_idx + 1u;
        }
    }
    return 0u;
}

// From the starting index given, return the next index if loss noise, else 0
fn get_loss_idx(op_idx: u32) -> u32 {
    if (arrayLength(&ops) > (op_idx + 1)) {
        let op = &ops[op_idx + 1];
        if (op.id == OPID_LOSS_NOISE) {
            return op_idx + 1u;
        }
    }
    return 0u;
}

fn apply_1q_pauli_noise(shot_idx: u32, op_idx: u32, noise_idx: u32) {
    // NOTE: Assumes that whatever prepared the program ensured that noise_op.q1 matches op.q1 and
    // that op is a 1-qubit gate
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let noise_op = &ops[noise_idx];

    // Apply 1-qubit Pauli noise based on the probabilities in the op data, which are stored in
    // the real part (x) of the first 4 vec2 entries of the unitary array (ignore [0] which is "I")
    let p_x = noise_op.unitary[1].x;
    let p_y = noise_op.unitary[2].x;
    let p_z = noise_op.unitary[3].x;

    shot.op_type = OPID_SHOT_BUFF_1Q; // Indicate to use the matrix in the shot buffer

    let rand = shot.rand_pauli;
    if (rand < p_x) {
        // Apply the X permutation (basically swap the rows)
        shot.unitary[0] = op.unitary[4];
        shot.unitary[1] = op.unitary[5];
        shot.unitary[4] = op.unitary[0];
        shot.unitary[5] = op.unitary[1];
    } else if (rand < (p_x + p_y)) {
        // Apply the Y permutation (swap rows with negated |0> state)
        shot.unitary[0] = cplxNeg(op.unitary[4]);
        shot.unitary[1] = cplxNeg(op.unitary[5]);
        shot.unitary[4] = op.unitary[0];
        shot.unitary[5] = op.unitary[1];
    } else if (rand < (p_x + p_y + p_z)) {
        // Apply Z error (negate |1> state)
        shot.unitary[0] = op.unitary[0];
        shot.unitary[1] = op.unitary[1];
        shot.unitary[4] = cplxNeg(op.unitary[4]);
        shot.unitary[5] = cplxNeg(op.unitary[5]);
    } else {
        // No noise. Set the op_type back to the op.id value if it's Id, Reset, or MResetZ, as they get handled specially in execute_op
        if (op.id == OPID_ID || op.id == OPID_RESET || op.id == OPID_MRESETZ) {
            shot.op_type = op.id;
        }
        if (is_1q_phase_gate(op.id)) {
            // For phase gates, treat everything as RZ for execution purposes
            shot.op_type = OPID_RZ;
        }
    }

    shot.op_idx = op_idx;
    if (shot.op_type == OPID_ID || shot.op_type == OPID_RZ) {
        shot.qubits_updated_last_op_mask = 0u;
    } else {
        shot.qubits_updated_last_op_mask = 1u << op.q1;
    };
}

fn apply_2q_pauli_noise(shot_idx: u32, op_idx: u32, noise_idx: u32) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let noise_op = &ops[noise_idx];

    // Correlated noise is stored in the real parts of the unitary.
    // unitary[0] = II, unitary[1] = IX, unitary[2] = IY, unitary[3] = IZ
    // unitary[4] = XI, unitary[5] = XX, unitary[6] = XY, unitary[7] = XZ
    // unitary[8] = YI, unitary[9] = YX, unitary[10]= YY, unitary[11]= YZ
    // unitary[12]= ZI, unitary[13]= ZX, unitary[14]= ZY, unitary[15]= ZZ

    var rand = shot.rand_pauli;
    var q1_pauli = 0;
    var q2_pauli = 0;

    // Find the paulis to apply based on the random number and the probabilities
    for (var i = 0; i < 4; i = i + 1) {
        for (var j = 0; j < 4; j = j + 1) {
            let p_ij = noise_op.unitary[i * 4 + j].x;
            if (rand < p_ij) {
                q1_pauli = i;
                q2_pauli = j;
                // Break out of both loops
                i = 4;
                j = 4;
            } else {
                rand = rand - p_ij;
            }
        }
    }

    // Only apply noise if needed
    if (q1_pauli != 0 || q2_pauli != 0) {
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
        if (q1_pauli == 1) {
            // Apply the X permutation
            let old_row_0 = op_row_0;
            let old_row_1 = op_row_1;
            op_row_0 = op_row_2;
            op_row_1 = op_row_3;
            op_row_2 = old_row_0;
            op_row_3 = old_row_1;
        } else if (q1_pauli == 2) {
            // Apply the Y permutation
            let old_row_0 = op_row_0;
            let old_row_1 = op_row_1;
            op_row_0 = rowNeg(op_row_2);
            op_row_1 = rowNeg(op_row_3);
            op_row_2 = old_row_0;
            op_row_3 = old_row_1;
        } else if (q1_pauli == 3) {
            // Apply Z permutation
            op_row_2 = rowNeg(op_row_2);
            op_row_3 = rowNeg(op_row_3);
        }
        // Apply the q2 permutations as needed
        if (q2_pauli == 1) {
            // Apply the X permutation
            let old_row_0 = op_row_0;
            let old_row_2 = op_row_2;
            op_row_0 = op_row_1;
            op_row_2 = op_row_3;
            op_row_1 = old_row_0;
            op_row_3 = old_row_2;
        } else if (q2_pauli == 2) {
            // Apply the Y permutation
            let old_row_0 = op_row_0;
            let old_row_2 = op_row_2;
            op_row_0 = rowNeg(op_row_1);
            op_row_2 = rowNeg(op_row_3);
            op_row_1 = old_row_0;
            op_row_3 = old_row_2;
        } else if (q2_pauli == 3) {
            // Apply Z permutation
            op_row_1 = rowNeg(op_row_1);
            op_row_3 = rowNeg(op_row_3);
        }
        // Write the rows back to the shot buffer unitary
        setUnitaryRow(shot_idx, 0u, op_row_0);
        setUnitaryRow(shot_idx, 1u, op_row_1);
        setUnitaryRow(shot_idx, 2u, op_row_2);
        setUnitaryRow(shot_idx, 3u, op_row_3);
        shot.op_type = OPID_SHOT_BUFF_2Q;
    } else {
        // No noise to apply. Leave if CX or CZ  or RZZ as they get handled specially in execute_op
        if (op.id == OPID_CX || op.id == OPID_CZ || op.id == OPID_RZZ) {
            shot.op_type = op.id;
        } else {
            shot.op_type = OPID_SHOT_BUFF_2Q;
        }
    }
    shot.op_idx = op_idx;
    if (shot.op_type == OPID_CZ || shot.op_type == OPID_RZZ) {
        shot.qubits_updated_last_op_mask = 0u;
    } else  {
        shot.qubits_updated_last_op_mask = (1u << op.q1 ) | (1u << op.q2);
    }
}

// Get the qubit id at the given index from the correlated noise op's qubit args
// Qubit args are stored in the unitary matrix elements as f32 values
fn get_correlated_noise_qubit(op_idx: u32, index: u32) -> u32 {
    // Qubit ids are stored in the unitary as f32 values, starting at unitary[0].x, unitary[0].y, etc.
    let vec_idx = index / 2u;
    let component = index % 2u;
    if (component == 0u) {
        return u32(ops[op_idx].unitary[vec_idx].x);
    } else {
        return u32(ops[op_idx].unitary[vec_idx].y);
    }
}

// Prepare the shot state for executing a correlated noise operation
fn prep_correlated_noise(shot_idx: u32, op_idx: u32) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];

    // The noise table index is stored in op.q1, and the qubit count is stored in op.q2
    let noise_table_idx = op.q1;
    let qubit_count = op.q2;
    let table = &correlated_noise_tables[noise_table_idx];

    // Generate a Q1.63 random number (two u32 values for lo and hi 32 bits)
    // Mask off the high bit of rand_hi to ensure the value is in [0, 1) range
    let rand_lo = next_rand_u32(shot_idx);
    let rand_hi = next_rand_u32(shot_idx) & 0x7FFFFFFFu;

    // Get the total noise probability from the table metadata
    let noise_prob_lo = table.noise_probability_lo;
    let noise_prob_hi = table.noise_probability_hi;

    // Check if noise should be applied at all by comparing the random number against the total noise probability
    // If rand >= noise_probability, then no noise is applied
    if (rand_hi > noise_prob_hi || (rand_hi == noise_prob_hi && rand_lo >= noise_prob_lo)) {
        // No noise to apply - set the op to ID and return
        shot.op_type = OPID_ID;
        shot.op_idx = op_idx;
        shot.qubits_updated_last_op_mask = 0u;
        return;
    }

    // Noise should be applied - binary search to find which Pauli string to apply
    let start = i32(table.start_offset);
    let count = i32(table.entry_count);
    let entry_idx = binary_search_noise_table(rand_lo, rand_hi, start, count);
    let entry = &correlated_noise_entries[start + entry_idx];

    // Extract the Pauli string (2 bits per qubit: bit 0 = X flip, bit 1 = Z flip)
    let paulis_lo = entry.paulis_lo;
    let paulis_hi = entry.paulis_hi;

    // Build bit-flip and phase-flip masks based on the Pauli string and qubit arguments
    // For each qubit in the correlated noise op, check its Pauli type and set the corresponding mask bits
    var bit_flip_mask: u32 = 0u;
    var phase_flip_mask: u32 = 0u;

    for (var i: u32 = 0u; i < qubit_count; i++) {
        // Get the 2-bit Pauli value for this qubit position in the Pauli string
        // The Rust parsing stores paulis with the rightmost (last) character at the lowest bits,
        // but we want string position i (leftmost = 0) to map to qubit arg i.
        // So for position i, we need bits at (qubit_count - 1 - i) * 2.
        let bit_position = qubit_count - 1u - i;
        var pauli_bits: u32;
        if (bit_position < 16u) {
            pauli_bits = (paulis_lo >> (bit_position * 2u)) & 0x3u;
        } else {
            pauli_bits = (paulis_hi >> ((bit_position - 16u) * 2u)) & 0x3u;
        }

        // Get the actual qubit id from the op's qubit arguments
        let qubit_id = get_correlated_noise_qubit(op_idx, i);
        let qubit_mask = 1u << qubit_id;

        // Pauli encoding: 0=I, 1=X, 2=Z, 3=Y (X and Z)
        let has_bit_flip = (pauli_bits & 0x1u) != 0u;   // X or Y
        let has_phase_flip = (pauli_bits & 0x2u) != 0u; // Z or Y

        if (has_bit_flip) {
            bit_flip_mask |= qubit_mask;
        }
        if (has_phase_flip) {
            phase_flip_mask |= qubit_mask;
        }
    }

    // Store the masks in the shot buffer for the execute stage
    // We use the unitary entries to store these masks (reinterpreted as floats)
    shot.unitary[0] = vec2f(bitcast<f32>(bit_flip_mask), bitcast<f32>(phase_flip_mask));

    // For bit-flipped qubits, we need to swap the 0 and 1 probabilities and masks
    // This is done in prepare_op, not execute_op, since it's a simple swap
    for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
        let qubit_mask = 1u << q;
        if ((bit_flip_mask & qubit_mask) != 0u) {
            // Swap the probabilities
            let temp = shot.qubit_state[q].zero_probability;
            shot.qubit_state[q].zero_probability = shot.qubit_state[q].one_probability;
            shot.qubit_state[q].one_probability = temp;

            // Swap the bits in qubit_is_0_mask and qubit_is_1_mask
            let was_0 = (shot.qubit_is_0_mask & qubit_mask) != 0u;
            let was_1 = (shot.qubit_is_1_mask & qubit_mask) != 0u;
            if (was_0) {
                shot.qubit_is_0_mask &= ~qubit_mask;
                shot.qubit_is_1_mask |= qubit_mask;
            } else if (was_1) {
                shot.qubit_is_1_mask &= ~qubit_mask;
                shot.qubit_is_0_mask |= qubit_mask;
            }
        }
    }

    // Set up the shot state for the correlated noise execution
    shot.op_type = OPID_CORRELATED_NOISE;
    shot.op_idx = op_idx;
    // No probabilities need to be recomputed in execute_op since we've already swapped them here
    shot.qubits_updated_last_op_mask = 0u;
}

struct ShotParams {
    shot_idx: i32,
    shot_state_vector_start: i32,
    workgroup_collation_idx: i32,
    workgroup_idx_in_shot: i32,
    thread_idx_in_shot: i32,
    total_threads_per_shot: i32,
    zero_entry_count: i32,
    op_iterations: i32,
}

fn get_shot_params(
        workgroupId: u32,
        tid: u32,
        op_qubit_count: i32) -> ShotParams {
    // Workgroups are per shot if 22 or less qubits, else 2 workgroups for 23 qubits, 4 for 24, etc..
    let shot_idx: i32 = i32(workgroupId) / WORKGROUPS_PER_SHOT;
    let shot_state_vector_start: i32 = shot_idx * (1 << u32(QUBIT_COUNT));
    let workgroup_idx_in_shot: i32 = i32(workgroupId) % WORKGROUPS_PER_SHOT;
    let thread_idx_in_shot: i32 = workgroup_idx_in_shot * THREADS_PER_WORKGROUP + i32(tid);
    let total_threads_per_shot: i32 = WORKGROUPS_PER_SHOT * THREADS_PER_WORKGROUP;

    // If using multiple workgroups per shot, each workgroup will write its partial sums to the collation
    // buffer for later summing by the prepare_op stage. If single workgroup per shot, no collation needed.
    // Use -1 as a marker for single workgroup per shot case (in which case we should write directly to the shot).
    let workgroup_collation_idx: i32 = select(-1, i32(workgroupId), WORKGROUPS_PER_SHOT > 1);

    let zero_entry_count: i32 = (1 << u32(QUBIT_COUNT)) >> u32(op_qubit_count);
    let op_iterations: i32 = zero_entry_count / total_threads_per_shot;

    return ShotParams(
        shot_idx,
        shot_state_vector_start,
        workgroup_collation_idx,
        workgroup_idx_in_shot,
        thread_idx_in_shot,
        total_threads_per_shot,
        zero_entry_count,
        op_iterations
    );
}

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
    let shot_idx = globalId.x;
    let shot = &shots[shot_idx];

    // WebGPU guarantees that buffers are zero-initialized, so next_op_idx will correctly be 0 on the first dispatch
    let op_idx = shot.next_op_idx;

    // If we've gone past the end, set the op type to id and exit, so the execute stage is a no-op
    if (op_idx >= u32(arrayLength(&ops))) {
        // TODO: Set error/diagnostic info here
        shot.op_type = OPID_ID;
        shot.renormalize = 1.0;
        shot.qubits_updated_last_op_mask = 0u;
        return;
    }

    let op = &ops[op_idx];

    // Update the shot state based on the results of the last executed op (if needed)
    if (shot.qubits_updated_last_op_mask != 0) {
        update_qubit_state(shot_idx);
    }

    shot_init_per_op(shot_idx);
    shot.unitary = op.unitary;

    // Handle preparation MResetZ operations. These have unique handling and no associated noise ops, so prep and exit
    if (op.id == OPID_MRESETZ) {
        prep_mresetz(shot_idx, op_idx, false /* is_loss */);
        shot.next_op_idx = op_idx + 1u; // MResetZ has no associated noise ops, so just advance by 1
        return;
    }

    /* Handle noise:
       - For the 1-qubit op case, there could be pauli and loss noise after the op itself. We want to check for loss first and
         only apply pauli noise if the qubit wasn't lost. (If lost, the pauli noise and even the gate itself don't matter).
       - For the 2-qubit op case, there will only be optional pauli noise after the op itself. (Loss is applied via separate
         Id ops on each qubit after the 2-qubit op).
    */

    let pauli_op_idx = get_pauli_noise_idx(op_idx);
    let loss_op_idx = get_loss_idx(select(op_idx, pauli_op_idx, pauli_op_idx != 0u));
    shot.next_op_idx = max(op_idx, max(pauli_op_idx, loss_op_idx)) + 1u;

    // Handle correlated noise operations
    if (op.id == OPID_CORRELATED_NOISE) {
        prep_correlated_noise(shot_idx, op_idx);
        return;
    }

    // Before doing further work, if any qubit for the gate is lost, just skip by marking the op as ID
    if (shot.qubit_state[op.q1].heat == -1.0) ||
       (op.id == OPID_CX || op.id == OPID_CZ || op.id == OPID_SWAP || op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ || op.id == OPID_MAT2Q) &&
       (shot.qubit_state[op.q2].heat == -1.0) {
        shot.op_type = OPID_ID;
        shot.op_idx = op_idx;
        return;
    }

    // If there is loss noise to apply, do that now
    if (loss_op_idx != 0u) {
        let loss_op = &ops[loss_op_idx];
        let p_loss = loss_op.unitary[0].x; // Loss probability is stored in the x part of first vec2
        if (shot.rand_loss < p_loss) {
            // Qubit is lost - perform MResetZ with is_loss = true
            prep_mresetz(shot_idx, loss_op_idx, true /* is_loss */);
            // There is no further noise of gate to apply, just the loss execution.
            return;
        }
    }

    if pauli_op_idx != 0 {
        if ops[pauli_op_idx].id == OPID_PAULI_NOISE_1Q {
            apply_1q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
            // This will have set up all the state we need.
            return;
        } else {
            apply_2q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
            return;
        }
    }

    // No noise to apply, just set up the shot to execute the op as-is
    shot.op_idx = op_idx;
    shot.op_type = op.id;

    // Turn any Rxx, Ryy, or Rzz gates into a gate from the shot buffer
    // NOTE: Should probably just do this for all gates
    if (op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_MAT2Q || op.id == OPID_SWAP) {
        shot.op_type = OPID_SHOT_BUFF_2Q; // Indicate to use the matrix in the shot buffer
    }

    if (op.id > OPID_RESET && op.id < OPID_CX) {
        shot.op_type = OPID_SHOT_BUFF_1Q; // Indicate to use the matrix in the shot buffer
    }

    if (is_1q_phase_gate(op.id)) {
        // For phase gates, treat everything as RZ for execution purposes
        shot.op_type = OPID_RZ;
    }

    // Set this so the next prepare_op stage knows which qubits to update probabilities for
    switch shot.op_type {
      case OPID_ID, OPID_CZ, OPID_RZ, OPID_RZZ {
        shot.qubits_updated_last_op_mask = 0u;
      }
      case OPID_SHOT_BUFF_1Q {
        shot.qubits_updated_last_op_mask = 1u << op.q1;
      }
      case OPID_CX, OPID_SHOT_BUFF_2Q {
        shot.qubits_updated_last_op_mask = (1u << op.q1) | (1u << op.q2);
      }
      default {
        // TODO: Set error/diagnostic info here
      }
    }
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn initialize(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Get the params
    let params = get_shot_params(workgroupId.x, tid, 0 /* qubits per op */);

    // We want every thread to zero out its portion of the state vector for the shot
    // We also want threads executing in lockstep to update adjacent entries for better memory access patterns
    for (var i = 0; i < params.op_iterations; i++) {
        let entry_index: i32 = params.thread_idx_in_shot + i * params.total_threads_per_shot;
        stateVector[params.shot_state_vector_start + entry_index] = vec2f(0.0, 0.0);
    }

    // NOTE: No need to synchronize here, as each thread is writing to unique locations
    if (params.thread_idx_in_shot == 0) {
        // Set the |0...0> amplitude to 1.0 from the first workgroup & thread for the shot
        stateVector[params.shot_state_vector_start] = vec2f(1.0, 0.0);
        reset_all(params.shot_idx);
    }
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_op(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Get the params
    let params = get_shot_params(workgroupId.x, tid, 1 /* qubits per op */);

    let shot = &shots[params.shot_idx];
    let op_idx = shot.op_idx;
    let op = &ops[op_idx];
    let scale = shot.renormalize;

    // Handle correlated noise specially - it has its own iteration pattern
    if (shot.op_type == OPID_CORRELATED_NOISE) {
        // For correlated noise, we need to iterate over the full state vector
        let correlated_params = get_shot_params(workgroupId.x, tid, 0 /* qubits per op */);
        apply_correlated_noise(correlated_params);
        // No probability updates needed - they were handled in prepare_op
    } else if (shot.op_type != OPID_ID) {
        // Skip doing any work if the op is ID (no-op)
        let lowMask = (1 << op.q1) - 1;
        let highMask = (1 << u32(QUBIT_COUNT)) - 1 - lowMask;

        // For now, always recompute the probabilities from scratch when a qubit is acted upon.
        // (Could me a minor optimization to skip if the unitary doesn't change 0/1 probabilities, e.g., Id, S, Rz, etc.)
        var summed_probs: vec4f = vec4f();

        /* This loop is where all the real work happens. Try to keep this tight and efficient.

        We want a 'structure of arrays' like access pattern here for efficiency, so we process the state vector
        in blocks where each thread in the workgroup(s) handle an adjacent entry to be processed.

        Each thread should start at the state vector shot start + 'thread_idx_in_shot', which is sequential across the workgroup threads
        Each next entry for the thread is WORKGROUPS_PER_SHOT * THREADS_PER_WORKGROUP away.
        The thread should process zero_entries / threads_per_shot iterations, which is stored in op_iterations.
        */
        var entry_index = params.thread_idx_in_shot;

        for (var i = 0; i < params.op_iterations; i++) {
            let offset0: i32 = (entry_index & lowMask) | ((entry_index & highMask) << 1);
            let offset1: i32 = offset0 | (1 << op.q1);

            // See if we can skip doing any work for this pair, because the state vector entries to processes
            // are both definitely 0.0, as we know they are for states where other qubits are in definite opposite state.
            let skip_processing =
                ((offset0 & i32(shots[params.shot_idx].qubit_is_0_mask)) != 0) ||
                ((~offset1 & i32(shots[params.shot_idx].qubit_is_1_mask)) != 0);

            if (!skip_processing) {
                if shot.op_type == OPID_RZ {
                    // For RZ, we can skip reading/writing the |0> amplitude, as it is unchanged.
                    // Just apply the phase to the |1> amplitude. Probabilities also don't change.
                    let amp1: vec2f = stateVector[params.shot_state_vector_start + offset1];
                    let new1 = cplxMul(amp1, shot.unitary[5]);
                    stateVector[params.shot_state_vector_start + offset1] = new1;
                } else {
                    let amp0: vec2f = stateVector[params.shot_state_vector_start + offset0];
                    let amp1: vec2f = stateVector[params.shot_state_vector_start + offset1];

                    let new0 = scale * (cplxMul(amp0, shot.unitary[0]) + cplxMul(amp1, shot.unitary[1]));
                    let new1 = scale * (cplxMul(amp0, shot.unitary[4]) + cplxMul(amp1, shot.unitary[5]));

                    stateVector[params.shot_state_vector_start + offset0] = new0;
                    stateVector[params.shot_state_vector_start + offset1] = new1;

                    if shot.op_type == OPID_MRESETZ || scale != 1.0 {
                        // For MResetZ or renormalization, we need to update the probabilities for all qubits
                        update_all_qubit_probs(u32(offset0), new0, tid);
                        update_all_qubit_probs(u32(offset1), new1, tid);
                    } else {
                        summed_probs[0] += cplxMag2(new0);
                        summed_probs[1] += cplxMag2(new1);
                    }
                }
            }
            entry_index += params.total_threads_per_shot;
        }

        if scale == 1.0 && shot.op_type != OPID_RZ && shot.op_type != OPID_MRESETZ {
            // Update this thread's totals for the two qubits in the workgroup storage
            qubitProbabilities[tid].zero[op.q1] = summed_probs[0];
            qubitProbabilities[tid].one[op.q1]  = summed_probs[1];
        }
    }

    // workgroupBarrier can't be conditional in DX12 backend, so we have to do an unconditional one here
    // outside of the skip_work conditional above.
    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    // Skip for correlated noise since probabilities were already updated in prepare_op.
    if (tid == 0 && shot.op_type != OPID_RZ && shot.op_type != OPID_ID && shot.op_type != OPID_CORRELATED_NOISE) {
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (shot.qubits_updated_last_op_mask & (1u << q)) != 0u {
                sum_thread_totals_to_shot(q, params.shot_idx, params.workgroup_collation_idx);
            }
        }
    }
}

fn apply_correlated_noise(params: ShotParams) {
    // Probabilities are already updated in the prepare_op stage
    // Here we just need to apply the bit-flips and phase-flips to the state vector amplitudes

    let shot = &shots[params.shot_idx];

    // Get the bit-flip and phase-flip masks from the shot buffer (stored by prep_correlated_noise)
    let bit_flip_mask = bitcast<u32>(shot.unitary[0].x);
    let phase_flip_mask = bitcast<u32>(shot.unitary[0].y);

    // If no flips to apply, early exit
    if (bit_flip_mask == 0u && phase_flip_mask == 0u) {
        return;
    }

    var entry_index = params.thread_idx_in_shot;

    for (var i = 0; i < params.op_iterations; i++) {
        // Get the target index to swap the state with by flipping the bits as indicated in the bit_flip_mask
        let target_index = entry_index ^ i32(bit_flip_mask);

        // If there are an odd number of phase flips for the entry, we need to negate the amplitude
        let negate_index: f32 = select(1.0, -1.0, (countOneBits(entry_index & i32(phase_flip_mask)) & 1) != 0);

        if (bit_flip_mask == 0u && negate_index == -1.0) {
            // No bit flips to perform, but need to negate this entry (phase flip only)
            stateVector[params.shot_state_vector_start + entry_index] = cplxNeg(stateVector[params.shot_state_vector_start + entry_index]);
        } else if (entry_index < target_index) {
            // Bit flips are happening (as the indices are different), but to avoid double swapping only handle the swap
            // when entry_index < target_index (avoid reprocessing when later we encounter the target_index entry as the entry_index)

            let amp_entry: vec2f = stateVector[params.shot_state_vector_start + entry_index];
            let amp_target: vec2f = stateVector[params.shot_state_vector_start + target_index];

            // If there are an odd number of phase flips for the target, we need to negate that amplitude too
            let negate_target: f32 = select(1.0, -1.0, (countOneBits(target_index & i32(phase_flip_mask)) & 1) != 0);

            // Swap and apply any negations for phase flips.
            // Note this only applies -1 & 1 to the phase, not -i and i as the 'canonical' Y gate does.
            // However, this is sufficient for simulating noise, as the global phase doesn't matter.
            stateVector[params.shot_state_vector_start + entry_index] = cplxMul(amp_target, vec2f(negate_index, 0.0));
            stateVector[params.shot_state_vector_start + target_index] = cplxMul(amp_entry, vec2f(negate_target, 0.0));
        }

        // Jump ahead to the next entry to process
        entry_index += params.total_threads_per_shot;
    }
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_2q_op(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Get the params
    let params = get_shot_params(workgroupId.x, tid, 2 /* qubits per op */);

    // Workgroups are per shot if 22 or less qubits, else 2 workgroups for 23 qubits, 4 for 24, etc..
    let shot = &shots[params.shot_idx];

    let op_idx = shot.op_idx;
    let op = &ops[op_idx];

    // Calculate masks to split the index into low, mid, and high bits around the two qubits
    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    // Number of bits in each section
    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = u32(QUBIT_COUNT) - hiQubit - 1;

    // The masks below help extract the low, mid, and high bits from the counter to use around the two qubits locations
    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << u32(QUBIT_COUNT)) - 1 - midMask - lowMask;

    // Each iteration processes 4 amplitudes (the four affected by the 2-qubit gate), so quarter as many iterations as chunk size
    var entry_index = params.thread_idx_in_shot;
    var summed_probs: vec4f = vec4f();

    let update_probs = shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ;

    for (var i = 0; i < params.op_iterations; i++) {
        // q1 is the control, q2 is the target
        let offset00: i32 = (entry_index & lowMask) | ((entry_index & midMask) << 1) | ((entry_index & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << op.q2);
        let offset10: i32 = offset00 | (1 << op.q1);
        let offset11: i32 = offset10 | (1 << op.q2);

        let can_skip_processing =
            (((u32(offset00) & shot.qubit_is_0_mask) != 0) ||
            ((~(u32(offset11)) & shot.qubit_is_1_mask) != 0));
        if !can_skip_processing {
            switch shot.op_type {
              case OPID_CZ {
                let amp11: vec2f = stateVector[params.shot_state_vector_start + offset11];
                stateVector[params.shot_state_vector_start + offset11] = cplxNeg(amp11);
                // CZ doesn't change any probabilities, so no need to update summed_probs
              }
              case OPID_RZZ {
                // Firt and last entries are unchanged, only need to update the middle two
                let amp01: vec2f = stateVector[params.shot_state_vector_start + offset01];
                let amp10: vec2f = stateVector[params.shot_state_vector_start + offset10];
                // Unitary matrix second entry in the second row is 5, third entry in the third row is 10
                stateVector[params.shot_state_vector_start + offset01] = cplxMul(amp01, shot.unitary[5]);
                stateVector[params.shot_state_vector_start + offset10] = cplxMul(amp10, shot.unitary[10]);
              }
              case OPID_CX {
                // Need to read all 4 to update the probabilities correctly, but only swap the |10> and |11> entries
                let amp00: vec2f = stateVector[params.shot_state_vector_start + offset00];
                let amp01: vec2f = stateVector[params.shot_state_vector_start + offset01];
                let amp10: vec2f = stateVector[params.shot_state_vector_start + offset10];
                let amp11: vec2f = stateVector[params.shot_state_vector_start + offset11];
                stateVector[params.shot_state_vector_start + offset10] = amp11;
                stateVector[params.shot_state_vector_start + offset11] = amp10;
                summed_probs[0] += (cplxMag2(amp00) + cplxMag2(amp01));
                summed_probs[1] += (cplxMag2(amp11) + cplxMag2(amp10));
                summed_probs[2] += (cplxMag2(amp00) + cplxMag2(amp11));
                summed_probs[3] += (cplxMag2(amp01) + cplxMag2(amp10));
              }
              default {
                // Assume OPID_SHOT_BUFF_2Q
                // Get the state vector entries
                let states = array<vec2f,4>(
                    stateVector[params.shot_state_vector_start + offset00],
                    stateVector[params.shot_state_vector_start + offset01],
                    stateVector[params.shot_state_vector_start + offset10],
                    stateVector[params.shot_state_vector_start + offset11]
                );
                // Apply the unitary from the shot buffer
                let result00 = innerProduct(getUnitaryRow(params.shot_idx, 0), states);
                let result01 = innerProduct(getUnitaryRow(params.shot_idx, 1), states);
                let result10 = innerProduct(getUnitaryRow(params.shot_idx, 2), states);
                let result11 = innerProduct(getUnitaryRow(params.shot_idx, 3), states);
                // Write back the results
                stateVector[params.shot_state_vector_start + offset00] = result00;
                stateVector[params.shot_state_vector_start + offset01] = result01;
                stateVector[params.shot_state_vector_start + offset10] = result10;
                stateVector[params.shot_state_vector_start + offset11] = result11;
                // Update the probabilities for the acted on qubits
                summed_probs[0] += (cplxMag2(result00) + cplxMag2(result01));
                summed_probs[1] += (cplxMag2(result10) + cplxMag2(result11));
                summed_probs[2] += (cplxMag2(result00) + cplxMag2(result10));
                summed_probs[3] += (cplxMag2(result01) + cplxMag2(result11));
              }
            }
        }

        entry_index += params.total_threads_per_shot;
    }

    // Update this thread's totals for the two qubits in the workgroup storage
    if (update_probs) {
        // Update all for other 2-qubit gates
        qubitProbabilities[tid].zero[op.q1] = summed_probs[0];
        qubitProbabilities[tid].one[op.q1]  = summed_probs[1];
        qubitProbabilities[tid].zero[op.q2] = summed_probs[2];
        qubitProbabilities[tid].one[op.q2]  = summed_probs[3];
    }

    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    if (tid == 0) {
        if (update_probs) {
            sum_thread_totals_to_shot(op.q1, params.shot_idx, params.workgroup_collation_idx);
            sum_thread_totals_to_shot(op.q2, params.shot_idx, params.workgroup_collation_idx);
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
            qubitProbabilities[tid].one[q] += prob;
        } else {
            qubitProbabilities[tid].zero[q] += prob;
        }
        mask = mask << 1u;
    }
}

fn sum_thread_totals_to_shot(q: u32, shot_idx: i32, wkg_collation_idx: i32) {
    var total_zero: f32 = 0.0;
    var total_one: f32 = 0.0;
    for (var j = 0; j < THREADS_PER_WORKGROUP; j++) {
        total_zero += qubitProbabilities[j].zero[q];
        total_one += qubitProbabilities[j].one[q];
    }
    if (wkg_collation_idx >= 0) {
        // Write to the workgroup collation buffer for later summation into the shot state
        workgroup_collation.sums[wkg_collation_idx].qubits[q] = vec2f(total_zero, total_one);
    } else {
        // Single workgroup per shot case - write directly to the shot state
        let within_threshold = abs(1.0 - (total_zero + total_one)) < PROB_THRESHOLD;
        if !within_threshold {
            // Populate the diagnostics buffer, if not already set
            let old_value = atomicCompareExchangeWeak(
                &diagnostics.error_code,
                0u,
                ERR_INVALID_THREAD_TOTAL);
            if old_value.exchanged {
                // This is the first error - fill in the details
                let shot = &shots[shot_idx];
                diagnostics.extra1 = q;
                diagnostics.extra2 = total_zero;
                diagnostics.extra3 = total_one;
                // DX12 backend has issues copying structs. See https://github.com/gfx-rs/wgpu/issues/8552
                // DX12-start-strip
                diagnostics.shot = *shot;
                diagnostics.op = ops[shot.op_idx];
                // DX12-end-strip
            }
            let err_index = (shot_idx + 1) * i32(RESULT_COUNT) - 1;
            atomicCompareExchangeWeak(
                    &results[err_index],
                    0u,
                    ERR_INVALID_THREAD_TOTAL);
        } else {
            shots[shot_idx].qubit_state[q].zero_probability = total_zero;
            shots[shot_idx].qubit_state[q].one_probability = total_one;
        }
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

// Performas a binary search on a correlated noise probability table
//
// Preconditions:
// - table is sorted ascending, with every entry higher than the prior
// - table entries are cumulative probabilities totaling <= 1.0
// - 'start' is the offset into the buffer array where this table's entries begin
// - 'count' is the number of entries in this table
// - 'rand_lo' and 'rand_hi' form a Q1.63 format random number in [0.0, 1.0) to use for the search
// - This will only called if a result should be found, i.e.,
//   - count > 0
//   - rand < table[start + count - 1].probability
//
// Returns the index of the found entry relative to 'start', which is the smallest index where "rand < table[start + index].probability"
fn binary_search_noise_table(rand_lo: u32, rand_hi: u32, start: i32, count: i32) -> i32 {
    var low: i32 = 0;
    var high: i32 = count;

    while (low < high) {
        let mid: i32 = low + (high - low) / 2;

        let p_lo = correlated_noise_entries[start + mid].probability_lo;
        let p_hi = correlated_noise_entries[start + mid].probability_hi;

        if (rand_hi < p_hi || (rand_hi == p_hi && rand_lo < p_lo)) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    return low;
}

// Hash and random number generation functions

// See https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/
// Use PCG hash function to generate a well-distributed hash from a simple integer input (e.g., shot id)
fn hash_pcg(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Returns a random u32 value based on the xorwow algorithm
fn next_rand_u32(shot_idx: u32) -> u32 {
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
    return t + rng_state.counter;
}

fn next_rand_f32(shot_idx: u32) -> f32 {
    let rand_u32: u32 = next_rand_u32(shot_idx);

    // Convert the 32 random bits to a float in the [0.0, 1.0) range

    // Keep only the lower 23 bits (the fraction portion of a float) with a 0 exponent biased to 127
    let rand_f32_bits = (rand_u32 & 0x7FFFFF) | (127 << 23);
    // Bitcast to an f32 in the [1.0, 2.0) range
    let f: f32 = bitcast<f32>(rand_f32_bits);
    // And decrement by 1 to return values from [0..1)
    return f - 1.0;
}
