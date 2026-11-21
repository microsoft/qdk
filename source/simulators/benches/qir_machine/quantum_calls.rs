// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a quantum call costs: one whose operands are all constants, which
//! building the program prebuilds (`prebuilt`), against one whose qubits
//! and angle are read from registers, which the machine builds when a shot
//! reaches it (`from_registers`).
//!
//! Each program is [`UNITS`] gates in one block, cycling through `x`, `h`,
//! `cx` and `rz` on qubits 0 to 2, then the return.

use criterion::Criterion;
use qdk_simulators::bytecode::{AdaptiveProgram, Instruction};

use crate::encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, OP_QUANTUM_GATE, OPID_CX, OPID_H, OPID_RZ, OPID_X,
};
use crate::workload::{op, program, ret, set};
use crate::{UNITS, measure};

pub(crate) fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantum_calls");
    measure(&mut group, "prebuilt", &prebuilt());
    measure(&mut group, "from_registers", &from_registers());
    group.finish();
}

/// The angle of the rotations.
const ANGLE: f64 = 0.5;

/// The gate of the `i`-th call: its operation id and its two qubits (the
/// second unused by a one-qubit gate).
fn gate(i: u64) -> (u64, u64, u64) {
    [
        (OPID_X, 0, 0),
        (OPID_H, 1, 0),
        (OPID_CX, 0, 1),
        (OPID_RZ, 2, 0),
    ][usize::try_from(i % 4).expect("below 4")]
}

/// The gates with constant operands.
fn prebuilt() -> AdaptiveProgram<u64> {
    let block: Vec<_> = (0..UNITS)
        .map(|i| {
            let (op_id, q1, q2) = gate(i);
            Instruction {
                src0: ANGLE.to_bits(),
                aux0: op_id,
                aux1: q1,
                aux2: q2,
                ..op(
                    OP_QUANTUM_GATE,
                    FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
                )
            }
        })
        .chain([ret()])
        .collect();
    program(&[&block])
}

/// The gates with operands read from registers: registers 0 to 2 hold the
/// qubits 0 to 2, register 3 the angle.
fn from_registers() -> AdaptiveProgram<u64> {
    let mut block: Vec<_> = (0..3)
        .map(|qubit| set(qubit, qubit))
        .chain([set(3, ANGLE.to_bits())])
        .collect();
    block.extend((0..UNITS).map(|i| {
        let (op_id, q1, q2) = gate(i);
        Instruction {
            src0: 3,
            aux0: op_id,
            aux1: q1,
            aux2: q2,
            ..op(OP_QUANTUM_GATE, 0)
        }
    }));
    block.push(ret());
    program(&[&block])
}
