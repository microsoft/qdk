// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Runs the programs the QDK compiles today, Adaptive Profile QIR programs,
//! on any quantum target: a machine runs their classical part, written once
//! for every target, and hands each batch of quantum calls to whoever
//! resumes it.
//!
//! Its modules serve three sides:
//! - the machine building a program: `program`'s `check` and
//!   `prepared_quantum_calls`, `fault`, `quantum_instruction_encoding`;
//! - the machine running a context: `machine`, `trap`;
//! - the contract between the machine and a resumer: `call`, `gate`,
//!   `operator_matrix`, `resumer`, `program`.
//!
//! A resumer drives a context through [`QirMachine::resume`], and never
//! sees the bytecode or how it encodes the quantum instructions: it reads
//! the calls ([`QirCall`], of a [`QirInstruction`], a gate's matrix in
//! [`QirGate`]), answers with their outcomes
//! ([`QirOutcome`]), and learns what a program needs from its
//! [`QirProgram`].
//!
//! A bytecode program of the Adaptive Profile ([`QirProgram`]), checked
//! once when it is built, whose quantum instructions are calls of the
//! machine's QIR instructions ([`QirCall`], of a [`QirInstruction`]).
//! Building it reports the faults of its bytecode at once ([`QirFaults`]).
//! The unitary instructions have their matrices in [`QirGate`].
//!
//! A [`QirMachine`] runs the program's classical part, returning to its
//! caller at each batch of quantum calls: a shot's state is held in a
//! [`QirShotContext`], which [`QirMachine::resume`] runs to its next stop
//! ([`QirStop`]): a batch of calls, whose outcomes ([`QirOutcome`]) the
//! next resume takes, or the program's records. A context traps
//! ([`QirTrap`]) on outcomes that do not answer its batch, on a value that
//! building the program could not check, or where it uses a result whose
//! readout came back with no value, for a reason other than a loss
//! ([`QirReadout::Missing`]). A resumer
//! ([`QirResumer`]) runs each batch on the shot's quantum state and
//! resumes the context with its outcomes, keeping the rules of its
//! [`run`](QirResumer::run).
//!
//! The machine computes what the bytecode's reference interpreter
//! (`bytecode::runtime::run_shot`) computes: every shot records what
//! `run_shot` records, but in a few cases, each marked
//! `COMPATIBILITY NOTE` or `DISCUSS COMPATIBILITY`, with its reason, at
//! the check or the test of this crate that pins it.
//!
//! An introduction, with an example, is in `src/qir/README.md`.

mod call;
mod fault;
mod gate;
mod machine;
mod operator_matrix;
mod program;
mod quantum_instruction_encoding;
mod resumer;
mod trap;

pub use call::{QirCall, QirInstruction, QirOutcome, QirQubits, QirReadout};
pub use fault::{QirError, QirFault, QirFaultSite, QirFaults, SideTable};
pub use gate::QirGate;
pub use machine::{QirMachine, QirShotContext, QirStop};
pub use operator_matrix::OperatorMatrix;
pub use program::{MeasuredQubit, QirProgram, TwoQubitGate};
#[cfg(test)]
pub(crate) use quantum_instruction_encoding::QIR_INSTRUCTIONS;
pub use resumer::{QirResumeError, QirResumer, QirShotId, expect_ready};
pub use trap::QirTrap;
