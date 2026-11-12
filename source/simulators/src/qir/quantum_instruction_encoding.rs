// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the bytecode holds each [`QirInstruction`], and the one function
//! that turns it into a call ([`QirCall`]), which is all a resumer sees of
//! it: never the bytecode.
//!
//! The bytecode holds a quantum instruction as an opcode and an operation
//! id, with its operands in fields that differ from one instruction to
//! another; the machine's instructions are named. Keeping the map in one
//! place means that a new quantum instruction, or a change of its
//! encoding, touches only this file and the check.
//!
//! For each instruction: the opcode and
//! operation id that run it ([`QIR_INSTRUCTIONS`]), the operands it takes
//! ([`Shape`]), and the one function that builds a [`QirCall`] from them
//! ([`build`]), whether building the program gives the operands or a run
//! reads them from registers. The bytecode is an [`AdaptiveProgram`],
//! written by the Python package's adaptive pass (`qdk/_adaptive_pass.py`)
//! in the encoding of `qdk/_adaptive_bytecode.py`, which
//! `bytecode/encoding.rs` mirrors ("Where a program comes from", in
//! `src/qir/README.md`).
//!
//! This is the one place that maps the bytecode's quantum instructions to
//! the machine's: the check (`program/check.rs`) reads which fields
//! of a quantum instruction hold its operands, and decodes it through this
//! map. The classical instructions are read with the `OP_*` constants, in
//! `program/check.rs` and `machine/walk.rs`.

use super::{QirCall, QirGate, QirInstruction};
#[cfg(doc)]
use crate::bytecode::AdaptiveProgram;
use crate::bytecode::encoding::{
    OPID_CORRELATED_NOISE, OPID_CX, OPID_CY, OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ,
    OPID_RESETZ, OPID_RX, OPID_RXX, OPID_RY, OPID_RYY, OPID_RZ, OPID_RZZ, OPID_S, OPID_S_ADJ,
    OPID_SWAP, OPID_SX, OPID_SX_ADJ, OPID_T, OPID_T_ADJ, OPID_X, OPID_Y, OPID_Z,
};

/// Where the bytecode holds a [`QirInstruction`]: the opcode that runs
/// it and, for an opcode that runs several, its operation id
/// ([`Op::op_id`](crate::bytecode::Op::op_id)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// `QUANTUM_GATE`, with this operation id.
    Gate(u64),
    /// `MEASURE`, with this operation id.
    Measure(u64),
    /// `RESET`, with this operation id.
    Reset(u64),
    /// `PEEK_LOSS`.
    PeekLoss,
    /// `READOUT_NOISE`.
    ReadoutNoise,
}

/// The machine's instructions, in their order, with where the bytecode holds
/// each: by operation id, correlated noise (131) the last, then the loss
/// and readout-noise instructions, which have none. Neither the identity
/// (operation 0) nor `ccx` (operation 20) is one ([`QirInstruction`] says
/// why).
///
/// Crate-visible so that the tests can check they cover every instruction.
pub(crate) const QIR_INSTRUCTIONS: [(Encoding, QirInstruction); 27] = [
    (Encoding::Reset(OPID_RESETZ), QirInstruction::Reset),
    (Encoding::Gate(OPID_X), QirInstruction::Gate(QirGate::X)),
    (Encoding::Gate(OPID_Y), QirInstruction::Gate(QirGate::Y)),
    (Encoding::Gate(OPID_Z), QirInstruction::Gate(QirGate::Z)),
    (Encoding::Gate(OPID_H), QirInstruction::Gate(QirGate::H)),
    (Encoding::Gate(OPID_S), QirInstruction::Gate(QirGate::S)),
    (
        Encoding::Gate(OPID_S_ADJ),
        QirInstruction::Gate(QirGate::SAdj),
    ),
    (Encoding::Gate(OPID_T), QirInstruction::Gate(QirGate::T)),
    (
        Encoding::Gate(OPID_T_ADJ),
        QirInstruction::Gate(QirGate::TAdj),
    ),
    (Encoding::Gate(OPID_SX), QirInstruction::Gate(QirGate::Sx)),
    (
        Encoding::Gate(OPID_SX_ADJ),
        QirInstruction::Gate(QirGate::SxAdj),
    ),
    (Encoding::Gate(OPID_RX), QirInstruction::Gate(QirGate::Rx)),
    (Encoding::Gate(OPID_RY), QirInstruction::Gate(QirGate::Ry)),
    (Encoding::Gate(OPID_RZ), QirInstruction::Gate(QirGate::Rz)),
    (Encoding::Gate(OPID_CX), QirInstruction::Gate(QirGate::Cx)),
    (Encoding::Gate(OPID_CZ), QirInstruction::Gate(QirGate::Cz)),
    (Encoding::Gate(OPID_RXX), QirInstruction::Gate(QirGate::Rxx)),
    (Encoding::Gate(OPID_RYY), QirInstruction::Gate(QirGate::Ryy)),
    (Encoding::Gate(OPID_RZZ), QirInstruction::Gate(QirGate::Rzz)),
    (Encoding::Measure(OPID_MZ), QirInstruction::Measure),
    (
        Encoding::Measure(OPID_MRESETZ),
        QirInstruction::MeasureReset,
    ),
    (
        Encoding::Gate(OPID_SWAP),
        QirInstruction::Gate(QirGate::Swap),
    ),
    (Encoding::Gate(OPID_MOVE), QirInstruction::Move),
    (Encoding::Gate(OPID_CY), QirInstruction::Gate(QirGate::Cy)),
    (
        Encoding::Gate(OPID_CORRELATED_NOISE),
        QirInstruction::CorrelatedNoise,
    ),
    (Encoding::PeekLoss, QirInstruction::PeekLoss),
    (Encoding::ReadoutNoise, QirInstruction::ReadoutNoise),
];

/// The position in [`QIR_INSTRUCTIONS`] of the instruction that `encoding`
/// runs; `None` when it runs none of them.
pub(super) fn position(encoding: Encoding) -> Option<usize> {
    QIR_INSTRUCTIONS
        .iter()
        .position(|(held, _)| *held == encoding)
}

/// The position of `instruction` in [`QIR_INSTRUCTIONS`].
pub(super) fn index(instruction: QirInstruction) -> usize {
    QIR_INSTRUCTIONS
        .iter()
        .position(|(_, held)| *held == instruction)
        .expect("the table holds every QirInstruction")
}

/// The operands an instruction takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// A qubit.
    Qubit,
    /// Two qubits.
    TwoQubits,
    /// A qubit and an angle.
    QubitAngle,
    /// Two qubits and an angle.
    TwoQubitsAngle,
    /// A qubit and the result written.
    QubitResult,
    /// The probabilities of reading 0 as 1 and 1 as 0, and the result
    /// they act on.
    ReadoutNoise,
    /// Any number of qubits, and the id of the noise table drawn from.
    CorrelatedNoise,
}

/// The operands `instruction` takes.
pub(super) fn shape(instruction: QirInstruction) -> Shape {
    match instruction {
        QirInstruction::Gate(gate) => match (gate.qubit_count(), gate.is_rotation()) {
            (1, false) => Shape::Qubit,
            (1, true) => Shape::QubitAngle,
            (_, false) => Shape::TwoQubits,
            (_, true) => Shape::TwoQubitsAngle,
        },
        QirInstruction::Reset | QirInstruction::Move => Shape::Qubit,
        QirInstruction::Measure | QirInstruction::MeasureReset | QirInstruction::PeekLoss => {
            Shape::QubitResult
        }
        QirInstruction::ReadoutNoise => Shape::ReadoutNoise,
        QirInstruction::CorrelatedNoise => Shape::CorrelatedNoise,
    }
}

/// One of the program's registers, as an operand names it.
pub(super) type Register = u32;

/// An operand of a call: known when the program is built, or held in a
/// register, whose value a run gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CallOperand<T> {
    Known(T),
    InRegister(Register),
}

/// The operands of a call of an instruction other than correlated noise,
/// by kind, each kind in the instruction's order: those its [`Shape`]
/// takes, any other slot `Known` zero, which [`build`] does not read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CallOperands {
    pub(super) qubits: [CallOperand<u32>; 2],
    /// A rotation's angle, or readout noise's two probabilities.
    pub(super) reals: [CallOperand<f64>; 2],
    /// The result the call writes: one at most, by construction, so that
    /// the machine, its analysis and the call itself
    /// ([`QirCall::result`]) agree on it.
    pub(super) result: Option<CallOperand<u32>>,
}

/// Operands that are all known: the qubits, the reals and the result
/// (zero when the call writes none) that [`build`] takes.
pub(super) type KnownOperands = ([u32; 2], [f64; 2], u32);

impl CallOperands {
    /// No operand: every slot `Known` zero.
    pub(super) const NONE: Self = Self {
        qubits: [CallOperand::Known(0); 2],
        reals: [CallOperand::Known(0.0); 2],
        result: None,
    };

    /// The operands, when all are known.
    pub(super) fn known(&self) -> Option<KnownOperands> {
        let qubit = |operand| match operand {
            CallOperand::Known(qubit) => Some(qubit),
            CallOperand::InRegister(_) => None,
        };
        let real = |operand| match operand {
            CallOperand::Known(real) => Some(real),
            CallOperand::InRegister(_) => None,
        };
        let result = match self.result {
            None => 0,
            Some(operand) => qubit(operand)?,
        };
        Some((
            [qubit(self.qubits[0])?, qubit(self.qubits[1])?],
            [real(self.reals[0])?, real(self.reals[1])?],
            result,
        ))
    }
}

/// The call of `instruction`, any but correlated noise, on the operands
/// its [`Shape`] takes from `qubits`, `reals` and `result`, in their
/// order; the other values are not read.
#[inline]
pub(super) fn build(
    instruction: QirInstruction,
    (qubits, reals, result): KnownOperands,
) -> QirCall {
    let qubit = qubits[0];
    match instruction {
        QirInstruction::Gate(gate) => QirCall::Gate {
            gate,
            qubits: (qubit, (gate.qubit_count() == 2).then_some(qubits[1])),
            angle: gate.is_rotation().then_some(reals[0]),
        },
        QirInstruction::Measure => QirCall::Measure { qubit, result },
        QirInstruction::MeasureReset => QirCall::MeasureReset { qubit, result },
        QirInstruction::Reset => QirCall::Reset { qubit },
        QirInstruction::Move => QirCall::Move { qubit },
        QirInstruction::PeekLoss => QirCall::PeekLoss { qubit, result },
        QirInstruction::ReadoutNoise => QirCall::ReadoutNoise {
            zero_as_one: reals[0],
            one_as_zero: reals[1],
            result,
        },
        QirInstruction::CorrelatedNoise => {
            unreachable!("correlated noise is built from its call arguments")
        }
    }
}
