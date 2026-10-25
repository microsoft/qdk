// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The legacy [`Simulator`] as an L3 terminal of the region ISA.

use executor_protocol::{
    Batch, Capabilities, Close, Describe, ErrorKind, Executor, Fold, Immediate, Outcome, Rejected,
    Value,
};

use crate::{MeasurementResult, QubitID, Simulator};

use super::{MeasurementKind, RegionInstruction, apply_unitary_immediately};

impl From<MeasurementResult> for Value {
    fn from(result: MeasurementResult) -> Self {
        match result {
            MeasurementResult::Zero => Value::Zero,
            MeasurementResult::One => Value::One,
            MeasurementResult::Loss => Value::Error(ErrorKind::Loss),
        }
    }
}

impl TryFrom<Value> for MeasurementResult {
    type Error = ErrorKind;

    /// The measurement result a value reports.
    ///
    /// # Errors
    ///
    /// The value's error kind when it is an error other than a lost qubit,
    /// which no [`MeasurementResult`] represents.
    fn try_from(value: Value) -> Result<Self, ErrorKind> {
        match value {
            Value::Zero => Ok(MeasurementResult::Zero),
            Value::One => Ok(MeasurementResult::One),
            Value::Error(ErrorKind::Loss) => Ok(MeasurementResult::Loss),
            Value::Error(kind) => Err(kind),
        }
    }
}

/// A legacy [`Simulator`] that runs batches of the region ISA on one shot.
///
/// It checks a whole batch before it applies any of it, so that a rejected
/// batch leaves the state as it was: the legacy simulators index their state
/// by qubit and result without checking, and an index beyond them would
/// panic or corrupt the state partway through a batch.
pub struct SimulatorTerminal<S> {
    simulator: S,
    qubits: usize,
    results: usize,
    report: SimulatorReport,
    closed: bool,
}

impl<S: Simulator> SimulatorTerminal<S> {
    /// A terminal on a new simulator of `qubits` qubits and `results`
    /// results, seeded with `seed`, as [`Simulator::new`] makes it.
    #[must_use]
    pub fn new(qubits: usize, results: usize, seed: u32, noise: S::Noise) -> Self {
        Self {
            simulator: S::new(qubits, results, seed, noise),
            qubits,
            results,
            report: SimulatorReport::default(),
            closed: false,
        }
    }

    fn check(&self, at: usize, instruction: &RegionInstruction) -> Result<(), Rejected> {
        let reason = match instruction {
            RegionInstruction::ExecuteRegion { region_id, region } => region
                .operations()
                .iter()
                .flat_map(|operation| {
                    let (first, second) = operation.qubits();
                    std::iter::once(first).chain(second)
                })
                .find(|&qubit| qubit >= self.qubits)
                .map(|qubit| {
                    format!(
                        "region {} uses qubit {qubit}, but the simulator has {} qubits",
                        region_id.value(),
                        self.qubits
                    )
                }),
            RegionInstruction::Measure(request) => self
                .qubit_beyond(request.qubit)
                .or_else(|| {
                    (request.result_id >= self.results).then(|| {
                        format!(
                            "result {} is beyond the simulator's {} results",
                            request.result_id, self.results
                        )
                    })
                })
                .or_else(|| {
                    u32::try_from(request.result_id).is_err().then(|| {
                        format!("result {} does not fit an outcome's tag", request.result_id)
                    })
                }),
            RegionInstruction::Reset { qubit } => self.qubit_beyond(*qubit),
        };
        match reason {
            Some(reason) => Err(Rejected::Instruction { at, reason }),
            None => Ok(()),
        }
    }

    fn qubit_beyond(&self, qubit: QubitID) -> Option<String> {
        (qubit >= self.qubits).then(|| {
            format!(
                "qubit {qubit} is beyond the simulator's {} qubits",
                self.qubits
            )
        })
    }
}

impl<S> SimulatorTerminal<S> {
    /// The simulator, with the state the batches so far left.
    #[must_use]
    pub fn simulator(&self) -> &S {
        &self.simulator
    }

    /// The simulator, with the state the batches so far left.
    #[must_use]
    pub fn into_simulator(self) -> S {
        self.simulator
    }
}

impl<S: Simulator> Executor<Batch<RegionInstruction>> for SimulatorTerminal<S> {
    type Response = Immediate<Outcome>;

    /// Runs the batch on the simulator, in order.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`Close::close`]; [`Rejected::Instruction`]
    /// for the first instruction that uses a qubit or a result beyond those
    /// the terminal was made with. Nothing of a rejected batch runs.
    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        for (at, instruction) in batch.iter().enumerate() {
            self.check(at, instruction)?;
        }
        self.report.batches += 1;
        let mut outcomes = Vec::new();
        for instruction in batch {
            match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    for &operation in region.operations() {
                        apply_unitary_immediately(&mut self.simulator, operation);
                    }
                    self.report.regions += 1;
                    self.report.operations += region.operations().len() as u64;
                }
                RegionInstruction::Measure(request) => {
                    match request.kind {
                        MeasurementKind::MeasureZ => {
                            self.simulator.mz(request.qubit, request.result_id);
                        }
                        MeasurementKind::MeasureResetZ => {
                            self.simulator.mresetz(request.qubit, request.result_id);
                        }
                    }
                    self.report.measurements += 1;
                    let tag = u32::try_from(request.result_id);
                    let result = self.simulator.measurements().get(request.result_id);
                    let (Ok(tag), Some(&result)) = (tag, result) else {
                        return Ok(Immediate::failed(
                            outcomes,
                            format!("the simulator did not record result {}", request.result_id),
                        ));
                    };
                    outcomes.push(Outcome {
                        tag,
                        value: result.into(),
                    });
                }
                RegionInstruction::Reset { qubit } => {
                    self.simulator.resetz(*qubit);
                    self.report.resets += 1;
                }
            }
        }
        Ok(Immediate::done(outcomes))
    }
}

impl<S> Describe for SimulatorTerminal<S> {
    /// The qubits it was made with, as many as fit the protocol's count; one
    /// state, kept between calls, so feedback.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(u32::try_from(self.qubits).unwrap_or(u32::MAX)))
            .with_states(Some(1))
    }
}

impl<S> Close for SimulatorTerminal<S> {
    type Report = SimulatorReport;

    fn close(&mut self) -> SimulatorReport {
        self.closed = true;
        std::mem::take(&mut self.report)
    }
}

/// What a [`SimulatorTerminal`] ran: its batches and their instructions.
///
/// The reports of terminals fold into the report of the run they took part
/// in, by adding their counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimulatorReport {
    /// The batches it ran; rejected batches are not counted.
    pub batches: u64,
    /// The regions it applied.
    pub regions: u64,
    /// The unitary operations of those regions.
    pub operations: u64,
    /// The measurements it made.
    pub measurements: u64,
    /// The resets it made.
    pub resets: u64,
}

impl Fold for SimulatorReport {
    fn fold(&mut self, other: Self) {
        self.batches += other.batches;
        self.regions += other.regions;
        self.operations += other.operations;
        self.measurements += other.measurements;
        self.resets += other.resets;
    }
}
