// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum Execution Profiler: backend interfaces for quantum instructions.
//!
//! QIR execution requires LLVM 20 development libraries
//! (`LLVM_SYS_201_PREFIX` can specify their installation).
//!
//! The runner supports static qubit/result IDs, adaptive control flow, and
//! structured output recording. Dynamic allocation and legacy runtime calls
//! are rejected. It does not depend on or modify QIR Runner.
//!
//! Run the test suite with `cargo test -p qep`.

#[cfg(test)]
mod tests;

mod backends;
mod runner;
pub use backends::{CountingBackend, CountingBackendCounts, NoopBackend, SparseSimBackend};
pub use runner::{OutputRecord, OutputValue, RunOutput, run_qir};

/// A backend for the quantum intrinsics supported by QDK.
///
/// Qubits and result slots are identified by `usize` IDs; rotation angles are
/// in radians. Measurement issuance is separate from reading its classical
/// value, allowing backends to schedule work without waiting for completion.
/// Result values use `false` for zero and `true` for one.
/// Methods return an error if the backend cannot perform the operation.
pub trait QirBackend {
    /// Applies a doubly controlled X gate.
    fn ccx(&mut self, control1: usize, control2: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled X gate.
    fn cx(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled Y gate.
    fn cy(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled Z gate.
    fn cz(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies an X-axis rotation.
    fn rx(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit XX rotation.
    fn rxx(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Y-axis rotation.
    fn ry(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit YY rotation.
    fn ryy(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Z-axis rotation.
    fn rz(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit ZZ rotation.
    fn rzz(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Hadamard gate.
    fn h(&mut self, target: usize) -> Result<(), String>;

    /// Applies an S gate.
    fn s(&mut self, target: usize) -> Result<(), String>;

    /// Applies an adjoint S gate.
    fn sadj(&mut self, target: usize) -> Result<(), String>;

    /// Applies a square-root X gate.
    fn sx(&mut self, target: usize) -> Result<(), String>;

    /// Applies a T gate.
    fn t(&mut self, target: usize) -> Result<(), String>;

    /// Applies an adjoint T gate.
    fn tadj(&mut self, target: usize) -> Result<(), String>;

    /// Applies an X gate.
    fn x(&mut self, target: usize) -> Result<(), String>;

    /// Applies a Y gate.
    fn y(&mut self, target: usize) -> Result<(), String>;

    /// Applies a Z gate.
    fn z(&mut self, target: usize) -> Result<(), String>;

    /// Exchanges two qubit states.
    fn swap(&mut self, target1: usize, target2: usize) -> Result<(), String>;

    /// Issue a computational-basis measurement into the given result slot.
    ///
    /// This may return before the measurement completes. Reusing a result ID
    /// replaces its previous measurement or explicitly written value.
    fn mz(&mut self, target: usize, result: usize) -> Result<(), String>;

    /// Reset a qubit to zero.
    fn reset(&mut self, target: usize) -> Result<(), String>;

    /// Issue a measurement into the given result slot followed by a qubit reset.
    ///
    /// Like `mz`, this need not wait for physical completion.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String>;

    /// Resolve a result slot to its classical value, waiting or advancing
    /// modeled time if necessary. Repeated reads return the same value until
    /// another measurement or write replaces the slot.
    fn read_result(&mut self, result: usize) -> Result<bool, String>;

    /// Replace a result slot with a classical value, respecting prior operations
    /// on that slot. Used by QIR's writable-results runtime function.
    fn write_result(&mut self, result: usize, value: bool) -> Result<(), String>;
}
