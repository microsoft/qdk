// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A resumer that runs each shot on a simulator of the crate's
//! ([`QirSimulatorResumer`]), and the run of a batch on a simulator that
//! it, and the grouping resumer, use ([`run_batch`]).
//!
//! Example code, not the crate's API: a resumer over a [`Simulator`] is a
//! few lines. The example's `main`, the tests and the benchmark include
//! this file, so it names the crate's items by their paths.

use std::convert::Infallible;

use qdk_simulators::qir::{
    QirCall, QirGate, QirMachine, QirOutcome, QirProgram, QirReadout, QirResumeError, QirResumer,
    QirShotId, QirStop,
};
use qdk_simulators::{MeasurementResult, OutputRecord, Simulator};

/// Runs the shots of a run one after another, in their order, each on a
/// simulator made for it by `make`, from the program and the shot's id: so
/// whoever makes the resumer chooses each shot's seed, and a shot seeded by its
/// id gives the same records wherever it runs.
///
/// One context of the machine serves every shot of a run
/// ([`QirMachine::restart_shot`]). Its runs never wait, and its own work
/// never fails ([`Infallible`]): a run fails only where a shot traps.
pub struct QirSimulatorResumer<F> {
    make: F,
}

impl<F> QirSimulatorResumer<F> {
    /// The resumer whose shots run on the simulators `make` makes.
    pub fn new(make: F) -> Self {
        Self { make }
    }
}

impl<S, F> QirResumer for QirSimulatorResumer<F>
where
    S: Simulator,
    F: FnMut(&QirProgram, QirShotId) -> S,
{
    type Error = Infallible;

    async fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<Infallible>> {
        // One context serves every shot: `restart_shot` makes it a new
        // shot's, reusing its memory.
        let mut context = machine.start_shot();
        let mut outcomes = Vec::new();
        let mut records = Vec::with_capacity(shots.len());
        for &shot in shots {
            // The shot's quantum state, made from the program's facts and
            // the shot's id, by which `make` seeds it.
            let mut simulator = (self.make)(machine.program(), shot);
            machine.restart_shot(&mut context);
            outcomes.clear();
            loop {
                // Resume the shot with its last batch's outcomes (none at
                // first), up to its next stop.
                match machine.resume(&mut context, &outcomes) {
                    // A batch: run its quantum calls on the shot's simulator,
                    // in order, and keep one outcome per quantum call that
                    // writes a result, for the next resume.
                    Ok(QirStop::Batch(batch)) => {
                        outcomes.clear();
                        run_batch(&mut simulator, batch, &mut outcomes);
                    }
                    // The shot's end: its records, at its place.
                    Ok(QirStop::Ended(ended)) => {
                        records.push(ended.to_vec());
                        break;
                    }
                    // A trap stops the run, naming its shot: the shots run
                    // in the order of `shots`, so it is the first failing.
                    Err(trap) => return Err(QirResumeError::Trapped { shot, trap }),
                }
            }
        }
        Ok(records)
    }
}

/// Runs `batch` on `simulator`, each quantum call as the simulator operation
/// that `run_shot`, the bytecode's reference interpreter, applies for its
/// instruction, in the batch's order; then appends to `outcomes` the outcome of
/// each quantum call that writes a result, in the batch's order: the
/// simulator's value of the result after the batch.
///
/// No two quantum calls of a batch write one result (the machine stops before a
/// quantum call writing a result its batch writes), so each result's value is
/// read once, after the batch.
///
/// # Panics
///
/// When a qubit or a result is past the simulator's: a batch of the
/// machine has none, on a simulator with the program's qubits and results.
pub fn run_batch<S: Simulator>(
    simulator: &mut S,
    batch: &[QirCall],
    outcomes: &mut Vec<QirOutcome>,
) {
    let first = outcomes.len();
    for call in batch {
        match call {
            QirCall::Gate {
                gate,
                qubits: (qubit, second),
                angle,
            } => {
                let qubit = *qubit as usize;
                let second = || second.expect("a two-qubit gate has its second qubit") as usize;
                let angle = || angle.expect("a rotation has its angle");
                match gate {
                    QirGate::X => simulator.x(qubit),
                    QirGate::Y => simulator.y(qubit),
                    QirGate::Z => simulator.z(qubit),
                    QirGate::H => simulator.h(qubit),
                    QirGate::S => simulator.s(qubit),
                    QirGate::SAdj => simulator.s_adj(qubit),
                    QirGate::T => simulator.t(qubit),
                    QirGate::TAdj => simulator.t_adj(qubit),
                    QirGate::Sx => simulator.sx(qubit),
                    QirGate::SxAdj => simulator.sx_adj(qubit),
                    QirGate::Rx => simulator.rx(angle(), qubit),
                    QirGate::Ry => simulator.ry(angle(), qubit),
                    QirGate::Rz => simulator.rz(angle(), qubit),
                    QirGate::Cx => simulator.cx(qubit, second()),
                    QirGate::Cy => simulator.cy(qubit, second()),
                    QirGate::Cz => simulator.cz(qubit, second()),
                    QirGate::Rxx => simulator.rxx(angle(), qubit, second()),
                    QirGate::Ryy => simulator.ryy(angle(), qubit, second()),
                    QirGate::Rzz => simulator.rzz(angle(), qubit, second()),
                    QirGate::Swap => simulator.swap(qubit, second()),
                }
            }
            QirCall::Measure { qubit, result } => {
                simulator.mz(*qubit as usize, *result as usize);
            }
            QirCall::MeasureReset { qubit, result } => {
                simulator.mresetz(*qubit as usize, *result as usize);
            }
            QirCall::Reset { qubit } => simulator.resetz(*qubit as usize),
            QirCall::Move { qubit } => simulator.mov(*qubit as usize),
            QirCall::CorrelatedNoise { qubits, table } => {
                let targets: Vec<usize> = qubits.iter().map(|&qubit| qubit as usize).collect();
                simulator.correlated_noise_intrinsic(*table, &targets);
            }
            QirCall::PeekLoss { qubit, result } => {
                simulator.peek_loss(*qubit as usize, *result as usize);
            }
            QirCall::ReadoutNoise {
                zero_as_one,
                one_as_zero,
                result,
            } => simulator.apply_readout_noise(*zero_as_one, *one_as_zero, *result as usize),
        }
        if let Some(result) = call.result() {
            outcomes.push(QirOutcome {
                result,
                value: QirReadout::Missing,
            });
        }
    }
    let values = simulator.measurements();
    for outcome in &mut outcomes[first..] {
        outcome.value = match values[outcome.result as usize] {
            MeasurementResult::Zero => QirReadout::Zero,
            MeasurementResult::One => QirReadout::One,
            MeasurementResult::Loss => QirReadout::Loss,
        };
    }
}
