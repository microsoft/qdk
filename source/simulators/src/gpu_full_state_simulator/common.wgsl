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

// Tolerance for probabilities to sum to 1.0
const PROB_THRESHOLD: f32 = 0.0001;

// Always use 32 threads per workgroup for max concurrency on most current GPU hardware
const MAX_WORKGROUP_SUM_PARTITIONS: i32 = 1 << u32(MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);

// Operation IDs
const OPID_ID      = 0u;
const OPID_RESETZ  = 1u;
const OPID_X       = 2u;
const OPID_Y       = 3u;
const OPID_Z       = 4u;
const OPID_H       = 5u;
const OPID_S       = 6u;
const OPID_SAdj    = 7u;
const OPID_T       = 8u;
const OPID_TAdj    = 9u;
const OPID_RX      = 12u;
const OPID_RY      = 13u;
const OPID_RZ      = 14u;
const OPID_CX      = 15u;
const OPID_CZ      = 16u;
const OPID_RXX     = 17u;
const OPID_RYY     = 18u;
const OPID_RZZ     = 19u;
const OPID_MZ      = 21u;
const OPID_MRESETZ = 22u;
const OPID_SWAP    = 24u;
const OPID_MAT1Q   = 25u;
const OPID_MAT2Q   = 26u;
const OPID_CY      = 29u;

const OPID_PAULI_NOISE_1Q = 128u;
const OPID_PAULI_NOISE_2Q = 129u;
const OPID_LOSS_NOISE = 130u;
const OPID_CORRELATED_NOISE = 131u;

// If the application of noise results in a custom matrix, it will have been stored in the shot buffer
// These OPIDs indicate to use that matrix and for how many qubits. (The qubit ids are in the original Op)
const OPID_SHOT_BUFF_1Q = 256u;
const OPID_SHOT_BUFF_2Q = 257u;

struct WorkgroupSums {
    qubits: array<vec2f, MAX_QUBIT_COUNT>, // Each vec2f holds (zero_probability, one_probability)
};

struct WorkgroupCollationBuffer {
    sums: array<WorkgroupSums, MAX_WORKGROUP_SUM_PARTITIONS>,
};

fn is_1q_phase_gate(op_id: u32) -> bool {
    return (op_id == OPID_S || op_id == OPID_SAdj || op_id == OPID_T || op_id == OPID_TAdj || op_id == OPID_RZ);
}

fn is_1q_op(op_id: u32) -> bool {
    return ((op_id >= OPID_ID && op_id <= OPID_RZ) ||
        op_id == OPID_MZ || op_id == OPID_MRESETZ ||
        op_id == OPID_MAT1Q || op_id == OPID_SHOT_BUFF_1Q);
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
    // Reserved draw: qubit loss is now sampled from the combined `rand_pauli`
    // distribution rather than its own value, but we still advance the RNG by
    // one draw here to keep the per-op random stream (and thus seeded results)
    // identical to the previous loss model.
    next_rand_f32(shot_idx);
}

// Resets the entire shot state, including RNG, probabilities, and per-qubit tracking.
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
    shot.pending_loss_mask = 0u;

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

// Build a measure-and-reset (or measure-only) instrument for `qubit` given a
// measured `result`, store it in the shot buffer, set up renormalization, and
// mark the qubit as no longer in a definite basis state so the execute stage
// recomputes its probabilities. Shared by `prep_measure_reset` and
// `prep_loss_commit`; the caller sets `shot.op_idx` and `shot.op_type`.
fn prep_measure_reset_instrument(shot_idx: u32, qubit: u32, result: u32, resets_to_zero: bool) {
    let shot = &shots[shot_idx];

    // Construct the measurement/reset instrument based on the measured result
    // Put the instrument into the shot buffer for the execute_op stage to apply
    if resets_to_zero {
        // Reset variants (MResetZ, ResetZ):
        // Result=0: [[1,0],[0,0]] - project onto |0⟩ (already there)
        // Result=1: [[0,1],[0,0]] - swap |1⟩ into |0⟩ slot (reset)
        shot.unitary[0] = select(vec2f(1.0, 0.0), vec2f(0.0, 0.0), result == 1u);
        shot.unitary[1] = select(vec2f(0.0, 0.0), vec2f(1.0, 0.0), result == 1u);
        shot.unitary[4] = vec2f();
        shot.unitary[5] = vec2f();
    } else {
        // Measure-only (MZ):
        // Result=0: [[1,0],[0,0]] - project onto |0⟩
        // Result=1: [[0,0],[0,1]] - project onto |1⟩ (keep in place)
        shot.unitary[0] = select(vec2f(1.0, 0.0), vec2f(0.0, 0.0), result == 1u);
        shot.unitary[1] = vec2f();
        shot.unitary[4] = vec2f();
        shot.unitary[5] = select(vec2f(0.0, 0.0), vec2f(1.0, 0.0), result == 1u);
    }

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
        // A mask with all qubits set
        ((1u << u32(QUBIT_COUNT)) - 1u)
        // Exclude qubits already in definite states
            & ~(shot.qubit_is_0_mask | shot.qubit_is_1_mask);
}

fn prep_measure_reset(shot_idx: u32, op_idx: u32, is_loss: bool, stores_result: bool, resets_to_zero: bool) {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let qubit = get_measure_qubit(shot_idx, op_idx);

    // Choose measurement result based on qubit probabilities and random number
    let result = select(1u, 0u, shot.rand_measure < shot.qubit_state[qubit].zero_probability);

    // If this is being called due to loss noise, we don't write the result back to the results buffer
    // Instead, mark the qubit as lost by setting the heat to -1.0
    if !is_loss {
        if stores_result {
            let result_id = get_measure_result(shot_idx, op_idx); // Result id to store the measurement result in is stored in q2

            // If the qubit is already marked as lost, just report that and exit. It's already in the zero
            // state so nothing to update or renormalize. The execute op should be a no-op (ID)
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
            // No result to store (e.g. ResetZ). If the qubit is lost, it's already in the zero
            // state so nothing to update. Just set to ID and return.
            if shot.qubit_state[qubit].heat == -1.0 {
                shot.op_type = OPID_ID;
                shot.op_idx = op_idx;
                return;
            }
        }
    } else {
        shot.qubit_state[qubit].heat = -1.0;
    }

    prep_measure_reset_instrument(shot_idx, qubit, result, resets_to_zero);

    shot.op_idx = op_idx;
    // Use OPID_MRESETZ as the op_type for all three variants in execute stage
    // (they all use the same matrix-apply + update_all_qubit_probs path)
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

// Loss policy values. These are stamped onto a gate op's `q3` field by the host
// (see `LossPolicy::as_u32` on the Rust side) and tell the shader how to handle
// the gate when one of its operands is lost. `0` means "no policy stamped",
// which the shader treats the same as SKIP.
const LOSS_POLICY_SKIP              = 0u;
const LOSS_POLICY_PROPAGATE         = 1u;
const LOSS_POLICY_DEGRADE           = 2u;
const LOSS_POLICY_RESIDUAL_S_DAGGER = 3u;
const LOSS_POLICY_APPLY_ANYWAY      = 4u;

// Returns true if the gate at `op_idx` touches at least one lost qubit.
// `q1`/`q2` are the (resolved) operands of the gate.
fn gate_has_lost_operand(shot_idx: u32, op_idx: u32, q1: u32, q2: u32) -> bool {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    if (shot.qubit_state[q1].heat == -1.0) {
        return true;
    }
    let is_2q = !is_1q_op(op.id);
    return is_2q && (shot.qubit_state[q2].heat == -1.0);
}

// Builds a 4x4 (in shot.unitary) that applies the 1-qubit matrix `m` (given as
// m00,m01,m10,m11) to `target_is_q2 ? q2 : q1` and identity to the other qubit
// of the pair. The lost qubit is in the |0> state, so the identity factor keeps
// it there. The 2-qubit basis is |q1 q2>, so the row/col index is
// (2 * q1_bit + q2_bit).
fn set_1q_on_pair_unitary(shot_idx: u32, target_is_q2: bool,
                          m00: vec2f, m01: vec2f, m10: vec2f, m11: vec2f) {
    let shot = &shots[shot_idx];
    // Zero the whole 4x4 first.
    for (var i = 0u; i < 16u; i++) {
        shot.unitary[i] = vec2f(0.0, 0.0);
    }
    if target_is_q2 {
        // Acts on q2 (low bit): block-diagonal diag(M, M).
        // Top-left block (q1 = 0):
        shot.unitary[0]  = m00; shot.unitary[1]  = m01;
        shot.unitary[4]  = m10; shot.unitary[5]  = m11;
        // Bottom-right block (q1 = 1):
        shot.unitary[10] = m00; shot.unitary[11] = m01;
        shot.unitary[14] = m10; shot.unitary[15] = m11;
    } else {
        // Acts on q1 (high bit): M (x) I.
        shot.unitary[0]  = m00; shot.unitary[2]  = m01;
        shot.unitary[8]  = m10; shot.unitary[10] = m11;
        shot.unitary[5]  = m00; shot.unitary[7]  = m01;
        shot.unitary[13] = m10; shot.unitary[15] = m11;
    }
}

// Multiplies one row of the 4x4 pair unitary (in shot.unitary) by -i, in place.
// Folding a diag(1, -i) = S-dagger factor on one qubit into a 2-qubit matrix
// scales the rows whose target-qubit bit is 1 by -i. For a complex entry
// (x + y i), (x + y i) * -i = y - x i.
fn scale_pair_unitary_row_by_neg_i(shot_idx: u32, row: u32) {
    let shot = &shots[shot_idx];
    for (var c = 0u; c < 4u; c++) {
        let e = shot.unitary[row * 4u + c];
        shot.unitary[row * 4u + c] = vec2f(e.y, -e.x);
    }
}

// Sets up the shot to execute a 2-qubit shot-buffer op on the gate's operands.
fn finish_2q_shot_buffer(shot_idx: u32, op_idx: u32, q1: u32, q2: u32) {
    let shot = &shots[shot_idx];
    shot.op_idx = op_idx;
    shot.op_type = OPID_SHOT_BUFF_2Q;
    shot.qubits_updated_last_op_mask = (1u << q1) | (1u << q2);
}

// Loses a single surviving `qubit` for the PROPAGATE policy: samples a
// measurement outcome, collapses the qubit to that outcome and resets it to
// |0>, and marks it lost (heat = -1.0). The collapse is expressed as a 2-qubit
// tensor on the gate's operands (reset on `qubit`, identity on the lost
// partner, which is already in |0>), reusing the standard shot-buffer execute
// path. `qubit` must be one of the gate's two operands `q1`/`q2`.
fn propagate_loss_to_qubit(shot_idx: u32, op_idx: u32, q1: u32, q2: u32, qubit: u32) {
    let shot = &shots[shot_idx];

    let result = select(1u, 0u, shot.rand_measure < shot.qubit_state[qubit].zero_probability);

    // Reset instrument (project + move |1> into |0> slot), same as MResetZ:
    //   result==0: [[1,0],[0,0]]
    //   result==1: [[0,1],[0,0]]
    let m00 = select(vec2f(1.0, 0.0), vec2f(0.0, 0.0), result == 1u);
    let m01 = select(vec2f(0.0, 0.0), vec2f(1.0, 0.0), result == 1u);
    let m10 = vec2f(0.0, 0.0);
    let m11 = vec2f(0.0, 0.0);

    let target_is_q2 = (qubit == q2);
    set_1q_on_pair_unitary(shot_idx, target_is_q2, m00, m01, m10, m11);

    // Renormalize by the measured branch probability.
    shot.renormalize = select(
        1.0 / sqrt(shot.qubit_state[qubit].zero_probability),
        1.0 / sqrt(shot.qubit_state[qubit].one_probability),
        result == 1u);

    // Mark the qubit lost and clear its definite-state bits so the probability
    // pass recomputes it.
    shot.qubit_state[qubit].heat = -1.0;
    shot.qubit_is_0_mask = shot.qubit_is_0_mask & ~(1u << qubit);
    shot.qubit_is_1_mask = shot.qubit_is_1_mask & ~(1u << qubit);

    finish_2q_shot_buffer(shot_idx, op_idx, q1, q2);
}

// Handles a gate whose operand(s) include at least one lost qubit, according to
// the loss policy stamped on the op's `q3` field. `q1`/`q2` are the (resolved)
// operands. Returns true if the gate was fully handled here (the caller should
// return), or false if normal processing should continue (only for
// APPLY_ANYWAY, which runs the gate as usual).
fn handle_lost_operand_policy(shot_idx: u32, op_idx: u32, q1: u32, q2: u32) -> bool {
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let is_1q = is_1q_op(op.id);
    let is_2q = !is_1q;
    let policy = op.policy;

    // Loss policies only make sense for multi-qubit gates.
    // If this is a single-qubit gate, skip it entirely.
    if (is_1q) {
        shot.op_type = OPID_ID;
        shot.op_idx = op_idx;
        return true;
    }

    let q1_lost = shot.qubit_state[q1].heat == -1.0;
    let q2_lost = is_2q && (shot.qubit_state[q2].heat == -1.0);
    let has_survivor = is_2q && !(q1_lost && q2_lost);
    // The surviving operand (only meaningful when has_survivor is true).
    let survivor = select(q1, q2, q1_lost);
    let survivor_is_q2 = q1_lost;

    // SWAP is special: it physically relocates the two qubits, so their loss
    // state is always exchanged regardless of the policy (the policy only
    // governs whether the unitary runs). Handle it explicitly here.
    if (op.id == OPID_SWAP) {
        switch policy {
            case LOSS_POLICY_PROPAGATE {
                propagate_loss_to_qubit(shot_idx, op_idx, q1, q2, survivor);
                return true;
            }
            case LOSS_POLICY_RESIDUAL_S_DAGGER {
                // Match the CPU/stabilizer SWAP + residual S-dagger semantics:
                //   1. Apply the full SWAP (shot.unitary already holds it).
                //   2. Apply S-dagger = diag(1, -i) to the (originally) lost
                //      operand's position, which after the SWAP holds the
                //      survivor's amplitudes.
                //   3. Exchange the per-qubit loss flag (heat) of the operands.

                // Fold the S-dagger into the SWAP matrix by scaling, by -i, the
                // two rows of the |q1 q2> pair matrix whose lost-qubit bit is 1.
                // q1 is the high bit (rows 2, 3); q2 is the low bit (rows 1, 3).
                let lost_row = select(1u, 2u, q1_lost);
                scale_pair_unitary_row_by_neg_i(shot_idx, lost_row);
                scale_pair_unitary_row_by_neg_i(shot_idx, 3u);
                // Exchange the per-qubit loss flag (heat) of the two operands.
                let heat1 = shot.qubit_state[q1].heat;
                shot.qubit_state[q1].heat = shot.qubit_state[q2].heat;
                shot.qubit_state[q2].heat = heat1;
                // The 2-qubit execute path skips amplitudes for qubits known to be
                // in a definite state, which would skip the amplitudes SWAP needs to move.
                // Clear those bits for both operands so the swap is actually applied.
                shot.qubit_is_0_mask = shot.qubit_is_0_mask & ~((1u << q1) | (1u << q2));
                shot.qubit_is_1_mask = shot.qubit_is_1_mask & ~((1u << q1) | (1u << q2));
                // shot.unitary now holds (S-dagger on lost) * SWAP.
                finish_2q_shot_buffer(shot_idx, op_idx, q1, q2);
                return true;
            }
            case LOSS_POLICY_APPLY_ANYWAY {
                // Exchange the per-qubit loss flag (heat) of the two operands.
                let heat1 = shot.qubit_state[q1].heat;
                shot.qubit_state[q1].heat = shot.qubit_state[q2].heat;
                shot.qubit_state[q2].heat = heat1;
                // The 2-qubit execute path skips amplitudes for qubits known to be
                // in a definite state, which would skip the amplitudes SWAP needs to move.
                // Clear those bits for both operands so the swap is actually applied.
                shot.qubit_is_0_mask = shot.qubit_is_0_mask & ~((1u << q1) | (1u << q2));
                shot.qubit_is_1_mask = shot.qubit_is_1_mask & ~((1u << q1) | (1u << q2));
                // shot.unitary already holds the SWAP matrix (set by the caller).
                finish_2q_shot_buffer(shot_idx, op_idx, q1, q2);
                return true;
            }
            default {
                shot.op_type = OPID_ID;
                shot.op_idx = op_idx;
                return true;
            }
        }
    }

    // APPLY_ANYWAY: run the gate as if nothing was lost.
    if (policy == LOSS_POLICY_APPLY_ANYWAY) {
        return false;
    }

    if (policy == LOSS_POLICY_PROPAGATE && has_survivor) {
        propagate_loss_to_qubit(shot_idx, op_idx, q1, q2, survivor);
        return true;
    }

    if (policy == LOSS_POLICY_RESIDUAL_S_DAGGER && has_survivor) {
        // Apply S-dagger = diag(1, -i) to the surviving operand.
        set_1q_on_pair_unitary(shot_idx, survivor_is_q2,
            vec2f(1.0, 0.0), vec2f(0.0, 0.0),
            vec2f(0.0, 0.0), vec2f(0.0, -1.0));
        finish_2q_shot_buffer(shot_idx, op_idx, q1, q2);
        return true;
    }

    if (policy == LOSS_POLICY_DEGRADE && has_survivor &&
        (op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ)) {
        // Degrade the two-qubit rotation to its single-qubit version on the
        // survivor. The op's unitary[0] holds cos(θ/2) for Rxx/Ryy; we recover
        // the angle to build the 1-qubit rotation matrix.
        let cos_half = op.unitary[0].x;
        if (op.id == OPID_RXX) {
            // Rx(θ) = [[c, -i s], [-i s, c]], where s = sin(θ/2).
            let s = op.unitary[3].y * -1.0; // unitary[3] = (0, -sin(θ/2))
            set_1q_on_pair_unitary(shot_idx, survivor_is_q2,
                vec2f(cos_half, 0.0), vec2f(0.0, -s),
                vec2f(0.0, -s), vec2f(cos_half, 0.0));
        } else if (op.id == OPID_RYY) {
            // Ry(θ) = [[c, -s], [s, c]], where s = sin(θ/2).
            let s = op.unitary[3].y; // unitary[3] = (0, sin(θ/2)) for Ryy
            set_1q_on_pair_unitary(shot_idx, survivor_is_q2,
                vec2f(cos_half, 0.0), vec2f(-s, 0.0),
                vec2f(s, 0.0), vec2f(cos_half, 0.0));
        } else {
            // Rzz -> Rz(θ). The GPU Rz convention is [[1, 0], [0, e^{iθ}]],
            // and unitary[5] = e^{iθ} holds the full-angle phase.
            let phase = op.unitary[5];
            set_1q_on_pair_unitary(shot_idx, survivor_is_q2,
                vec2f(1.0, 0.0), vec2f(0.0, 0.0),
                vec2f(0.0, 0.0), phase);
        }
        finish_2q_shot_buffer(shot_idx, op_idx, q1, q2);
        return true;
    }

    // SKIP (and any policy with no applicable survivor, e.g. DEGRADE on a
    // controlled gate, or a single-qubit gate): skip the gate entirely.
    shot.op_type = OPID_ID;
    shot.op_idx = op_idx;
    return true;
}

fn apply_1q_pauli_noise(shot_idx: u32, op_idx: u32, noise_idx: u32) {
    // NOTE: Assumes that whatever prepared the program ensured that noise_op.q1 matches op.q1 and
    // that op is a 1-qubit gate
    let shot = &shots[shot_idx];
    let op = &ops[op_idx];
    let noise_op = &ops[noise_idx];

    // Categorical outcome probabilities by 3-bit term (X=1, Z=2, Y=3, L=4),
    // stored at flat slot k = term in `unitary[k / 2][k % 2]`. The identity
    // outcome (slot 0) is implicit.
    let p_x = noise_op.unitary[0].y;
    let p_z = noise_op.unitary[1].x;
    let p_y = noise_op.unitary[1].y;
    let p_loss = noise_op.unitary[2].x;

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
        // Either loss or no noise: the gate executes unmodified. If loss was
        // sampled, schedule a loss commit for this qubit; a following
        // loss-commit op performs the measure + reset.
        if (rand < (p_x + p_z + p_y + p_loss)) {
            shot.pending_loss_mask |= (1u << op.q1);
        }
        // No noise. Set the op_type back to the op.id value if it's Id, MResetZ, MZ, or ResetZ, as they get handled specially in execute_op
        if (op.id == OPID_ID || op.id == OPID_MRESETZ || op.id == OPID_MZ || op.id == OPID_RESETZ) {
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

    // The categorical distribution over the 25 (q1_term, q2_term) outcomes is
    // stored at flat slot k = q1_term * 5 + q2_term in `unitary[k / 2][k % 2]`.
    // Terms use the 3-bit encoding: I=0, X=1, Z=2, Y=3, L=4. The II slot (0) is
    // implicit and carries the remaining probability.
    var rand = shot.rand_pauli;
    var q1_term = 0;
    var q2_term = 0;

    // Find the terms to apply based on the random number and the probabilities
    for (var a = 0; a < 5; a = a + 1) {
        for (var b = 0; b < 5; b = b + 1) {
            let k = a * 5 + b;
            if (k == 0) { continue; } // II carries no stored probability
            let slot = noise_op.unitary[k / 2];
            let p_ab = select(slot.x, slot.y, (k & 1) == 1);
            if (rand < p_ab) {
                q1_term = a;
                q2_term = b;
                // Break out of both loops
                a = 5;
                b = 5;
            } else {
                rand = rand - p_ab;
            }
        }
    }

    // Schedule loss commits for any qubit whose sampled term is loss (L = 4).
    // A following loss-commit op performs the measure + reset.
    if (q1_term == 4) { shot.pending_loss_mask |= (1u << op.q1); }
    if (q2_term == 4) { shot.pending_loss_mask |= (1u << op.q2); }

    // A Pauli fault (X, Z, Y = 1, 2, 3) is fused into the gate by permuting its
    // rows. Loss (4) and identity (0) leave the gate unmodified for that qubit.
    let q1_pauli = q1_term >= 1 && q1_term <= 3;
    let q2_pauli = q2_term >= 1 && q2_term <= 3;

    if (q1_pauli || q2_pauli) {
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
        if (q1_term == 1) {
            // Apply the X permutation
            let old_row_0 = op_row_0;
            let old_row_1 = op_row_1;
            op_row_0 = op_row_2;
            op_row_1 = op_row_3;
            op_row_2 = old_row_0;
            op_row_3 = old_row_1;
        } else if (q1_term == 3) {
            // Apply the Y permutation
            let old_row_0 = op_row_0;
            let old_row_1 = op_row_1;
            op_row_0 = rowNeg(op_row_2);
            op_row_1 = rowNeg(op_row_3);
            op_row_2 = old_row_0;
            op_row_3 = old_row_1;
        } else if (q1_term == 2) {
            // Apply Z permutation
            op_row_2 = rowNeg(op_row_2);
            op_row_3 = rowNeg(op_row_3);
        }
        // Apply the q2 permutations as needed
        if (q2_term == 1) {
            // Apply the X permutation
            let old_row_0 = op_row_0;
            let old_row_2 = op_row_2;
            op_row_0 = op_row_1;
            op_row_2 = op_row_3;
            op_row_1 = old_row_0;
            op_row_3 = old_row_2;
        } else if (q2_term == 3) {
            // Apply the Y permutation
            let old_row_0 = op_row_0;
            let old_row_2 = op_row_2;
            op_row_0 = rowNeg(op_row_1);
            op_row_2 = rowNeg(op_row_3);
            op_row_1 = old_row_0;
            op_row_3 = old_row_2;
        } else if (q2_term == 2) {
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
        // No Pauli fault to fuse (identity or loss only). Leave if CX, CY, CZ, or RZZ as they get handled specially in execute_op
        if (op.id == OPID_CX || op.id == OPID_CY || op.id == OPID_CZ || op.id == OPID_RZZ) {
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

fn apply_1q_op(workgroupId: u32, tid: u32, q1: u32) {
    let params = get_shot_params(workgroupId, tid, 1 /* qubits per op */);
    let shot = &shots[params.shot_idx];
    let scale = shot.renormalize;
    let lowMask = (1 << q1) - 1;
    let highMask = (1 << u32(QUBIT_COUNT)) - 1 - lowMask;
    let qubit_is_0_mask = i32(shots[params.shot_idx].qubit_is_0_mask);
    let qubit_is_1_mask = i32(shots[params.shot_idx].qubit_is_1_mask);

    var summed_probs: vec4f = vec4f();

    /* This loop is where all the real work happens. Try to keep this tight and efficient.

    We want a 'structure of arrays' like access pattern here for efficiency, so we process the state vector
    in blocks where each thread in the workgroup(s) handle an adjacent entry to be processed.

    Each thread should start at the state vector shot start + 'thread_idx_in_shot', which is sequential across the workgroup threads
    Each next entry for the thread is WORKGROUPS_PER_SHOT * THREADS_PER_WORKGROUP away.
    */
    var entry_index = params.thread_idx_in_shot;

    for (var i = 0; i < params.op_iterations; i++) {
        let offset0: i32 = (entry_index & lowMask) | ((entry_index & highMask) << 1);
        let offset1: i32 = offset0 | (1 << q1);

        // See if we can skip doing any work for this pair, because the state vector entries to processes
        // are both definitely 0.0, as we know they are for states where other qubits are in definite opposite state.
        let skip_processing = ((offset0 & qubit_is_0_mask) != 0) || ((~offset1 & qubit_is_1_mask) != 0);

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

                if shot.op_type == OPID_MRESETZ || shot.op_type == OPID_LOSS_NOISE || scale != 1.0 {
                    // For MResetZ, loss-commit, or renormalization, update the probabilities for all qubits
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

    if scale == 1.0 && shot.op_type != OPID_RZ && shot.op_type != OPID_MRESETZ && shot.op_type != OPID_LOSS_NOISE {
        // Update this thread's totals for the two qubits in the workgroup storage
        qubitProbabilities[tid].zero[q1] = summed_probs[0];
        qubitProbabilities[tid].one[q1]  = summed_probs[1];
    }
}

fn apply_2q_op(workgroupId: u32, tid: u32, q1: u32, q2: u32) {
    let params = get_shot_params(workgroupId, tid, 2 /* qubits per op */);
    let shot = &shots[params.shot_idx];
    let update_probs = shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ;

    // Sometimes a 2-qubit op may be converted to a no-op (ID) due to qubit loss etc., so skip processing in that case
    // Calculate masks to split the index into low, mid, and high bits around the two qubits
    let lowQubit = select(q1, q2, q1 > q2);
    let hiQubit = select(q1, q2, q1 < q2);

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
        let offset01: i32 = offset00 | (1 << q2);
        let offset10: i32 = offset00 | (1 << q1);
        let offset11: i32 = offset10 | (1 << q2);

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
            case OPID_CY {
                // Like CX, but swap |10> and |11> with +/- i phases.
                let amp00: vec2f = stateVector[params.shot_state_vector_start + offset00];
                let amp01: vec2f = stateVector[params.shot_state_vector_start + offset01];
                let amp10: vec2f = stateVector[params.shot_state_vector_start + offset10];
                let amp11: vec2f = stateVector[params.shot_state_vector_start + offset11];
                stateVector[params.shot_state_vector_start + offset10] = vec2f(amp11.y, -amp11.x); // -i * |11>
                stateVector[params.shot_state_vector_start + offset11] = vec2f(-amp10.y, amp10.x); // i * |10>
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
        qubitProbabilities[tid].zero[q1] = summed_probs[0];
        qubitProbabilities[tid].one[q1]  = summed_probs[1];
        qubitProbabilities[tid].zero[q2] = summed_probs[2];
        qubitProbabilities[tid].one[q2]  = summed_probs[3];
    }
}

fn apply_correlated_noise(workgroupId: u32, tid: u32) {
    let params = get_shot_params(workgroupId, tid, 0 /* need to walk all entries */);
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

// Result of sampling which correlated noise entry (if any) to apply.
struct CorrelatedNoiseSample {
    should_apply: u32, // 0 = no noise, 1 = apply noise
    paulis_lo: u32,
    paulis_hi: u32,
}

// Samples the correlated noise table to determine whether noise should be applied, and if so,
// which Pauli string was selected. If no noise is applied, the shot is set to ID and the caller
// can return early.
fn sample_correlated_noise(shot_idx: u32, op_idx: u32, noise_table_idx: u32) -> CorrelatedNoiseSample {
    let shot = &shots[shot_idx];
    let table = &batch_data.correlated_noise_tables[noise_table_idx];

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
        // No noise to apply - set the op to ID
        shot.op_type = OPID_ID;
        shot.op_idx = op_idx;
        shot.qubits_updated_last_op_mask = 0u;
        return CorrelatedNoiseSample(0u, 0u, 0u);
    }

    // Noise should be applied - binary search to find which Pauli string to apply
    let start = i32(table.start_offset);
    let count = i32(table.entry_count);
    let entry_idx = binary_search_noise_table(rand_lo, rand_hi, start, count);
    let entry = &batch_data.correlated_noise_entries[start + entry_idx];

    return CorrelatedNoiseSample(1u, entry.paulis_lo, entry.paulis_hi);
}

// Extracts the 3-bit term value for qubit position `i` from a Pauli + loss string.
// Terms use the encoding I=0, X=1, Z=2, Y=3, L=4. The low two bits double as the
// bit-flip (0x1) and phase-flip (0x2) indicators, and 0x4 marks loss.
// The Rust parsing stores terms with the rightmost (last) character at the lowest
// bits, so for position i we read the 3 bits at (qubit_count - 1 - i) * 3.
fn get_pauli_bits(paulis_lo: u32, paulis_hi: u32, qubit_count: u32, i: u32) -> u32 {
    let bit_position = (qubit_count - 1u - i) * 3u;
    if (bit_position + 3u <= 32u) {
        return (paulis_lo >> bit_position) & 0x7u;
    } else if (bit_position >= 32u) {
        return (paulis_hi >> (bit_position - 32u)) & 0x7u;
    } else {
        // The 3-bit term straddles the boundary between the lo and hi words.
        let low_part = paulis_lo >> bit_position;
        let high_part = paulis_hi << (32u - bit_position);
        return (low_part | high_part) & 0x7u;
    }
}

// Commits correlated noise masks into the shot state: stores the masks, swaps probabilities and
// tracking bits for bit-flipped qubits, records any loss, and sets the shot up for the correlated
// noise execute stage. Qubits in `loss_mask` are scheduled for loss; following loss-commit ops
// perform the measure + reset.
fn commit_correlated_noise(shot_idx: u32, op_idx: u32, bit_flip_mask: u32, phase_flip_mask: u32, loss_mask: u32) {
    let shot = &shots[shot_idx];

    // Schedule loss for any qubit whose sampled term was loss. The actual
    // measure + reset is performed by the loss-commit ops emitted after the
    // correlated-noise op.
    shot.pending_loss_mask |= loss_mask;

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
        let p_lo = batch_data.correlated_noise_entries[start + mid].probability_lo;
        let p_hi = batch_data.correlated_noise_entries[start + mid].probability_hi;

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
