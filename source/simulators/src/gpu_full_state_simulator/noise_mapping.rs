// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    noise_config::{NoiseConfig, NoiseTable, PauliAndLossString},
    shader_types::{Op, ops},
};

/// The 3-bit term value representing qubit loss (matches `encode_pauli`).
const LOSS_TERM: u64 = 4;

/// Decodes the categorical-outcome flat slot for a fault string. Outcomes are
/// stored in the noise op's matrix floats; the host writes slot `k` and the
/// shader reads it as `unitary[k / 2][k % 2]`.
///
/// Terms use the 3-bit encoding (I=0, X=1, Z=2, Y=3, L=4). For a 1-qubit table
/// the slot is the single qubit's term; for a 2-qubit table it is
/// `q1_term * 5 + q2_term` (5 possible terms per qubit). The identity outcome
/// (slot 0) is implicit and never stored.
fn outcome_slot(pauli: PauliAndLossString, qubits: u32) -> usize {
    match qubits {
        1 => (pauli & 0b111) as usize,
        2 => {
            let q1_term = ((pauli >> 3) & 0b111) as usize;
            let q2_term = (pauli & 0b111) as usize;
            q1_term * 5 + q2_term
        }
        _ => panic!("Unsupported qubit count in noise table: {qubits}"),
    }
}

/// Returns true if any fault string in the table loses a qubit.
fn table_has_loss(noise_table: &NoiseTable<f32>) -> bool {
    noise_table
        .pauli_strings
        .iter()
        .any(|p| (0..noise_table.qubits).any(|i| (p >> (i * 3)) & 0b111 == LOSS_TERM))
}

fn set_noise_op_probabilities(noise_table: &NoiseTable<f32>, op: &mut Op) {
    for (pauli, prob) in noise_table
        .pauli_strings
        .iter()
        .zip(&noise_table.probabilities)
    {
        op.set_noise_prob_slot(outcome_slot(*pauli, noise_table.qubits), *prob);
    }
}

fn get_noise_op(op: &Op, noise_table: &NoiseTable<f32>) -> Op {
    let mut noise_op = match noise_table.qubits {
        1 => Op::new_1q_gate(ops::PAULI_NOISE_1Q, op.q1),
        2 => Op::new_2q_gate(ops::PAULI_NOISE_2Q, op.q1, op.q2),
        _ => panic!(
            "Unsupported qubit count in noise table: {}",
            noise_table.qubits
        ),
    };
    set_noise_op_probabilities(noise_table, &mut noise_op);
    noise_op
}

/// Returns the [`NoiseTable`] in `noise_config` that applies to the given op,
/// or `None` if the op has no associated noise table (e.g. a noise op itself).
fn noise_table_for<'a>(
    op: &Op,
    noise_config: &'a NoiseConfig<f32, f64>,
) -> Option<&'a NoiseTable<f32>> {
    let noise_table = match op.id {
        ops::ID => &noise_config.i,
        ops::X => &noise_config.x,
        ops::Y => &noise_config.y,
        ops::Z => &noise_config.z,
        ops::H => &noise_config.h,
        ops::S => &noise_config.s,
        ops::S_ADJ => &noise_config.s_adj,
        ops::T => &noise_config.t,
        ops::T_ADJ => &noise_config.t_adj,
        ops::SX => &noise_config.sx,
        ops::SX_ADJ => &noise_config.sx_adj,
        ops::RX => &noise_config.rx,
        ops::RY => &noise_config.ry,
        ops::RZ => &noise_config.rz,
        ops::CX => &noise_config.cx,
        ops::CY => &noise_config.cy,
        ops::CZ => &noise_config.cz,
        ops::RXX => &noise_config.rxx,
        ops::RYY => &noise_config.ryy,
        ops::RZZ => &noise_config.rzz,
        ops::SWAP => &noise_config.swap,
        ops::MOVE => &noise_config.mov,
        ops::MZ => &noise_config.mz,
        ops::MRESETZ | ops::RESETZ => &noise_config.mresetz,
        _ => return None,
    };
    Some(noise_table)
}

/// Returns the [`LossPolicy`] configured for the given op's gate, encoded as a
/// `u32` for the GPU shader (see [`LossPolicy::as_u32`]). Returns `None` for
/// ops that have no associated gate noise table.
///
/// The shader reads this from the gate op's `q3` field to decide how to handle
/// the gate when one of its operands is lost.
#[must_use]
pub fn loss_policy_u32(op: &Op, noise_config: &NoiseConfig<f32, f64>) -> Option<u32> {
    noise_table_for(op, noise_config).map(|table| table.on_loss.as_u32())
}

/// Builds the noise ops to insert after `op` for the given config, or `None`
/// if the gate is noiseless.
///
/// `emit_loss_commits` controls whether loss-commit ops are appended after the
/// categorical sampler op. The base (non-adaptive) path dispatches ops linearly
/// and needs an explicit loss-commit op per qubit to perform the deferred
/// measure + reset, so it passes `true`. The adaptive path instead drains
/// `pending_loss_mask` inside the interpreter loop, so it passes `false` to
/// avoid emitting loss-commit ops that would never be dispatched (which would
/// otherwise roughly double the op pool for circuits with loss on every gate).
#[must_use]
pub fn get_noise_ops(
    op: &Op,
    noise_config: &NoiseConfig<f32, f64>,
    emit_loss_commits: bool,
) -> Option<Vec<Op>> {
    let noise_table = noise_table_for(op, noise_config)?;

    if noise_table.is_noiseless() {
        return None;
    }
    // Always emit the categorical noise op (its distribution now includes any
    // loss outcomes). On the base path, when the table can lose a qubit, also
    // emit a loss-commit op per target qubit; each fires only if the sampler set
    // its qubit's bit.
    let mut results = vec![get_noise_op(op, noise_table)];

    if emit_loss_commits && table_has_loss(noise_table) {
        if ops::is_2q_op(op.id) {
            results.push(Op::new_loss_commit(op.q1));
            results.push(Op::new_loss_commit(op.q2));
        } else if ops::is_1q_op(op.id) {
            results.push(Op::new_loss_commit(op.q1));
        } else {
            panic!("unsupported op for loss noise: {op:?}");
        }
    }
    Some(results)
}

/// Expand a program by inserting a loss-commit op after each correlated-noise
/// op, one per targeted qubit. Correlated-noise (intrinsic) tables can contain
/// loss strings, and the shader records any sampled loss in `pending_loss_mask`;
/// these loss-commit ops perform the deferred measure + reset. Ops with no loss
/// sampled leave the mask clear, so the loss-commit acts as identity.
#[must_use]
pub fn expand_correlated_loss_commits(ops: &[Op]) -> Vec<Op> {
    // Most programs have no correlated noise, so start from the existing length.
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        out.push(*op);
        if op.id == ops::CORRELATED_NOISE {
            // q2 holds the number of qubits the correlated op targets.
            for i in 0..op.q2 {
                let q = op.correlated_noise_qubit(i);
                out.push(Op::new_loss_commit(q));
            }
        }
    }
    out
}
