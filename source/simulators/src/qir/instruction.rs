// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The quantum instructions of QIR that a [`QirMachine`] runs
//! ([`QirInstruction`]).
//!
//! They exist so that a resumer can learn what a program needs before it
//! runs it ([`QirProgram::instructions`]), and tell the calls of a batch
//! apart ([`QirCall::instruction`]), by QIR's own names, without the
//! bytecode's opcodes and operation ids.

use std::fmt;

use qsc_data_structures::measurement::MeasurementBasis;

use super::QirGate;
#[cfg(doc)]
use super::{QirCall, QirMachine, QirProgram};

/// A QIR instruction of the machine: what a [`QirCall`] does, apart from
/// its qubits, results and parameters. These are the quantum instructions
/// of QIR's Adaptive Profile that a [`QirProgram`] runs, each named by its
/// QIR mnemonic ([`mnemonic`](Self::mnemonic)); a program lists those it
/// uses ([`QirProgram::instructions`]).
///
/// The identity (`i`, operation 0) is not one. A program using it is
/// refused when built. No producer emits it: Q#'s `I` compiles to
/// nothing, and an explicit wait is `move`. Run as nothing, it would skip
/// the GPU simulator's idle noise. If a producer emits it, it is to be
/// added as an instruction charged with idle noise, as `move` is. Nor is
/// `ccx` one, since the passes decompose it first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QirInstruction {
    /// A unitary gate.
    Gate(QirGate),
    /// `mx`, `my` or `mz`: measures a qubit in the X, Y or Z basis into a
    /// result.
    Measure(MeasurementBasis),
    /// `mresetz`: measures a qubit in the Z basis into a result, then
    /// resets it to |0⟩. It has no X or Y form, as the bytecode has no
    /// operation for one.
    MeasureReset,
    /// `reset`: resets a qubit to |0⟩.
    Reset,
    /// `move`: moves a qubit, which a noisy simulator may charge with idle
    /// noise or loss; it changes no state otherwise.
    Move,
    /// `correlated_noise`: draws a fault from a noise table and applies it
    /// to its qubits.
    CorrelatedNoise,
    /// `peek_loss`: writes into a result whether a qubit is lost.
    PeekLoss,
    /// `readout_noise`: flips the readout a result holds, with a
    /// probability for each value.
    ReadoutNoise,
}

impl QirInstruction {
    /// Its QIR mnemonic: the name of its QIR function less the prefix
    /// (`__quantum__qis__`, or `__quantum__rt__` for `readout_noise`) and
    /// any `__body`, as `mz`, `cx` or `correlated_noise`.
    #[must_use]
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::Gate(gate) => gate.mnemonic(),
            Self::Measure(MeasurementBasis::X) => "mx",
            Self::Measure(MeasurementBasis::Y) => "my",
            Self::Measure(MeasurementBasis::Z) => "mz",
            Self::MeasureReset => "mresetz",
            Self::Reset => "reset",
            Self::Move => "move",
            Self::CorrelatedNoise => "correlated_noise",
            Self::PeekLoss => "peek_loss",
            Self::ReadoutNoise => "readout_noise",
        }
    }
}

impl fmt::Display for QirInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.mnemonic())
    }
}
