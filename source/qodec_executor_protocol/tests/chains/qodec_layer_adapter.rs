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

/// A rotation by a non-Clifford angle, which a stabilizer simulator cannot
/// run, fails the shot below the layers and loses it there and in them:
/// the shot sent again, as a debugger stepping on would, is refused as an
/// earlier failure, before anything runs.
#[test]
fn a_shot_an_error_lost_is_refused_when_sent_again() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let program = Program::parse("prepare_z 0; rotate_z 0 theta=0.5 | measure_z 0");

    let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set");
    let runs = (Driver::new(TopIsaDriver::new(&model.top, &program).going_on_after_errors())
        >> Adapter::new(QodecLayerAdapter::whole(&model).expect("the example's layers assemble"))
        >> Executor::new(&mut executor))
    .expect("the layers fit the driver above and the simulator below")
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
/// on two blocks: the fault it raises names it as layer 1, its place in the
/// whole qodec, as Qodec's runtime running every layer names it.
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
