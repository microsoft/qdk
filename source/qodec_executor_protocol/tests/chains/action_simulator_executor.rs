// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ActionSimulatorExecutor`]: a qodec's bottom ISA run per shot on an
//! action simulator reads what that simulator reads driven directly, from
//! the shot's own stream of the seed, and keeps each shot's errors.

use executor_protocol::{Driver, ExecuteRefusal, Executor, ShotSet};
use qodec_executor_protocol::ActionSimulatorExecutor;

use crate::SEED;
use crate::direct_runtime::{full_state, run_directly, simulator, stabilizer};
use crate::programs::Program;
use crate::qodecs;
use crate::top_isa_driver::{ExecuteEnd, ShotRun, TopIsaDriver};

/// Three qubits, each measured after a Hadamard: three random bits per
/// shot, in Stim's ISA, the bottom of Steane's qodec.
const COINS: &str = "R 0; H 0; M 0; R 1; H 1; M 1 | R 2; H 2; M 2";

/// The qubits of the state-vector simulator.
const QUBITS: u32 = 3;

/// Runs `program`, of `executor`'s ISA, on `shots`.
fn run(executor: &mut ActionSimulatorExecutor, program: &Program, shots: &ShotSet) -> Vec<ShotRun> {
    let isa = qodecs::model("steane/steane.qodec.yaml").bottom;
    (Driver::new(TopIsaDriver::new(&isa, program)) >> Executor::new(executor))
        .expect("the driver sends the executor's ISA")
        .run(shots, SEED)
        .expect("the program runs")
}

/// On both simulators, each shot reads the bits the simulator reads under
/// Qodec's translator, driven directly from the shot's stream of the seed.
#[test]
fn each_shot_reads_what_its_simulator_reads_driven_directly() {
    let bottom = qodecs::model("steane/steane.qodec.yaml").bottom;
    let program = Program::parse(COINS);
    let mut stabilizers =
        ActionSimulatorExecutor::stabilizer(&bottom).expect("Stim's ISA is a protocol set");
    let mut full_states =
        ActionSimulatorExecutor::full_state(&bottom, QUBITS).expect("Stim's ISA is a protocol set");

    for run in run(&mut stabilizers, &program, &ShotSet::ids(0..16)) {
        let mut direct = simulator(&bottom, stabilizer(run.stream));
        assert_eq!(run.readouts, run_directly(&bottom, &program, &mut direct));
    }
    for run in run(&mut full_states, &program, &ShotSet::ids(0..16)) {
        let mut direct = simulator(&bottom, full_state(run.stream, QUBITS as usize));
        assert_eq!(run.readouts, run_directly(&bottom, &program, &mut direct));
    }
}

/// A shot reads the same bits run alone as among other shots: it draws
/// from its own stream, not from the batch's.
#[test]
fn each_shot_draws_from_its_own_stream_of_the_seed() {
    let bottom = qodecs::model("steane/steane.qodec.yaml").bottom;
    let program = Program::parse(COINS);
    let mut executor =
        ActionSimulatorExecutor::stabilizer(&bottom).expect("Stim's ISA is a protocol set");

    let together = run(&mut executor, &program, &ShotSet::ids(0..16));
    assert!(
        together
            .iter()
            .any(|run| run.readouts != together[0].readouts),
        "precondition: the shots read different bits"
    );

    for shot in [5, 11] {
        let alone = run(&mut executor, &program, &ShotSet::ids([shot]));
        assert_eq!(
            alone[0].readouts, together[shot as usize].readouts,
            "shot {shot}"
        );
    }
}

/// A rotation by a non-Clifford angle, which the stabilizer simulator
/// cannot run, fails the shot and loses it: the shot sent again is refused
/// as an earlier failure, before anything runs.
#[test]
fn a_shot_the_simulator_failed_is_refused_when_sent_again() {
    let bottom = qodecs::model("repetition3/repetition3.qodec.yaml").bottom;
    let program = Program::parse("R 0; rotate_z 0 theta=0.5 | M 0");
    let mut executor =
        ActionSimulatorExecutor::stabilizer(&bottom).expect("the example's ISA is a protocol set");

    let runs = (Driver::new(TopIsaDriver::new(&bottom, &program).going_on_after_errors())
        >> Executor::new(&mut executor))
    .expect("the driver sends the executor's ISA")
    .run(&ShotSet::ids([3]), SEED)
    .expect("the driver goes on after errors");

    let [first, second] = runs[0].ends.as_slice() else {
        panic!("two `execute`s: {:?}", runs[0].ends);
    };
    assert!(
        matches!(first, ExecuteEnd::Failed { .. }),
        "precondition: the rotation fails: {first:?}"
    );
    assert!(
        matches!(second, ExecuteEnd::Refused(ExecuteRefusal::EarlierFailure(shot)) if shot.get() == 3),
        "{second:?}"
    );
}
