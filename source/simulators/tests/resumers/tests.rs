// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the resumers, a module per resumer, and the simulators
//! they share: the crate's, seeded by the shot's id, and [`Log`], which
//! logs the operations it is asked to apply and reads each measurement as
//! its test scripts it.

mod grouping_resumer;
mod simulator_resumer;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::qir::{QirProgram, QirShotId};
use qdk_simulators::{MeasurementResult, QubitID, ResultID, Simulator};

/// A noiseless full-state simulator of `program`'s qubits and results for
/// `shot`, seeded with its id, so that a shot reads the same outcomes
/// wherever it runs: the kit's rule 7.
pub fn full_state(program: &QirProgram, shot: QirShotId) -> FullStateSimulator {
    FullStateSimulator::new(
        program.qubits() as usize,
        program.results() as usize,
        shot.0,
        Arc::default(),
    )
}

/// The operations a [`Log`] was asked to apply, shared with whoever reads
/// them.
pub type Operations = Rc<RefCell<Vec<String>>>;

/// What a [`Log`] reads at the measurement of a qubit.
type Reads = Rc<dyn Fn(QubitID) -> MeasurementResult>;

/// A simulator that logs the operations it is asked to apply, each written as
/// the method of the [`Simulator`] trait and its arguments (`cx(0, 1)`), and
/// reads each measurement of a qubit as `reads` says; `peek_loss` finds every
/// qubit lost, and readout noise changes no result.
pub struct Log {
    operations: Operations,
    reads: Reads,
    measurements: Vec<MeasurementResult>,
}

impl Log {
    /// A log of `results` results, logging into `operations`, that reads each
    /// qubit as `reads` says.
    pub fn new_reading(
        results: usize,
        operations: Operations,
        reads: impl Fn(QubitID) -> MeasurementResult + 'static,
    ) -> Self {
        Self {
            operations,
            reads: Rc::new(reads),
            measurements: vec![MeasurementResult::Zero; results],
        }
    }

    fn log(&mut self, operation: String) {
        self.operations.borrow_mut().push(operation);
    }

    fn measure(&mut self, target: QubitID, result_id: ResultID) {
        self.measurements[result_id] = (self.reads)(target);
    }
}

/// The parity of `qubit`: `Zero` for an even qubit, `One` for an odd one.
pub fn parity(qubit: QubitID) -> MeasurementResult {
    if qubit.is_multiple_of(2) {
        MeasurementResult::Zero
    } else {
        MeasurementResult::One
    }
}

impl Simulator for Log {
    type Noise = ();
    type StateDumpData = ();

    /// A log of its own that reads each qubit as its [`parity`].
    fn new(_: usize, num_results: usize, _: u32, (): ()) -> Self {
        Log::new_reading(num_results, Operations::default(), parity)
    }
    fn x(&mut self, target: QubitID) {
        self.log(format!("x({target})"));
    }
    fn y(&mut self, target: QubitID) {
        self.log(format!("y({target})"));
    }
    fn z(&mut self, target: QubitID) {
        self.log(format!("z({target})"));
    }
    fn h(&mut self, target: QubitID) {
        self.log(format!("h({target})"));
    }
    fn s(&mut self, target: QubitID) {
        self.log(format!("s({target})"));
    }
    fn s_adj(&mut self, target: QubitID) {
        self.log(format!("s_adj({target})"));
    }
    fn sx(&mut self, target: QubitID) {
        self.log(format!("sx({target})"));
    }
    fn sx_adj(&mut self, target: QubitID) {
        self.log(format!("sx_adj({target})"));
    }
    fn t(&mut self, target: QubitID) {
        self.log(format!("t({target})"));
    }
    fn t_adj(&mut self, target: QubitID) {
        self.log(format!("t_adj({target})"));
    }
    fn rx(&mut self, angle: f64, target: QubitID) {
        self.log(format!("rx({angle}, {target})"));
    }
    fn ry(&mut self, angle: f64, target: QubitID) {
        self.log(format!("ry({angle}, {target})"));
    }
    fn rz(&mut self, angle: f64, target: QubitID) {
        self.log(format!("rz({angle}, {target})"));
    }
    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.log(format!("cx({control}, {target})"));
    }
    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.log(format!("cy({control}, {target})"));
    }
    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.log(format!("cz({control}, {target})"));
    }
    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.log(format!("rxx({angle}, {q1}, {q2})"));
    }
    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.log(format!("ryy({angle}, {q1}, {q2})"));
    }
    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.log(format!("rzz({angle}, {q1}, {q2})"));
    }
    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        self.log(format!("swap({q1}, {q2})"));
    }
    fn mz(&mut self, target: QubitID, result_id: ResultID) {
        self.log(format!("mz({target}, {result_id})"));
        self.measure(target, result_id);
    }
    fn mresetz(&mut self, target: QubitID, result_id: ResultID) {
        self.log(format!("mresetz({target}, {result_id})"));
        self.measure(target, result_id);
    }
    fn resetz(&mut self, target: QubitID) {
        self.log(format!("resetz({target})"));
    }
    fn mov(&mut self, target: QubitID) {
        self.log(format!("mov({target})"));
    }
    fn correlated_noise_intrinsic(&mut self, intrinsic_id: u32, targets: &[usize]) {
        self.log(format!(
            "correlated_noise_intrinsic({intrinsic_id}, {targets:?})"
        ));
    }
    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }
    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }
    fn state_dump(&self) -> &() {
        &()
    }
    fn peek_loss(&mut self, target: QubitID, result_id: ResultID) {
        self.log(format!("peek_loss({target}, {result_id})"));
        self.measurements[result_id] = MeasurementResult::Loss;
    }
    fn apply_loss_noise(&mut self, p_loss: f64, target: QubitID) {
        self.log(format!("apply_loss_noise({p_loss}, {target})"));
    }
    fn apply_readout_noise(&mut self, p_zero_as_one: f64, p_one_as_zero: f64, result_id: ResultID) {
        self.log(format!(
            "apply_readout_noise({p_zero_as_one}, {p_one_as_zero}, {result_id})"
        ));
    }
    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.log(format!("write_result({value}, {result_id})"));
    }
}
