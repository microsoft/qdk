// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Runs the programs the QDK compiles today, Adaptive Profile QIR programs,
//! on any quantum target: a machine runs their classical part, written once
//! for every target, and hands each batch of quantum calls to whoever
//! resumes it.
//!
//! Its modules serve three sides:
//! - building a program: `program`'s `check` and `prepared_quantum_calls`,
//!   `fault`, `quantum_instruction_encoding`;
//! - the machine running a context: `machine`, `trap`;
//! - the contract between the machine and a resumer: `call`, `gate`,
//!   `instruction`, `operator_matrix`, `outcome`, `resumer`, `program`'s
//!   public face ([`QirProgram`]).
//!
//! A resumer drives a context through [`QirMachine::resume`], and never
//! sees the bytecode or how it encodes the quantum instructions: it reads
//! the calls ([`QirCall`], of a [`QirInstruction`], a gate's matrix in
//! [`QirGate`]), answers with their outcomes
//! ([`QirOutcome`]), and learns what a program needs from its
//! [`QirProgram`].
//!
//! Building a [`QirProgram`] checks its bytecode once and reports every
//! fault at once ([`QirFaults`]).
//!
//! A [`QirMachine`] runs the program's classical part. A shot's state is
//! held in a [`QirShotContext`]. [`QirMachine::resume`] runs it to its
//! next stop ([`QirStop`]): a batch of calls, or the program's records. A
//! context traps ([`QirTrap`]) where it cannot go on. A resumer
//! ([`QirResumer`]) runs each batch, keeping the rules of its
//! [`run`](QirResumer::run).
//!
//! The machine computes what the bytecode's reference interpreter
//! (`bytecode::runtime::run_shot`) computes: every shot records what
//! `run_shot` records, except in a few cases, each marked
//! `COMPATIBILITY NOTE` or `DISCUSS COMPATIBILITY`, with its reason, at
//! the check or the test of this crate that pins it.
//!
//! An introduction, with an example, is in `src/qir/README.md`.

mod call;
mod fault;
mod gate;
mod instruction;
mod machine;
mod operator_matrix;
mod outcome;
mod program;
mod quantum_instruction_encoding;
mod resumer;
mod trap;

pub use call::{QirCall, QirQubits};
pub use fault::{QirError, QirFault, QirFaultSite, QirFaults, SideTable};
pub use gate::QirGate;
pub use instruction::QirInstruction;
pub use machine::{QirMachine, QirShotContext, QirStop};
pub use operator_matrix::OperatorMatrix;
pub use outcome::{QirOutcome, QirReadout};
pub use program::{MeasuredQubit, QirProgram, TwoQubitGate};
#[cfg(test)]
pub(crate) use quantum_instruction_encoding::QIR_INSTRUCTIONS;
pub use resumer::{QirResumeError, QirResumer, QirShotId, expect_ready};
pub use trap::QirTrap;
