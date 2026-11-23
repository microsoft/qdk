// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A simulator that logs the quantum calls it gets ([`LoggingSimulator`]),
//! for comparing `run_shot`'s calls with the machine's.

use qdk_simulators::qir::{QirCall, QirGate, QirReadout};
use qdk_simulators::{MeasurementResult, QubitID, ResultID, Simulator};
use qsc_data_structures::measurement::MeasurementBasis;

use super::NoOpSimulator;

/// A simulator that does nothing but log each call it gets, as the
/// [`QirCall`] the machine would hand over for it, and measures as
/// [`NoOpSimulator`] does.
pub(crate) struct LoggingSimulator {
    calls: Vec<QirCall>,
    simulator: NoOpSimulator,
}

impl LoggingSimulator {
    /// A simulator that has logged no calls, on `results` results, each
    /// `Zero`.
    pub(crate) fn with_results(results: usize) -> Self {
        Simulator::new(0, results, 0, ())
    }

    /// The calls logged, in the order they came.
    pub(crate) fn into_calls(self) -> Vec<QirCall> {
        self.calls
    }

    /// Each result's value, in the order of their ids.
    pub(crate) fn results(&self) -> impl Iterator<Item = MeasurementResult> + '_ {
        self.simulator.measurements().iter().copied()
    }

    fn gate(&mut self, gate: QirGate, qubits: (QubitID, Option<QubitID>), angle: Option<f64>) {
        self.calls.push(QirCall::Gate {
            gate,
            qubits: (index(qubits.0), qubits.1.map(index)),
            angle,
        });
    }

    fn measure(&mut self, basis: MeasurementBasis, qubit: QubitID, result: ResultID) {
        self.calls.push(QirCall::Measure {
            basis,
            qubit: index(qubit),
            result: index(result),
        });
        self.simulator.mz(qubit, result);
    }
}

/// A qubit's or a result's index, as a [`QirCall`] holds it.
fn index(id: usize) -> u32 {
    u32::try_from(id).expect("a qubit or a result of a program fits in 32 bits")
}

impl Simulator for LoggingSimulator {
    type Noise = ();
    type StateDumpData = ();

    fn new(qubits: usize, results: usize, seed: u32, (): ()) -> Self {
        LoggingSimulator {
            calls: Vec::new(),
            simulator: NoOpSimulator::new(qubits, results, seed, ()),
        }
    }
    fn x(&mut self, target: QubitID) {
        self.gate(QirGate::X, (target, None), None);
    }
    fn y(&mut self, target: QubitID) {
        self.gate(QirGate::Y, (target, None), None);
    }
    fn z(&mut self, target: QubitID) {
        self.gate(QirGate::Z, (target, None), None);
    }
    fn h(&mut self, target: QubitID) {
        self.gate(QirGate::H, (target, None), None);
    }
    fn s(&mut self, target: QubitID) {
        self.gate(QirGate::S, (target, None), None);
    }
    fn s_adj(&mut self, target: QubitID) {
        self.gate(QirGate::SAdj, (target, None), None);
    }
    fn sx(&mut self, target: QubitID) {
        self.gate(QirGate::Sx, (target, None), None);
    }
    fn sx_adj(&mut self, target: QubitID) {
        self.gate(QirGate::SxAdj, (target, None), None);
    }
    fn t(&mut self, target: QubitID) {
        self.gate(QirGate::T, (target, None), None);
    }
    fn t_adj(&mut self, target: QubitID) {
        self.gate(QirGate::TAdj, (target, None), None);
    }
    fn rx(&mut self, angle: f64, target: QubitID) {
        self.gate(QirGate::Rx, (target, None), Some(angle));
    }
    fn ry(&mut self, angle: f64, target: QubitID) {
        self.gate(QirGate::Ry, (target, None), Some(angle));
    }
    fn rz(&mut self, angle: f64, target: QubitID) {
        self.gate(QirGate::Rz, (target, None), Some(angle));
    }
    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.gate(QirGate::Cx, (control, Some(target)), None);
    }
    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.gate(QirGate::Cy, (control, Some(target)), None);
    }
    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.gate(QirGate::Cz, (control, Some(target)), None);
    }
    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.gate(QirGate::Rxx, (q1, Some(q2)), Some(angle));
    }
    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.gate(QirGate::Ryy, (q1, Some(q2)), Some(angle));
    }
    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.gate(QirGate::Rzz, (q1, Some(q2)), Some(angle));
    }
    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        self.gate(QirGate::Swap, (q1, Some(q2)), None);
    }
    fn mx(&mut self, target: QubitID, result_id: ResultID) {
        self.measure(MeasurementBasis::X, target, result_id);
    }
    fn my(&mut self, target: QubitID, result_id: ResultID) {
        self.measure(MeasurementBasis::Y, target, result_id);
    }
    fn mz(&mut self, target: QubitID, result_id: ResultID) {
        self.measure(MeasurementBasis::Z, target, result_id);
    }
    fn mresetz(&mut self, target: QubitID, result_id: ResultID) {
        self.calls.push(QirCall::MeasureReset {
            qubit: index(target),
            result: index(result_id),
        });
        self.simulator.mresetz(target, result_id);
    }
    fn resetz(&mut self, target: QubitID) {
        self.calls.push(QirCall::Reset {
            qubit: index(target),
        });
    }
    fn mov(&mut self, target: QubitID) {
        self.calls.push(QirCall::Move {
            qubit: index(target),
        });
    }
    fn correlated_noise_intrinsic(&mut self, intrinsic_id: u32, targets: &[usize]) {
        self.calls.push(QirCall::CorrelatedNoise {
            qubits: targets.iter().copied().map(index).collect(),
            table: intrinsic_id,
        });
    }
    fn measurements(&self) -> &[MeasurementResult] {
        self.simulator.measurements()
    }
    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        self.simulator.take_measurements()
    }
    fn state_dump(&self) -> &() {
        &()
    }
    fn peek_loss(&mut self, target: QubitID, result_id: ResultID) {
        self.calls.push(QirCall::PeekLoss {
            qubit: index(target),
            result: index(result_id),
        });
        self.simulator.peek_loss(target, result_id);
    }
    fn apply_loss_noise(&mut self, _: f64, _: QubitID) {
        unreachable!("no measured program has loss noise, which no QirCall stands for");
    }
    fn apply_readout_noise(&mut self, p_zero_as_one: f64, p_one_as_zero: f64, result_id: ResultID) {
        let readout = match self.simulator.measurements()[result_id] {
            MeasurementResult::Zero => QirReadout::Zero,
            MeasurementResult::One => QirReadout::One,
            MeasurementResult::Loss => QirReadout::Loss,
        };
        self.calls.push(QirCall::ReadoutNoise {
            zero_as_one: p_zero_as_one,
            one_as_zero: p_one_as_zero,
            result: index(result_id),
            readout,
        });
    }
    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.simulator.write_result(value, result_id);
    }
}
