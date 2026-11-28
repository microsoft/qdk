// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the grouped run's tests share: a run whose steps a test takes by
//! hand, on groups and failures it sets ([`in_a_run`]), and how a run
//! failed ([`failed_at`]).

use std::sync::atomic::AtomicBool;

use crate::mps_simulator::grouped_run::GroupedRun;
use crate::mps_simulator::grouped_run::first_failure::GroupFailure;
use crate::mps_simulator::grouped_run::shot_group::ShotGroup;
use crate::mps_simulator::lowering::SiteGate;
use crate::mps_simulator::test_utils::reference_spy::{
    ReferenceSpy, ReferenceSpyError, ReferenceSpyState,
};
use crate::mps_simulator::{DEFAULT_MAX_BOND_DIMENSION, MpsResumerError, MpsRunReport};
use crate::qir::{QirGate, QirMachine, QirProgram, QirResumeError, QirShotId, QirTrap};

/// A group of shots on the spy's states.
pub(super) type SpyGroup = ShotGroup<ReferenceSpyState>;

/// Gives what `scene` gives on a grouped run of `shots` of `program` on
/// `engine`, started as [`MpsResumer`](crate::mps_simulator::MpsResumer)
/// starts it, at seed 7 and the default max bond dimension, stopped by
/// `stop_flag`, if any. So a test takes the run's steps by hand, on groups
/// and failures it sets.
pub(super) fn in_a_run<R>(
    engine: &ReferenceSpy,
    program: &QirProgram,
    shots: &[QirShotId],
    stop_flag: Option<&AtomicBool>,
    scene: impl FnOnce(GroupedRun<'_, ReferenceSpy>) -> R,
) -> R {
    let machine = QirMachine::new(program);
    let mut report = MpsRunReport::new(DEFAULT_MAX_BOND_DIMENSION.get());
    let run = GroupedRun::start(
        engine,
        &machine,
        shots,
        7,
        DEFAULT_MAX_BOND_DIMENSION,
        stop_flag,
        &mut report,
    )
    .expect("the program's qubits make a chain");
    scene(run)
}

/// The group `run` starts with, of every shot, taken off its stack.
pub(super) fn the_first_group(run: &mut GroupedRun<'_, ReferenceSpy>) -> SpyGroup {
    run.waiting
        .pop()
        .expect("a run starts with one group of every shot")
}

/// `gate` on `site`, as a step's gate.
pub(super) fn gate_on(gate: QirGate, site: usize) -> SiteGate {
    SiteGate::new(&gate.matrix_at(None), &[site])
}

/// A group's failure for the stop flag, which a test keeps as a run's
/// failure.
pub(super) fn stopped() -> GroupFailure<ReferenceSpyError> {
    GroupFailure::Resumer(MpsResumerError::Stopped)
}

/// How a run failed.
#[derive(Debug, PartialEq)]
pub(super) enum Failure {
    /// A shot trapped, on a branch on a register holding 2.
    Trap,
    /// The engine failed a call, as the spy's failing engines do.
    Engine,
    /// The stop flag stopped the run.
    Stopped,
}

/// The shot `error` names, and how it failed the run. It panics, naming
/// `scene`, on any other failure than a [`Failure`].
pub(super) fn failed_at(
    error: &QirResumeError<MpsResumerError<ReferenceSpyError>>,
    scene: &str,
) -> (QirShotId, Failure) {
    match error {
        QirResumeError::Trapped {
            shot,
            trap: QirTrap::NotABit { value: 2, .. },
        } => (*shot, Failure::Trap),
        QirResumeError::Resumer {
            shot,
            error: MpsResumerError::Engine(ReferenceSpyError::Device),
        } => (*shot, Failure::Engine),
        QirResumeError::Resumer {
            shot,
            error: MpsResumerError::Stopped,
        } => (*shot, Failure::Stopped),
        other => panic!("{scene}: a trap, the engine or the stop flag fails it: {other:?}"),
    }
}
