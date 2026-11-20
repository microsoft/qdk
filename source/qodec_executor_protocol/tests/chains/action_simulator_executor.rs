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
/// cannot run, fails shot 3 and loses it: the shot sent the rotation
/// again, together with shot 1, in one batch or after shot 1's in a batch
/// of its own, is refused as an earlier failure before anything runs, so
/// shot 1, which the same rotation would fail, ends refused, not failed.
#[test]
fn a_shot_the_simulator_failed_is_refused_when_sent_again() {
    let bottom = qodecs::model("repetition3/repetition3.qodec.yaml").bottom;
    let program = Program::parse("R 0; rotate_z 0 theta=0.5 | R 0; rotate_z 0 theta=0.5");
    let together: &[&[usize]] = &[&[0, 1]];
    let shot_3_after_shot_1: &[&[usize]] = &[&[0], &[1]];
    for batches in [together, shot_3_after_shot_1] {
        let driver = TopIsaDriver::new(&bottom, &program)
            .sending_execute_to(0, &[1])
            .sending_execute_in_batches(1, batches)
            .going_on_after_errors();
        let mut executor = ActionSimulatorExecutor::stabilizer(&bottom)
            .expect("the example's ISA is a protocol set");

        let runs = (Driver::new(driver) >> Executor::new(&mut executor))
            .expect("the driver sends the executor's ISA")
            .run(&ShotSet::ids([1, 3]), SEED)
            .expect("the driver goes on after errors");

        let [first, second] = runs[1].ends.as_slice() else {
            panic!("{batches:?}: two `execute`s for shot 3: {:?}", runs[1].ends);
        };
        assert!(
            matches!(first, ExecuteEnd::Failed { .. }),
            "precondition: the rotation fails: {first:?}"
        );
        let refused = |end: &ExecuteEnd| matches!(end, ExecuteEnd::Refused(ExecuteRefusal::EarlierFailure(shot)) if shot.get() == 3);
        assert!(refused(second), "{batches:?}: {second:?}");
        assert!(
            matches!(runs[0].ends.as_slice(), [end] if refused(end)),
            "{batches:?}: shot 1: {:?}",
            runs[0].ends
        );
    }
}

/// A correction conditioned on the parity of the shot's latest readout, in
/// an unchecked batch to an executor whose needs state no parity: Qodec's
/// calls have no form for it, so its `execute` fails naming the
/// instruction, rather than sending the correction without its condition,
/// and runs nothing of the shot, which goes on: the next `execute` reads
/// what it reads with the failing one never sent.
#[test]
fn a_readout_parity_argument_fails_its_execute_and_keeps_the_shot() {
    let bottom = qodecs::model("steane/steane.qodec.yaml").bottom;
    let mut executor =
        ActionSimulatorExecutor::stabilizer(&bottom).expect("Stim's ISA is a protocol set");

    let runs = (Driver::new(
        TopIsaDriver::new(
            &bottom,
            &Program::parse("R 0; M 0; R 1; X 1 | Z_if 1 correction=1 | M 1"),
        )
        .with_a_readout_parity(1, 0)
        .going_on_after_errors(),
    ) >> Executor::new(&mut executor))
    .expect("the driver sends the executor's ISA")
    .run(&ShotSet::ids([0]), SEED)
    .expect("the driver goes on after errors");
    let expected = run(
        &mut executor,
        &Program::parse("R 0; M 0; R 1; X 1 | M 1"),
        &ShotSet::ids([0]),
    );

    let [
        ExecuteEnd::Ran,
        ExecuteEnd::Failed { message, .. },
        ExecuteEnd::Ran,
    ] = runs[0].ends.as_slice()
    else {
        panic!("only the parity fails its `execute`: {:?}", runs[0].ends);
    };
    assert!(message.contains("Z_if"), "{message}");
    assert_eq!(runs[0].readouts, expected[0].readouts);
}
