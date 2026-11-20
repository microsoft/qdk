// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ActionSimulatorExecutor`]: a qodec's bottom ISA run per shot on an
//! action simulator reads what that simulator reads driven directly, from
//! the shot's own stream of the seed, and keeps each shot's errors.

use std::sync::Arc;

use executor_protocol::{Driver, ExecuteRefusal, Executor, ShotSet};
use qodec_executor_protocol::ActionSimulatorExecutor;
use qodec_runtime_recipes::Isa;

use crate::SEED;
use crate::direct_runtime::{full_state, run_directly, simulator, stabilizer};
use crate::programs::Program;
use crate::qodecs;
use crate::surface_programs::{SURFACE_LABEL_REUSED, SURFACE_REPREPARED};
use crate::top_isa_driver::{ExecuteEnd, ShotRun, TopIsaDriver};

/// Three qubits, each measured after a Hadamard: three random bits per
/// shot, in Stim's ISA, the bottom of Steane's qodec.
const COINS: &str = "R 0; H 0; M 0; R 1; H 1; M 1 | R 2; H 2; M 2";

/// The qubits of the state-vector simulator.
const QUBITS: u32 = 3;

/// Runs `program`, of Steane's bottom ISA, Stim's, on `executor`, on
/// `shots`.
fn run(executor: &mut ActionSimulatorExecutor, program: &Program, shots: &ShotSet) -> Vec<ShotRun> {
    let isa = qodecs::model("steane/steane.qodec.yaml").bottom;
    run_of(&isa, executor, program, shots)
}

/// Runs `program`, of `isa`, `executor`'s ISA, on `shots`.
fn run_of(
    isa: &Arc<Isa>,
    executor: &mut ActionSimulatorExecutor,
    program: &Program,
    shots: &ShotSet,
) -> Vec<ShotRun> {
    (Driver::new(TopIsaDriver::new(isa, program)) >> Executor::new(executor))
        .expect("the driver sends the executor's ISA")
        .run(shots, SEED)
        .expect("the program runs")
}

/// On both simulators, each shot reads the bits the simulator reads under
/// Qodec's translator, driven directly from the shot's stream of the seed.
/// On surface's two block types, a call may prepare a block at the label
/// of a live one of its own type, or at the label of a block of another
/// type that has ended: neither shares a label between two live blocks.
#[test]
fn each_shot_reads_what_its_simulator_reads_driven_directly() {
    let stim = qodecs::model("steane/steane.qodec.yaml").bottom;
    let surface = qodecs::model("surface/surface.qodec.yaml").top;
    // Each case: the executor's ISA, its program, and whether it runs on a
    // state-vector simulator rather than a stabilizer one.
    for (isa, text, on_full_state) in [
        (&stim, COINS, false),
        (&stim, COINS, true),
        (&surface, SURFACE_REPREPARED, false),
        (&surface, SURFACE_LABEL_REUSED, false),
    ] {
        let program = Program::parse(text);
        let mut executor = if on_full_state {
            ActionSimulatorExecutor::full_state(isa, QUBITS)
        } else {
            ActionSimulatorExecutor::stabilizer(isa)
        }
        .expect("the example's ISA is a protocol set");

        for run in run_of(isa, &mut executor, &program, &ShotSet::ids(0..16)) {
            let engine = if on_full_state {
                full_state(run.stream, QUBITS as usize)
            } else {
                stabilizer(run.stream)
            };
            let mut direct = simulator(isa, engine);
            assert_eq!(
                run.readouts,
                run_directly(isa, &program, &mut direct),
                "{text}, on a state vector: {on_full_state}"
            );
        }
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

/// An error fails shot 3 and loses it: the shot sent a rotation by a
/// non-Clifford angle again, together with shot 1, in one batch or after
/// shot 1's in a batch of its own, is refused as an earlier failure before
/// anything runs, so shot 1, which the same rotation would fail, ends
/// refused, not failed. The error is that rotation, which the stabilizer
/// simulator cannot run, or a rotation sent without its angle, unchecked,
/// which Qodec's translator cannot run (where the layers would keep the
/// shot).
#[test]
fn a_shot_an_error_lost_is_refused_when_sent_again() {
    /// A driver with one of its calls made malformed.
    type Malformed = fn(TopIsaDriver) -> TopIsaDriver;
    let bottom = qodecs::model("repetition3/repetition3.qodec.yaml").bottom;
    let program = Program::parse("R 0; rotate_z 0 theta=0.5 | R 0; rotate_z 0 theta=0.5");
    let together: &[&[usize]] = &[&[0, 1]];
    let shot_3_after_shot_1: &[&[usize]] = &[&[0], &[1]];
    // Each case: the batches, by position in shots [1, 3], of the second
    // `execute`, and the first `execute`'s malformed call, if one.
    let cases: [(_, Malformed); 3] = [
        (together, |driver| driver),
        (shot_3_after_shot_1, |driver| driver),
        (together, |driver| driver.without_its_arguments(0, 1)),
    ];
    for (batches, malformed) in cases {
        let driver = malformed(TopIsaDriver::new(&bottom, &program))
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

/// An instruction the executor cannot translate into Qodec's calls fails
/// its `execute` naming it, rather than running something else, and runs
/// nothing of the shot, which goes on: the next `execute` reads what it
/// reads with the failing one never sent. A correction conditioned on the
/// parity of the shot's latest readout, in an unchecked batch to an
/// executor whose needs state no parity, is an argument Qodec's calls have
/// no form for. A new surface patch 0 prepared while the merged patch 0 a
/// lattice-surgery merge gave is live, on an executor of surface's top
/// ISA, is two blocks of the protocol, at one index of two types, but one
/// label in Qodec, whose translator would end the merged patch to prepare
/// the new one: the failure names both types and the label, whether the
/// merge came in an earlier `execute` or earlier in the same one, and in a
/// checked batch, which the protocol's boundary lets through, as much as
/// in an unchecked one.
#[test]
fn an_instruction_the_executor_cannot_translate_fails_its_execute_and_keeps_the_shot() {
    /// A driver made to send its program otherwise: with one of its calls
    /// malformed, or in checked batches.
    type Altered = fn(TopIsaDriver) -> TopIsaDriver;
    let steane = qodecs::model("steane/steane.qodec.yaml").bottom;
    let surface = qodecs::model("surface/surface.qodec.yaml").top;
    let clash: &[&str] = &[
        "prepare_z",
        "`surface` block 0",
        "`merged` block 0",
        "label 0",
    ];
    // Each case: the executor's ISA, its program, that program without
    // the failing `execute`, how the driver sends it otherwise, if it
    // does, and what the failure names.
    let cases: [(_, _, _, Altered, &[&str]); 4] = [
        (
            &steane,
            "R 0; M 0; R 1; X 1 | Z_if 1 correction=1 | M 1",
            "R 0; M 0; R 1; X 1 | M 1",
            |driver| driver.with_a_readout_parity(1, 0),
            &["Z_if"],
        ),
        (
            &surface,
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | prepare_z 0 | measure_merged_z 0",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | measure_merged_z 0",
            |driver| driver,
            clash,
        ),
        (
            &surface,
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | prepare_z 0 | measure_merged_z 0",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | measure_merged_z 0",
            TopIsaDriver::checked,
            clash,
        ),
        (
            &surface,
            "prepare_z 0; prepare_z 1 | merge_zz 0 1; prepare_z 0 | measure_z 0; measure_z 1",
            "prepare_z 0; prepare_z 1 | measure_z 0; measure_z 1",
            |driver| driver,
            clash,
        ),
    ];
    for (isa, text, without, altered, named) in cases {
        let mut executor =
            ActionSimulatorExecutor::stabilizer(isa).expect("the example's ISA is a protocol set");
        let mut run = |driver: TopIsaDriver| {
            (Driver::new(driver) >> Executor::new(&mut executor))
                .expect("the driver sends the executor's ISA")
                .run(&ShotSet::ids([0]), SEED)
                .expect("the driver goes on after errors")
        };

        let runs =
            run(altered(TopIsaDriver::new(isa, &Program::parse(text))).going_on_after_errors());
        let expected = run(TopIsaDriver::new(isa, &Program::parse(without)));

        let [
            ExecuteEnd::Ran,
            ExecuteEnd::Failed { message, .. },
            ExecuteEnd::Ran,
        ] = runs[0].ends.as_slice()
        else {
            panic!(
                "{text}: only the untranslatable call fails: {:?}",
                runs[0].ends
            );
        };
        for named in named {
            assert!(message.contains(named), "{text}: names {named}: {message}");
        }
        assert_eq!(runs[0].readouts, expected[0].readouts, "{text}");
    }
}
