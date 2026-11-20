// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QodecLayerAdapter`]: a qodec's layers in a chain read what Qodec's
//! runtime reads driven directly, however they are bracketed, and keep each
//! shot's state and errors as that runtime does.

use executor_protocol::{Adapter, Driver, ExecuteRefusal, Executor, ShotConditions, ShotSet};
use qodec_executor_protocol::{ActionSimulatorExecutor, QodecLayerAdapter};
use qodec_runtime_recipes::{QodecModel, runtime};

use crate::SEED;
use crate::direct_runtime::{run_directly, simulator, stabilizer};
use crate::executor_spy::ExecutorSpy;
use crate::programs::Program;
use crate::qodecs;
use crate::top_isa_driver::{ExecuteEnd, ShotRun, TopIsaDriver};

/// The shots of each run.
const SHOTS: u32 = 16;

/// Runs `driver` on every layer of `model` over a stabilizer simulator.
fn on_stabilizer(model: &QodecModel, driver: TopIsaDriver) -> Vec<ShotRun> {
    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    (Driver::new(driver)
        >> Adapter::new(QodecLayerAdapter::whole(model).expect("the example's layers assemble"))
        >> Executor::new(&mut executor))
    .expect("the layers fit the driver above and the simulator below")
    .run(&ShotSet::ids(0..SHOTS), SEED)
    .expect("the program runs")
}

/// A stabilizer simulator of `model`'s bottom ISA, spied on.
fn spied_stabilizer(model: &QodecModel) -> ExecutorSpy<ActionSimulatorExecutor> {
    ExecutorSpy::new(
        ActionSimulatorExecutor::stabilizer(&model.bottom)
            .expect("the example's bottom ISA is a protocol set"),
    )
}

/// Runs `driver` on every layer of `model` over `below`, on `shots`.
fn over(
    model: &QodecModel,
    driver: TopIsaDriver,
    below: &mut ExecutorSpy<ActionSimulatorExecutor>,
    shots: &ShotSet,
) -> Vec<ShotRun> {
    (Driver::new(driver)
        >> Adapter::new(QodecLayerAdapter::whole(model).expect("the example's layers assemble"))
        >> Executor::new(below))
    .expect("the layers fit the driver above and the part below")
    .run(shots, SEED)
    .expect("the run gives its outputs")
}

/// Qodec's examples, each with a program of Qodec's own REPL tests or a
/// random one: on every shot, the chain reads the outcomes, flags and
/// conditions that Qodec's runtime reads driven directly over a stabilizer
/// simulator drawing from the shot's stream of the seed.
#[test]
fn each_shot_reads_what_qodecs_runtime_reads_driven_directly() {
    for (path, text) in [
        (
            "steane/steane.qodec.yaml",
            "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
        ),
        ("steane/steane.qodec.yaml", "prepare_z 0; measure_x 0"),
        (
            "c4c6/qodec.yaml",
            "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0",
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0",
        ),
        (
            "five_qubit/five_qubit.qodec.yaml",
            "prepare_z 0; idle 0; cycle 0; measure_x 0",
        ),
        (
            "iceberg/iceberg.qodec.yaml",
            "prepare_z_all 0; idle 0; measure_z_all 0",
        ),
    ] {
        let model = qodecs::model(path);
        let plan = qodecs::plan(&model);
        let program = Program::parse(text);
        for run in on_stabilizer(&model, TopIsaDriver::new(&model.top, &program)) {
            let mut direct = runtime(&plan, simulator(plan.bottom(), stabilizer(run.stream)));
            assert_eq!(
                run.readouts,
                run_directly(plan.top(), &program, &mut direct),
                "{path}: {text}"
            );
        }
    }
}

/// A program sent one step per `execute`, as a debugger steps through it,
/// reads what it reads sent whole: each shot's layers keep its blocks'
/// state and frames from one `execute` to the next.
#[test]
fn a_shots_layers_keep_its_state_from_one_execute_to_the_next() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    for (stepped, whole) in [
        (
            "prepare_z 0 | prepare_x 1 | idle_ft 0 | h 0 | cnot 0 1 | measure_z 0 | measure_x 1",
            "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
        ),
        ("prepare_z 0 | measure_x 0", "prepare_z 0; measure_x 0"),
    ] {
        let readouts = |text| {
            on_stabilizer(&model, TopIsaDriver::new(&model.top, &Program::parse(text)))
                .into_iter()
                .map(|run| run.readouts)
                .collect::<Vec<_>>()
        };
        assert_eq!(readouts(stepped), readouts(whole), "{stepped}");
    }
}

/// C4/C6's two layers, one adapter each, with room between them for a
/// part (a noise model, a debugger), read what one adapter of both reads.
#[test]
fn one_adapter_per_layer_reads_what_one_adapter_for_all_layers_reads() {
    let model = qodecs::model("c4c6/qodec.yaml");
    assert_eq!(model.layers.len(), 2, "precondition: C4/C6 has two layers");
    let program = Program::parse(
        "prepare_x_all 0; hadamard_all 0; mul_u 0; prepare_z_all 1; controlled_x_all 0 1; \
         measure_z_all 0; measure_x_all 1",
    );
    let whole = on_stabilizer(&model, TopIsaDriver::new(&model.top, &program));

    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    let layer = |k| QodecLayerAdapter::new(&model, k..k + 1).expect("each layer assembles alone");
    let per_layer = (Driver::new(TopIsaDriver::new(&model.top, &program))
        >> Adapter::new(layer(0))
        >> Adapter::new(layer(1))
        >> Executor::new(&mut executor))
    .expect("each layer fits the next")
    .run(&ShotSet::ids(0..SHOTS), SEED)
    .expect("the program runs");

    assert_eq!(
        per_layer
            .into_iter()
            .map(|run| run.readouts)
            .collect::<Vec<_>>(),
        whole
            .into_iter()
            .map(|run| run.readouts)
            .collect::<Vec<_>>()
    );
}

/// C4/C6's decoder cannot resolve the C6 measurement Qodec's REPL tests
/// end with, and says so in the result's status: the driver sees the shot
/// unresolved, through the protocol's conditions.
#[test]
fn the_conditions_the_layers_raise_reach_the_driver() {
    let model = qodecs::model("c4c6/qodec.yaml");
    let program = Program::parse("prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0");

    let runs = on_stabilizer(&model, TopIsaDriver::new(&model.top, &program));

    for run in runs {
        assert!(
            run.readouts.conditions.contains(ShotConditions::UNRESOLVED),
            "shot of stream {}: {:?}",
            run.stream,
            run.readouts.conditions
        );
    }
}

/// A Steane program sent to C4/C6's layers does not compose: the layers
/// accept their top ISA only.
#[test]
fn a_driver_of_another_isa_is_refused_at_composition() {
    let steane = qodecs::model("steane/steane.qodec.yaml");
    let c4c6 = qodecs::model("c4c6/qodec.yaml");
    let mut executor = ActionSimulatorExecutor::stabilizer(&c4c6.bottom)
        .expect("the example's bottom ISA is a protocol set");

    let composed = Driver::new(TopIsaDriver::new(
        &steane.top,
        &Program::parse("prepare_z 0; measure_z 0"),
    )) >> Adapter::new(
        QodecLayerAdapter::whole(&c4c6).expect("the example's layers assemble"),
    ) >> Executor::new(&mut executor);

    let Err(faults) = composed else {
        panic!("a Steane program composed with C4/C6's layers");
    };
    assert!(!faults.fit().is_empty(), "{faults}");
}

/// A repetition-code memory with a rotation lowers each logical block to
/// data and ancilla qubits: a state-vector simulator of fewer qubits than
/// those is refused when the chain is composed, and one of as many runs it.
#[test]
fn the_layers_ask_the_part_below_for_the_qubits_they_lower_their_blocks_to() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let program = Program::parse("prepare_z 0; idle 0; rotate_z 0 theta=0.5; measure_z 0");
    let composes = |qubits| {
        let mut executor = ActionSimulatorExecutor::full_state(&model.bottom, qubits)
            .expect("the example's bottom ISA is a protocol set");
        let composed = Driver::new(TopIsaDriver::new(&model.top, &program))
            >> Adapter::new(
                QodecLayerAdapter::whole(&model).expect("the example's layers assemble"),
            )
            >> Executor::new(&mut executor);
        match composed {
            Ok(_) => true,
            Err(faults) => {
                assert!(faults.fit().is_empty(), "the sets fit: {faults}");
                assert!(faults.admission().is_some(), "{faults}");
                false
            }
        }
    };

    assert!(!composes(QUBITS - 1));
    assert!(composes(QUBITS));
}

/// The qubits repetition3's layers lower one block to, for the memory with
/// a rotation: its three data qubits and two ancillas.
const QUBITS: u32 = 5;

/// An error below the layers fails shot 3 and loses it there and in them:
/// the shot sent again, as a debugger stepping on would, together with
/// shot 1, is refused as an earlier failure before anything of that
/// `execute` runs, so shot 1, sent before it, reaches no part below
/// either. The error is a rotation by a non-Clifford angle, which a
/// stabilizer simulator cannot run, sent whole or in batches of one
/// instruction after the earlier ones of its layer's batch ran below; or a
/// refusal of a part below that takes no instruction at all: the layers'
/// work on the shot is lost with it, so the refusal fails the shot rather
/// than being passed up as one.
#[test]
fn a_shot_an_error_lost_is_refused_when_sent_again() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let program = Program::parse("prepare_z 0; rotate_z 0 theta=0.5 | prepare_z 0; measure_z 0");
    // Each case, and the most instructions the part below takes per batch.
    let cases = [
        ("the simulator fails", None),
        ("the simulator fails a later batch of a split", Some(1)),
        ("the part below refuses", Some(0)),
    ];
    for (case, most) in cases {
        let driver = TopIsaDriver::new(&model.top, &program)
            .sending_execute_to(0, &[1])
            .going_on_after_errors();

        let mut below = spied_stabilizer(&model);
        if let Some(most) = most {
            below = below.taking_at_most(most);
        }
        let runs = over(&model, driver, &mut below, &ShotSet::ids([1, 3]));

        let [first, second] = runs[1].ends.as_slice() else {
            panic!("{case}: two `execute`s for shot 3: {:?}", runs[1].ends);
        };
        let failed_as_expected = matches!(
            (most, first),
            (None | Some(1), ExecuteEnd::Failed { refusal: None, .. })
                | (
                    Some(0),
                    ExecuteEnd::Failed {
                        refusal: Some(ExecuteRefusal::TooManyInstructions { max: 0, .. }),
                        ..
                    },
                )
        );
        assert!(failed_as_expected, "{case}: the shot fails: {first:?}");
        if most == Some(1) {
            assert!(
                below
                    .received
                    .iter()
                    .filter(|batch| batch.shots.contains(&3))
                    .count()
                    > 1,
                "precondition: batches of shot 3 ran below before the failing one: {:?}",
                below.received
            );
        }
        let refused = |end: &ExecuteEnd| matches!(end, ExecuteEnd::Refused(ExecuteRefusal::EarlierFailure(shot)) if shot.get() == 3);
        assert!(refused(second), "{case}: {second:?}");
        assert!(
            matches!(runs[0].ends.as_slice(), [end] if refused(end)),
            "{case}: shot 1: {:?}",
            runs[0].ends
        );
        assert!(
            below.received.iter().all(|batch| !batch.shots.contains(&1)),
            "{case}: shot 1 reached the part below: {:?}",
            below.received
        );
    }
}

/// A runner runs again on the shots of its last run, in which a rotation by
/// a non-Clifford angle, which a stabilizer simulator cannot run, failed
/// and lost them: the layers forgot them when the run ended, so each fails
/// again as it failed then, rather than being refused as an earlier
/// failure.
#[test]
fn a_shot_lost_in_one_run_runs_afresh_in_the_next() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let program = Program::parse("prepare_z 0; rotate_z 0 theta=0.5 | measure_z 0");
    let driver = TopIsaDriver::new(&model.top, &program).going_on_after_errors();
    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    let mut runner = (Driver::new(driver)
        >> Adapter::new(QodecLayerAdapter::whole(&model).expect("the example's layers assemble"))
        >> Executor::new(&mut executor))
    .expect("the layers fit the driver above and the simulator below");
    let shots = ShotSet::ids([3, 7]);
    let mut run = || {
        runner
            .run(&shots, SEED)
            .expect("the driver goes on after errors")
            .into_iter()
            .map(|run| run.ends)
            .collect::<Vec<_>>()
    };

    let first = run();
    assert!(
        first
            .iter()
            .all(|ends| matches!(ends.as_slice(), [ExecuteEnd::Failed { .. }, _])),
        "precondition: the rotation fails every shot: {first:?}"
    );
    assert_eq!(run(), first);
}

/// Batches sent checked, or unchecked, reach the part below the layers
/// checked as they were sent: the layers send the part below the
/// instructions it accepts, but leave the checks to what their caller
/// asks.
#[test]
fn the_layers_send_batches_checked_as_the_ones_they_get() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let program = Program::parse("prepare_z 0; prepare_x 1 | cnot 0 1; measure_z 0; measure_x 1");
    for checked in [false, true] {
        let driver = TopIsaDriver::new(&model.top, &program);
        let driver = if checked { driver.checked() } else { driver };

        let mut below = spied_stabilizer(&model);
        over(&model, driver, &mut below, &ShotSet::ids(0..4));

        assert!(
            !below.received.is_empty(),
            "precondition: batches went below"
        );
        assert!(
            below.received.iter().all(|batch| batch.checked == checked),
            "sent checked: {checked}; received: {:?}",
            below.received
        );
    }
}

/// A part below that takes at most two instructions per batch, as a
/// device with a small instruction memory may, gets each of the layers'
/// batches in batches of two, the last holding what is left, each
/// instruction once; checked or not, the shots read what they read with no
/// such limit.
#[test]
fn the_layers_split_their_batches_to_the_most_the_part_below_takes() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let program = Program::parse(
        "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
    );
    for checked in [false, true] {
        let run = |below: &mut ExecutorSpy<ActionSimulatorExecutor>| {
            let driver = TopIsaDriver::new(&model.top, &program);
            let driver = if checked { driver.checked() } else { driver };
            over(&model, driver, below, &ShotSet::ids(0..SHOTS))
                .into_iter()
                .map(|run| run.readouts)
                .collect::<Vec<_>>()
        };
        let mut unlimited = spied_stabilizer(&model);
        let mut limited = spied_stabilizer(&model).taking_at_most(2);

        let expected = run(&mut unlimited);
        let readouts = run(&mut limited);

        assert!(
            unlimited
                .received
                .iter()
                .any(|batch| batch.instructions > 2),
            "precondition: the layers send batches of more than two instructions"
        );
        let in_twos = unlimited
            .received
            .iter()
            .map(|batch| batch.instructions.div_ceil(2))
            .sum::<usize>();
        assert!(
            limited.received.len() == in_twos
                && limited.received.iter().all(|batch| batch.instructions <= 2),
            "checked: {checked}; {in_twos} batches of at most two expected: {:?}",
            limited.received
        );
        let sent = |below: &ExecutorSpy<_>| {
            below
                .received
                .iter()
                .map(|batch| batch.instructions)
                .sum::<usize>()
        };
        assert_eq!(
            sent(&limited),
            sent(&unlimited),
            "checked: {checked}; instructions sent below"
        );
        assert_eq!(readouts, expected, "checked: {checked}");
    }
}

/// A rotation sent without its angle, in an unchecked batch, is a call
/// Qodec's runtime rejects before running anything: it fails its `execute`
/// without losing the shot, and the next `execute` measures the block the
/// first prepared.
#[test]
fn a_call_the_layers_reject_fails_its_execute_and_keeps_the_shot() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let program = Program::parse("prepare_z 0 | rotate_z 0 theta=0.5 | measure_z 0");
    let driver = TopIsaDriver::new(&model.top, &program)
        .without_its_arguments(1, 0)
        .going_on_after_errors();

    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    let runs = (Driver::new(driver)
        >> Adapter::new(QodecLayerAdapter::whole(&model).expect("the example's layers assemble"))
        >> Executor::new(&mut executor))
    .expect("the layers fit the driver above and the simulator below")
    .run(&ShotSet::ids([3]), SEED)
    .expect("the driver goes on after errors");

    let [ExecuteEnd::Ran, ExecuteEnd::Failed { .. }, ExecuteEnd::Ran] = runs[0].ends.as_slice()
    else {
        panic!("only the malformed call fails: {:?}", runs[0].ends);
    };
    assert_eq!(runs[0].readouts.values, [Some(false)]);
}

/// C4/C6's lower layer alone, below a driver of its ISA, given a Hadamard
/// on two blocks, which Qodec's check of a call's arguments lets through
/// (it does not count blocks): the fault the layer raises when it binds the
/// call's blocks names it as layer 1, its place in the whole qodec, as
/// Qodec's runtime running every layer names it.
#[test]
fn a_layers_fault_names_its_place_in_the_whole_qodec() {
    let model = qodecs::model("c4c6/qodec.yaml");
    let program = Program::parse("prepare_z_all 0; hadamard_all 0");
    let driver = TopIsaDriver::new(&model.layers[1].upper, &program)
        .with_a_block_too_many(0, 1)
        .going_on_after_errors();

    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    let runs = (Driver::new(driver)
        >> Adapter::new(QodecLayerAdapter::new(&model, 1..2).expect("the layer assembles alone"))
        >> Executor::new(&mut executor))
    .expect("the layer fits the driver above and the simulator below")
    .run(&ShotSet::ids([0]), SEED)
    .expect("the driver goes on after errors");

    let [
        ExecuteEnd::Failed {
            fault: Some(fault), ..
        },
    ] = runs[0].ends.as_slice()
    else {
        panic!(
            "the malformed call fails as a Qodec fault: {:?}",
            runs[0].ends
        );
    };
    assert_eq!(fault.layer, Some(1), "{fault}");
}

/// Steane's program in checked batches reads what it reads unchecked: the
/// layers send the part below batches its boundary accepts.
#[test]
fn checked_batches_read_what_unchecked_ones_read() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let program = Program::parse(
        "prepare_z 0; prepare_x 1; idle_ft 0 | h 0; cnot 0 1 | measure_z 0; measure_x 1",
    );
    let readouts = |driver| {
        on_stabilizer(&model, driver)
            .into_iter()
            .map(|run| run.readouts)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        readouts(TopIsaDriver::new(&model.top, &program).checked()),
        readouts(TopIsaDriver::new(&model.top, &program))
    );
}
