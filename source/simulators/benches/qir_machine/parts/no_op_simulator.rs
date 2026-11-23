// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A simulator that does nothing ([`NoOpSimulator`]), for timing
//! `run_shot`'s classical part.

use qdk_simulators::{MeasurementResult, QubitID, ResultID, Simulator};

/// A simulator that does nothing, and measures every result as `Zero`; a
/// result written is kept, as the simulators keep it.
pub(crate) struct NoOpSimulator {
    measurements: Vec<MeasurementResult>,
}

impl Simulator for NoOpSimulator {
    type Noise = ();
    type StateDumpData = ();

    fn new(_: usize, num_results: usize, _: u32, (): ()) -> Self {
        NoOpSimulator {
            measurements: vec![MeasurementResult::Zero; num_results],
        }
    }
    fn x(&mut self, _: QubitID) {}
    fn y(&mut self, _: QubitID) {}
    fn z(&mut self, _: QubitID) {}
    fn h(&mut self, _: QubitID) {}
    fn s(&mut self, _: QubitID) {}
    fn s_adj(&mut self, _: QubitID) {}
    fn sx(&mut self, _: QubitID) {}
    fn sx_adj(&mut self, _: QubitID) {}
    fn t(&mut self, _: QubitID) {}
    fn t_adj(&mut self, _: QubitID) {}
    fn rx(&mut self, _: f64, _: QubitID) {}
    fn ry(&mut self, _: f64, _: QubitID) {}
    fn rz(&mut self, _: f64, _: QubitID) {}
    fn cx(&mut self, _: QubitID, _: QubitID) {}
    fn cy(&mut self, _: QubitID, _: QubitID) {}
    fn cz(&mut self, _: QubitID, _: QubitID) {}
    fn rxx(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn ryy(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn rzz(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn swap(&mut self, _: QubitID, _: QubitID) {}
    fn mx(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn my(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn mz(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn mresetz(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn resetz(&mut self, _: QubitID) {}
    fn mov(&mut self, _: QubitID) {}
    fn correlated_noise_intrinsic(&mut self, _: u32, _: &[usize]) {}
    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }
    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }
    fn state_dump(&self) -> &() {
        &()
    }
    fn peek_loss(&mut self, _: QubitID, _: ResultID) {}
    fn apply_loss_noise(&mut self, _: f64, _: QubitID) {}
    fn apply_readout_noise(&mut self, _: f64, _: f64, _: ResultID) {}
    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.measurements[result_id] = if value {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }
}
