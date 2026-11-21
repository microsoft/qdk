// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a call of correlated noise costs, on 1, 2 and 3 qubits: its qubits
//! are held in registers, so the machine builds each call when a shot
//! reaches it.
//!
//! Each program is [`UNITS`] calls in one block, the call `k` on the qubits
//! `0..n` rotated by `k` (from qubit `k mod n`), so that a runtime that
//! misplaces them makes other calls than `run_shot`; then the return.

use criterion::Criterion;
use qdk_simulators::bytecode::{AdaptiveProgram, Instruction, Op};

use crate::encoding::{FLAG_AUX1_IMM, FLAG_AUX2_IMM, OP_QUANTUM_GATE, OPID_CORRELATED_NOISE};
use crate::workload::{op, program, ret, set};
use crate::{UNITS, measure};

pub(crate) fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("correlated_noise");
    for qubits in 1..=3 {
        measure(
            &mut group,
            &format!("{qubits}_qubits"),
            &noise_program(qubits),
        );
    }
    group.finish();
}

/// The noise table the calls draw from.
const NOISE_TABLE: u64 = 7;

/// The index of correlated noise's operation in the program's table, after
/// the one of each operation id below 32.
const NOISE_OPERATION: u64 = 32;

/// [`UNITS`] calls of correlated noise on the qubits `0..qubits` (at most
/// 3), each held in the register of its number, the call `k` from the
/// qubit `k mod qubits`.
fn noise_program(qubits: u64) -> AdaptiveProgram<u64> {
    let mut block: Vec<_> = (0..3).map(|qubit| set(qubit, qubit)).collect();
    block.extend((0..UNITS).map(|k| Instruction {
        aux0: NOISE_OPERATION,
        aux1: qubits,
        aux2: k % qubits,
        ..op(OP_QUANTUM_GATE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }));
    block.push(ret());
    let mut bytecode = program(&[&block]);
    // The registers `0..qubits` twice, but the last, so that each call's
    // `qubits` arguments follow one another from its offset.
    bytecode.call_args = (0..2 * qubits - 1).map(|i| i % qubits).collect();
    bytecode.quantum_ops.push(Op::from_tuple((
        OPID_CORRELATED_NOISE,
        NOISE_TABLE,
        qubits,
        0,
        0,
    )));
    bytecode
}
