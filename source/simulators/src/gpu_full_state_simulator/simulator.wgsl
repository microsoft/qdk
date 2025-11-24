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

const OPT_SKIP_DEFINITE_STATES = true; // Enable to skip processing state vector entries that are definitely 0.0 due to other qubits being in definite states
const OPT_PHASE_GATES = true;
// TODO
// - Turn S, S_Adj, T, T_Adj into Rz gates and optimize them by skipping one read and probability updates
// - Similarly, for Rzz skip 2 of the 4 state vector reads/writes and probability updates
// - CZ also only needs 1 state vector read/write out of 4 and no probability updates

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
                    // TODO: DX12 backend has issues assigning structs. So commenting out for now.
                    // diagnostics.shot = *shot;
                    // diagnostics.op = ops[shot.op_idx];
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
            // results[(shot_idx * RESULT_COUNT) + result_id] = 2i;
            shot.op_type = OPID_ID;
            shot.op_idx = op_idx;
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

// Get the duration-based noise operation for the given qubit in the shot. Note that this returns a
// matrix of real values (not complex values) stored in a vec4f.
// TOOD: This should be called and applied when T1 or T2 are != 0.0 (and != +inf) and the qubit isn't already lost
fn get_duration_noise(shot_idx: u32, qubit: u32) -> vec4f {
    let shot = &shots[shot_idx];
    let qstate = &shot.qubit_state[qubit];

    // TODO: Check the math when T1 = +inf (no amplitude damping) or T2 = 2 * T1 (no 'pure' dephasing)
    // NOTE: IEEE 754 specifies that division by +inf results in 0.0, and that +inf == +inf
    // NOTE: The WGSL spec states: "Implementations may assume that overflow, infinities, and NaNs are not present during shader execution."
    // NOTE: T1 or T2 should never be zero, as that would be instantaneous damping/dephasing

    // TODO: Need to store & retrieve T1 and T2 from the shot state or uniforms
    // t1 = relaxation time, t2 = dephasing time, t_theta = 'pure' dephasing time
    // t2 must be <= 2 * t1
    let t1 = 0.000003;
    let t2 = 0.000001; // t2 should be <= 2 * t1

    // TODO: Should duration be a u32 in some user-defined unit to avoid float precision issues over long durations?
    let duration = shot.duration;
    let time_idle = duration - qstate.idle_since;

    if (t1 == 0.0 && t2 == 0.0 || qstate.heat == -1.0 || time_idle <= 0.0 || duration <= 0.0) {
        // No noise to apply, or qubit is lost, or no time has passed
        return vec4f(1.0, 0.0, 0.0, 1.0);
    }

    // No amplitude damping noise (t1 == 0.0) means treat t1 as +inf, and 1/+inf = 0.0

    // We need to avoid infinities here, so handle the t2 == 2 * t1 case separately. We treat a value of 0.0 as +inf.
    let t_theta = select(1.0 / ((1.0 / t2) - (1 / (2 * t1))), 0.0, t2 >= 2 * t1);


    // TODO: Remember to reset the idle_since for the qubit when acted upon

    // Work through some concrete examples here, as the values can be so small we want to check it doesn't underflow a float (10**-38)
    // - Let idle time be in ns and is 100ns.
    // - Let T1 be (10_000ns) and T2 be (10_000ns), so T_theta = 1 / (1/10_000 - 1/(20_000)) = 1 / (0.0001 - 0.00005) = 1 / 0.00005 = 20_000ns
    // - Then p_damp = 1 - exp(-100 / 10_000) = 1 - exp(-0.01) = 1 - 0.99005 = 0.00995
    // - Then p_dephase = 1 - exp(-100 / 20_000) = 1 - exp(-0.005) = 1 - 0.99501 = 0.00499
    // - If T1 = 10 seconds or 10_000_000_000ns, then p_damp = 1 - exp(-100 / 10_000_000_000) = 1 - exp(-0.00000001) = 1 - 0.99999999 = 0.00000001
    //
    // Note: For very small x, exp(-x) ~= 1 - x, so if x is below some threshold, we may just want to return x * -1 instead of 1 - exp(x), else
    // the intermediate result may round to 1.0 - 1.0 in float precision.

    // Amplitude damping probability (0% if no T1 value specified)
    var p_damp: f32 = 0.0;
    if (t1 > 0.0) {
        let x = time_idle / t1;
        if (x < 0.00001) {
            p_damp = x; // Use linear approximation for very small x
        } else {
            p_damp = 1.0 - exp(-x);
        }
    }

    var p_dephase: f32 = 0.0;
    if (t_theta > 0.0) {
        let x = time_idle / t_theta;
        if (x < 0.00001) {
            p_dephase = x; // Use linear approximation for very small x
        } else {
            p_dephase = 1.0 - exp(-x);
        }
    }

    let rand = shot.rand_damping; // TODO: Guess we don't need shot.rand_dephase?

    // The combined 'thermal relaxation' noise matricies are:
    //   - AP0: [[1, 0], [0, sqrt(1 - p_damp - p_dephase)]]
    //   - AP1: [[0, sqrt(p_damp)], [0, 0]]
    //   - AP2: [[0, 0], [0, sqrt(p_dephase)]]
    // Work backwards, defaulting to AP0 if AP2 or AP1 aren't selected
    // AP0 will just be the identity matrix if there's no damping or dephasing

    let p_ap2 = p_dephase * qstate.one_probability;
    let p_ap1 = p_damp * qstate.one_probability;

    if (rand < p_ap2) {
        // Return AP2 with renormalization to bring state vector back to norm 1.0
        return vec4f(0.0, 0.0, 0.0, 1.0 / sqrt(qstate.one_probability));
    } else if (rand < (p_ap2 + p_ap1)) {
        // Return AP1 with renormalization to bring state vector back to norm 1.0
        return vec4f(0.0, 1.0 / sqrt(qstate.one_probability), 0.0, 0.0);
    } else {
        // Return AP0
        // Entry (1,1) needs to scale down, then renormalize both back up so total probability is norm 1.0
        let new_1_1_scale = sqrt(1.0 - p_damp - p_dephase);
        let renorm = 1.0 / sqrt(qstate.zero_probability + qstate.one_probability * new_1_1_scale * new_1_1_scale);
        return vec4f(renorm, 0.0, 0.0, new_1_1_scale * renorm);
    }
}

fn apply_1q_pauli_noise(shot_idx: u32, op_idx: u32, noise_idx: u32) {
    // NOTE: Assumes that whatever prepared the program ensured that noise_op.q1 matches op.q1 and
    // that op is a 1-qubit gate
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let noise_op = &ops[noise_idx];

    // Apply 1-qubit Pauli noise based on the probabilities in the op data, which are stored in
    // the real part (x) of the first 3 vec2 entries of the unitary array.
    let p_x = noise_op.unitary[0].x;
    let p_y = noise_op.unitary[1].x;
    let p_z = noise_op.unitary[2].x;

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
        if (OPT_PHASE_GATES && is_1q_phase_gate(op.id)) {
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
        shot.op_type = OPID_SHOT_BUFF_2Q;
    } else {
        // No noise to apply. Leave if CX or CZ  or RZZ as they get handled specially in execute_op
        if (op.id == OPID_CX || op.id == OPID_CZ || (OPT_PHASE_GATES && op.id == OPID_RZZ)) {
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
        return;
    }

    let op = &ops[op_idx];

    shot_init_per_op(shot_idx);
    shot.unitary = op.unitary;

    // Update the shot state based on the results of the last executed op (if needed)
    if (shot.qubits_updated_last_op_mask != 0) {
        update_qubit_state(shot_idx);
    }

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
    if (op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ || op.id == OPID_MAT2Q || op.id == OPID_SWAP) {
        shot.op_type = OPID_SHOT_BUFF_2Q; // Indicate to use the matrix in the shot buffer
    }

    if (op.id > OPID_RESET && op.id < OPID_CX) {
        shot.op_type = OPID_SHOT_BUFF_1Q; // Indicate to use the matrix in the shot buffer
    }

    if (OPT_PHASE_GATES && is_1q_phase_gate(op.id)) {
        // For phase gates, treat everything as RZ for execution purposes
        shot.op_type = OPID_RZ;
    }

    if (OPT_PHASE_GATES && op.id == OPID_RZZ) {
        // If optimization phase gates, Rzz is special
        shot.op_type = OPID_RZZ;
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

    // Workgroups are per shot if 22 or less qubits, else 2 workgroups for 23 qubits, 4 for 24, etc..
    let shot = &shots[params.shot_idx];
    let op_idx = shot.op_idx;
    let op = &ops[op_idx];

    // Skip doing any work if the op is ID (no-op) or if renormalize is 1.0 (i.e., no renormalization needed)
    if shot.op_type != OPID_ID {
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
            let skip_processing = OPT_SKIP_DEFINITE_STATES &&
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

                    let new0 = cplxMul(amp0, shot.unitary[0]) + cplxMul(amp1, shot.unitary[1]);
                    let new1 = cplxMul(amp0, shot.unitary[4]) + cplxMul(amp1, shot.unitary[5]);

                    stateVector[params.shot_state_vector_start + offset0] = new0;
                    stateVector[params.shot_state_vector_start + offset1] = new1;

                    // Update the probabilities for the acted on qubit
                    summed_probs[0] += cplxMag2(new0);
                    summed_probs[1] += cplxMag2(new1);
                }
            }
            entry_index += params.total_threads_per_shot;
        }

        if shot.op_type != OPID_RZ {
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
    if (tid == 0 && shot.op_type != OPID_RZ && shot.op_type != OPID_ID) {
        sum_thread_totals_to_shot(op.q1, params.shot_idx, params.workgroup_collation_idx);
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

    for (var i = 0; i < params.op_iterations; i++) {
        // q1 is the control, q2 is the target
        let offset00: i32 = (entry_index & lowMask) | ((entry_index & midMask) << 1) | ((entry_index & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << op.q2);
        let offset10: i32 = offset00 | (1 << op.q1);
        let offset11: i32 = offset10 | (1 << op.q2);

        let can_skip_processing = OPT_SKIP_DEFINITE_STATES &&
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
    if (shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ) {
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
        if (shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ) {
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

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute_mz(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Get the params
    let params = get_shot_params(workgroupId.x, tid, 1);

    let shot = &shots[params.shot_idx];
    let op_idx = shot.op_idx;
    let op = &ops[op_idx];
    let qubit = op.q1;

    let lowMask = (1 << qubit) - 1;
    let highMask = (1 << u32(QUBIT_COUNT)) - 1 - lowMask;

    let qubit_is_0 = i32(shot.qubit_is_0_mask);
    let qubit_is_1 = i32(shot.qubit_is_1_mask);

    let scale = shot.renormalize;

    var entry_index = params.thread_idx_in_shot;

    for (var i = 0; i < params.op_iterations; i++) {
        let offset0: i32 = (entry_index & lowMask) | ((entry_index & highMask) << 1);
        let offset1: i32 = offset0 | (1 << qubit);

        // See if we can skip doing any work for this pair, because the state vector entries to processes
        // are both definitely 0.0, as we know they are for states where other qubits are in definite opposite state.
        let skip_processing = OPT_SKIP_DEFINITE_STATES &&
            ((offset0 & qubit_is_0) != 0) ||
            ((~offset1 & qubit_is_1) != 0);

        if (!skip_processing) {
            let amp0: vec2f = stateVector[params.shot_state_vector_start + offset0];
            let amp1: vec2f = stateVector[params.shot_state_vector_start + offset1];

            let new0 = scale * (cplxMul(amp0, shot.unitary[0]) + cplxMul(amp1, shot.unitary[1]));
            let new1 = scale * (cplxMul(amp0, shot.unitary[4]) + cplxMul(amp1, shot.unitary[5]));

            stateVector[params.shot_state_vector_start + offset0] = new0;
            stateVector[params.shot_state_vector_start + offset1] = new1;

            update_all_qubit_probs(u32(offset0), new0, tid);
            update_all_qubit_probs(u32(offset1), new1, tid);
        }
        entry_index += params.total_threads_per_shot;
    }

    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    if (tid == 0) {
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (shot.qubits_updated_last_op_mask & (1u << q)) != 0u {
                sum_thread_totals_to_shot(q, params.shot_idx, params.workgroup_collation_idx);
            }
        }
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
                // DX12 backend has issues copying structs. Add back once we have a solution.
                // diagnostics.shot = *shot;
                // diagnostics.op = ops[shot.op_idx];
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
