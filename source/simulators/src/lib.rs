// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod noise_config;
pub use quantum_sparse_sim::QuantumSim;
pub mod cpu_full_state_simulator;
mod gpu_full_state_simulator;
pub mod stabilizer_simulator;
pub use gpu_full_state_simulator::*;

/// A qubit ID.
pub type QubitID = usize;

/// The result of a mesasurement in the Z-basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementResult {
    Zero,
    One,
    Loss,
}

pub trait Simulator {
    type Noise: Default;
    type StateDumpData;

    /// Creates a new simulator.
    fn new(num_qubits: usize, num_results: usize, seed: u32, noise: Self::Noise) -> Self;

    /// Single qubit X gate.
    fn x(&mut self, target: QubitID);

    /// Single qubit Y gate.
    fn y(&mut self, target: QubitID);

    /// Single qubit Z gate.
    fn z(&mut self, target: QubitID);

    /// Single qubit H gate.
    fn h(&mut self, target: QubitID);

    /// Single qubit S gate.
    fn s(&mut self, target: QubitID);

    /// Single qubit S adjoint gate.
    fn s_adj(&mut self, target: QubitID);

    /// Single qubit SX gate.
    fn sx(&mut self, target: QubitID);

    /// Single qubit SX adjoint gate.
    fn sx_adj(&mut self, target: QubitID);

    /// Single qubit T gate.
    fn t(&mut self, target: QubitID);

    /// Single qubit T adjoint gate.
    fn t_adj(&mut self, target: QubitID);

    /// Single qubit RX gate.
    fn rx(&mut self, angle: f64, target: QubitID);

    /// Single qubit RY gate.
    fn ry(&mut self, angle: f64, target: QubitID);

    /// Single qubit RZ gate.
    fn rz(&mut self, angle: f64, target: QubitID);

    /// Controlled-X gate.
    fn cx(&mut self, control: QubitID, target: QubitID);

    /// Controlled-Z gate.
    fn cz(&mut self, control: QubitID, target: QubitID);

    /// Two qubits RXX gate.
    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID);

    /// Two qubits RYY gate.
    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID);

    /// Two qubits RZZ gate.
    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID);

    /// Two qubits SWAP gate.
    fn swap(&mut self, q1: QubitID, q2: QubitID);

    /// `MZ` operation.
    fn mz(&mut self, target: QubitID, result_id: QubitID);

    /// `MResetZ` operation.
    fn mresetz(&mut self, target: QubitID, result_id: QubitID);

    /// `ResetZ` operation.
    fn resetz(&mut self, target: QubitID);

    /// Move operation. The purpose of this operation is modeling
    /// the noise coming from qubit movement in neutral atom machines.
    fn mov(&mut self, target: QubitID);

    /// Applies a correlated noise intrinsic to `targets`.
    fn correlated_noise_intrinsic(&mut self, intrinsic_id: u32, targets: &[usize]);

    /// Returns a list of the measurements recorded during the simulation.
    fn measurements(&self) -> &[MeasurementResult];

    /// Returns a list of the measurements recorded during the simulation.
    fn take_measurements(&mut self) -> Vec<MeasurementResult>;

    /// Dumps the current state of the simulator in some representation that can be compared
    /// for `PartialEq` up to a global phase. This is meant to be used for testing.
    fn state_dump(&self) -> &Self::StateDumpData;
}
