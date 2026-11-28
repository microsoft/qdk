// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the MPS simulation method's tests share: a run of a program's
//! shots, the records a shot reads, and a Bell pair and the engine calls a
//! lone shot of it makes. [`reference_spy`] holds a spy over the exact
//! engine, which records the calls and stands in for an engine that
//! truncates or fails; [`programs`] writes the QIR programs the tests run.

pub(super) mod programs;
pub(super) mod reference_spy;

use crate::qir::{QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, expect_ready};
use crate::{MeasurementResult, OutputRecord};
use tensornet::MpsEngine;

use crate::bytecode::encoding::{OPID_CX, OPID_H};
use crate::mps_simulator::{MpsResumer, MpsResumerError};
use programs::{gate1, gate2, mz, program, ret};
use reference_spy::Call;

/// The records of a run's shots, in the order of its shots.
pub(super) type Records = Vec<Vec<OutputRecord>>;

/// A run's records, or the error that failed it, on an engine failing with
/// `E`.
pub(super) type RunResult<E> = Result<Records, QirResumeError<MpsResumerError<E>>>;

/// Runs `shots` of `program` on `resumer`.
pub(super) fn run<E: MpsEngine>(
    resumer: &mut MpsResumer<E>,
    program: &QirProgram,
    shots: &[QirShotId],
) -> RunResult<E::Error> {
    expect_ready(resumer.run(&QirMachine::new(program), shots))
}

/// The records of a shot that read `bits`, as results.
pub(super) fn results(bits: &[u8]) -> Vec<OutputRecord> {
    bits.iter()
        .map(|&bit| {
            OutputRecord::Result(if bit == 1 {
                MeasurementResult::One
            } else {
                MeasurementResult::Zero
            })
        })
        .collect()
}

/// The ids `ids`, as shots.
pub(super) fn ids(ids: &[u32]) -> Vec<QirShotId> {
    ids.iter().copied().map(QirShotId).collect()
}

/// A Bell pair read into two results: a run of one shot makes each call
/// of [`LONE_SHOT_CALLS`].
pub(super) fn bell() -> QirProgram {
    program(
        2,
        &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    )
}

/// The engine's calls a run of one shot of [`bell`] makes: it prepares a
/// state, applies the pair's gates, reads the largest bond dimension, reads
/// P(One) and projects at each measurement, and reads the retained weight
/// at the end. Only a split forks, which a lone shot never makes: a failed
/// fork has its own tests, in
/// `mps_simulator/grouped_run/site_steps/tests.rs`.
pub(super) const LONE_SHOT_CALLS: [Call; 6] = [
    Call::Prepare,
    Call::Apply,
    Call::Shape,
    Call::ProbabilityOfOne,
    Call::Project,
    Call::RetainedWeight,
];
