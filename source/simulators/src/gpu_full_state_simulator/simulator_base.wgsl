// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// common.wgsl is appended to the beginning of this file at runtime.

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
    termination_count: atomic<u32>,
    extra1: u32,
    extra2: f32,
    extra3: f32,
    _padding: u32,
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

fn get_measure_qubit(shot_idx: u32, op_idx: u32) -> u32 {
    return ops[op_idx].q1;
}

fn get_measure_result(shot_idx: u32, op_idx: u32) -> u32 {
    return ops[op_idx].q2;
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

    // Handle MResetZ, MZ, and ResetZ operations. These have unique handling and no associated noise ops, so prep and exit
    if (op.id == OPID_MRESETZ) {
        prep_measure_reset(shot_idx, op_idx, false /* is_loss */, true /* stores_result */, true /* resets_to_zero */);
        shot.next_op_idx = op_idx + 1u; // No associated noise ops, so just advance by 1
        return;
    }
    if (op.id == OPID_MZ) {
        prep_measure_reset(shot_idx, op_idx, false /* is_loss */, true /* stores_result */, false /* resets_to_zero */);
        shot.next_op_idx = op_idx + 1u;
        return;
    }
    if (op.id == OPID_RESETZ) {
        prep_measure_reset(shot_idx, op_idx, false /* is_loss */, false /* stores_result */, true /* resets_to_zero */);
        shot.next_op_idx = op_idx + 1u;
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
         (op.id == OPID_CX || op.id == OPID_CY || op.id == OPID_CZ || op.id == OPID_SWAP || op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ || op.id == OPID_MAT2Q) &&
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
            // (stores_result is irrelevant here since the is_loss path never stores a result)
            prep_measure_reset(shot_idx, op_idx, true /* is_loss */, false /* stores_result */, true /* resets_to_zero */);
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

    if (op.id >= OPID_X && op.id < OPID_CX) {
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
            case OPID_CX, OPID_CY, OPID_SHOT_BUFF_2Q {
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
fn execute(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    let shot_idx: i32 = i32(workgroupId.x) / WORKGROUPS_PER_SHOT;
    let shot = &shots[shot_idx];

    // If it's an ID gate, or a pure phase gate (including CZ) then probabilities don't need updating
    // Correlated noise also updates probabilities in prepare_op, so can skip doing that here
    let update_probs = shot.op_type != OPID_ID && shot.op_type != OPID_CORRELATED_NOISE &&
            shot.op_type != OPID_RZ && shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ;

    if (shot.op_type == OPID_ID) {
        // IGNORE
    } else if (shot.op_type == OPID_CORRELATED_NOISE) {
        apply_correlated_noise(workgroupId.x, tid);
    } else if (is_1q_op(shot.op_type)) {
        let q1: u32 = ops[shot.op_idx].q1;
        apply_1q_op(workgroupId.x, tid, q1);
    } else /* 2 qubit op */ {
        let q1: u32 = ops[shot.op_idx].q1;
        let q2: u32 = ops[shot.op_idx].q2;
        apply_2q_op(workgroupId.x, tid, q1, q2);
    }

    // workgroupBarrier can't be conditional in DX12 backend, so we have to do an unconditional one here
    // outside of the skip_work conditional above.
    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    // Skip for correlated noise since probabilities were already updated in prepare_op.
    if (tid == 0 && update_probs) {
        let shot_idx: i32 = i32(workgroupId.x) / WORKGROUPS_PER_SHOT;
        let workgroup_collation_idx: i32 = select(-1, i32(workgroupId.x), WORKGROUPS_PER_SHOT > 1);
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (shot.qubits_updated_last_op_mask & (1u << q)) != 0u {
                sum_thread_totals_to_shot(q, shot_idx, workgroup_collation_idx);
            }
        }
    }
}
