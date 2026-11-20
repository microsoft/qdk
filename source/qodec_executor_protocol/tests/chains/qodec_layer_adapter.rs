// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QodecLayerAdapter`]: a qodec's layers in a chain read what Qodec's
//! runtime reads driven directly, however they are bracketed, and keep each
//! shot's state and errors as that runtime does.

use std::ops::Range;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use executor_protocol::{
    Adapter, AdapterSetup, BlockType, Driver, ExecuteRefusal, Executor, InstructionDef,
    InstructionSet, Needs, Offer, Opcode, ShotConditions, ShotSet,
};
use qodec_executor_protocol::{ActionSimulatorExecutor, QodecLayerAdapter, QodecLayerAdapterError};
use qodec_runtime_recipes::{FaultKind, Isa, QodecModel, runtime};

use crate::SEED;
use crate::direct_runtime::{answering_no_bit, full_state, run_directly, simulator, stabilizer};
use crate::executor_spy::{ExecutorSpy, ReceivedBatch};
use crate::late_answer_executor::{LateAnswerExecutor, Wake};
use crate::programs::Program;
use crate::qodecs;
use crate::surface_programs::{SURFACE_LABEL_REUSED, SURFACE_REPREPARED};
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
/// simulator drawing from the shot's stream of the seed. The same holds
/// over a part below that numbers the layers' bottom ISA in another order,
/// as the fit lets it: the layers send it its own opcodes; and over one
/// that loses every value, as a device that lost its measurements does:
/// the layers get no bit for them, as Qodec's runtime over a backend
/// answering no bit does. On surface's two block types, a call may prepare
/// a block at the label of a live one of its own type, or at the label of
/// a block of another type that has ended: neither shares a label between
/// two live blocks. A gadget may pass a Pauli in Qodec's shorthand, `X`
/// for `X_0`, as the two-type fixture's `x` does.
#[test]
fn each_shot_reads_what_qodecs_runtime_reads_driven_directly() {
    let steane_gadgets =
        "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1";
    // Each case: its qodec, its program, and how the part below differs
    // from a stabilizer simulator of the layers' bottom ISA.
    for (path, text, below_is) in [
        ("steane/steane.qodec.yaml", steane_gadgets, Below::Simulator),
        (
            "steane/steane.qodec.yaml",
            steane_gadgets,
            Below::Renumbered,
        ),
        (
            "steane/steane.qodec.yaml",
            "prepare_z 0; measure_x 0",
            Below::Simulator,
        ),
        (
            "steane/steane.qodec.yaml",
            "prepare_z 0; measure_z 0",
            Below::LosingEveryValue,
        ),
        (
            "c4c6/qodec.yaml",
            "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0",
            Below::Simulator,
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0",
            Below::Simulator,
        ),
        (
            "surface/surface.qodec.yaml",
            SURFACE_REPREPARED,
            Below::Simulator,
        ),
        (
            "surface/surface.qodec.yaml",
            SURFACE_LABEL_REUSED,
            Below::Simulator,
        ),
        (
            "five_qubit/five_qubit.qodec.yaml",
            "prepare_z 0; idle 0; cycle 0; measure_x 0",
            Below::Simulator,
        ),
        (
            "iceberg/iceberg.qodec.yaml",
            "prepare_z_all 0; idle 0; measure_z_all 0",
            Below::Simulator,
        ),
        (TWO_LOWER_TYPES, TWO_PAIRS_ONE_FLIPPED, Below::Simulator),
    ] {
        let model = qodecs::model(path);
        let plan = qodecs::plan(&model);
        let program = Program::parse(text);
        let mut below = spied_stabilizer(&model);
        match below_is {
            Below::Simulator => {}
            Below::Renumbered => below = below.numbering_in_reverse(&model.bottom),
            Below::LosingEveryValue => below = below.losing_every_value(),
        }
        let runs = over(
            &model,
            TopIsaDriver::new(&model.top, &program),
            &mut below,
            &ShotSet::ids(0..SHOTS),
        );
        for run in runs {
            let engine = simulator(plan.bottom(), stabilizer(run.stream));
            let expected = if matches!(below_is, Below::LosingEveryValue) {
                run_directly(
                    plan.top(),
                    &program,
                    &mut runtime(&plan, answering_no_bit(engine)),
                )
            } else {
                run_directly(plan.top(), &program, &mut runtime(&plan, engine))
            };
            assert_eq!(
                run.readouts, expected,
                "{path}: {text}, below: {below_is:?}"
            );
        }
    }
}

/// How the part below a chain differs from a stabilizer simulator of the
/// layers' bottom ISA.
#[derive(Clone, Copy, Debug)]
enum Below {
    /// It does not.
    Simulator,
    /// It numbers the bottom ISA's instructions in reverse.
    Renumbered,
    /// It loses every value: each reads as missing.
    LosingEveryValue,
}

/// A one-layer test qodec whose lower ISA has two block types, `a` and
/// `b`: one of each per `pair` logical qubit, and one `b` block per
/// `single` one (its file says more).
const TWO_LOWER_TYPES: &str = "fixtures/two_lower_types.qodec.yaml";

/// Two logical qubits of [`TWO_LOWER_TYPES`], the second flipped: their
/// blocks below take labels 0 to 3 across both types, though each type has
/// two blocks live.
const TWO_PAIRS_ONE_FLIPPED: &str = "prepare_z 0; prepare_z 1; x 1 | measure_z 0; measure_z 1";

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

/// A driver that ends shot 5 after the first `execute`, as one that has
/// read all it wants of it may, leaves shot 1's layers in place: shot 1's
/// measurement in the next `execute` of the block it prepared in the first
/// reads what it reads when no shot ends.
#[test]
fn a_shot_keeps_its_layers_when_another_shot_ends() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let program = Program::parse("prepare_x 0 | measure_x 0");
    let run = |driver: TopIsaDriver| {
        let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
            .expect("the example's bottom ISA is a protocol set");
        let driver = driver.sending_execute_to(1, &[0]).going_on_after_errors();
        (Driver::new(driver)
            >> Adapter::new(
                QodecLayerAdapter::whole(&model).expect("the example's layers assemble"),
            )
            >> Executor::new(&mut executor))
        .expect("the layers fit the driver above and the simulator below")
        .run(&ShotSet::ids([1, 5]), SEED)
        .expect("the driver goes on after errors")
    };

    let ending = run(TopIsaDriver::new(&model.top, &program).ending_shots_after(0, &[1]));
    let not_ending = run(TopIsaDriver::new(&model.top, &program));

    assert_eq!(
        not_ending[0].ends,
        [ExecuteEnd::Ran, ExecuteEnd::Ran],
        "precondition: shot 1 runs"
    );
    assert_eq!(ending[0].ends, not_ending[0].ends);
    assert_eq!(ending[0].readouts, not_ending[0].readouts);
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

/// The conditions raised in or below the layers reach the driver, on the
/// shot they were raised on only. C4/C6's decoder cannot resolve the C6
/// measurement Qodec's REPL tests end with, and says so in the result's
/// status. A part below finds the work of the even shots rejected,
/// inconsistent or unresolved on a batch that reads nothing, such as the
/// seven Hadamards that are Steane's logical Hadamard, sent whole, in
/// batches of two, or one by one: no result of the layers carries it, yet
/// the driver sees it on those shots, and not on the odd ones. Where both
/// raise one on a shot in one `execute`, C4/C6's decoder and a part below
/// finding the work of the even shots rejected on every batch, the driver
/// sees both.
#[test]
fn the_conditions_raised_in_or_below_the_layers_reach_the_driver() {
    let unresolved = "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0";
    let stepped_hadamard = "prepare_z 0 | h 0 | measure_z 0";
    let split_hadamard = "prepare_z 0; h 0; measure_z 0";
    // Each case: its qodec, its program, the conditions the part below
    // raises on the even shots of a batch reading nothing, whether it
    // raises them on batches that read too, the most instructions it takes
    // per batch, and the condition the driver sees on those shots.
    let cases = [
        (
            "c4c6/qodec.yaml",
            unresolved,
            ShotConditions::NONE,
            false,
            None,
            ShotConditions::UNRESOLVED,
        ),
        (
            "c4c6/qodec.yaml",
            unresolved,
            ShotConditions::REJECTED,
            true,
            None,
            ShotConditions::UNRESOLVED | ShotConditions::REJECTED,
        ),
        (
            "steane/steane.qodec.yaml",
            stepped_hadamard,
            ShotConditions::REJECTED,
            false,
            None,
            ShotConditions::REJECTED,
        ),
        (
            "steane/steane.qodec.yaml",
            stepped_hadamard,
            ShotConditions::INCONSISTENT,
            false,
            None,
            ShotConditions::INCONSISTENT,
        ),
        (
            "steane/steane.qodec.yaml",
            split_hadamard,
            ShotConditions::UNRESOLVED,
            false,
            Some(2),
            ShotConditions::UNRESOLVED,
        ),
        (
            "steane/steane.qodec.yaml",
            stepped_hadamard,
            ShotConditions::REJECTED,
            false,
            Some(1),
            ShotConditions::REJECTED,
        ),
    ];
    for (path, text, raised_below, on_every_batch, most, seen) in cases {
        let model = qodecs::model(path);
        let even = |shot| shot % 2 == 0;
        let mut below = if on_every_batch {
            spied_stabilizer(&model).raising_on_every_batch(raised_below, even)
        } else {
            spied_stabilizer(&model).raising_on_batches_reading_nothing(raised_below, even)
        };
        if let Some(most) = most {
            below = below.taking_at_most(most);
        }

        let runs = over(
            &model,
            TopIsaDriver::new(&model.top, &Program::parse(text)),
            &mut below,
            &ShotSet::ids(0..SHOTS),
        );

        if raised_below != ShotConditions::NONE && !on_every_batch {
            assert!(
                below.received.iter().any(|batch| batch.readout_slots == 0),
                "precondition: {text}: a batch reading nothing went below: {:?}",
                below.received
            );
        }
        for (shot, run) in runs.iter().enumerate() {
            let conditions = run.readouts.conditions;
            if raised_below == ShotConditions::NONE || shot % 2 == 0 {
                assert!(
                    conditions.contains(seen),
                    "{text}, most {most:?}, shot {shot}: {conditions:?}"
                );
            } else {
                assert!(
                    !conditions.contains(raised_below),
                    "{text}, most {most:?}: shot {shot}, on which nothing was raised: \
                     {conditions:?}"
                );
            }
        }
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

/// A run of layers a qodec does not have is refused when the adapter is
/// made, naming the run and how many layers the qodec has: an empty run,
/// before, between or after C4/C6's two layers, a reversed one, and one
/// reaching past the last layer, from within or from beyond. A run of
/// one layer, or of both, is made, the last layer included.
#[test]
fn a_run_of_layers_the_qodec_does_not_have_is_refused() {
    let model = qodecs::model("c4c6/qodec.yaml");
    assert_eq!(model.layers.len(), 2, "precondition: C4/C6 has two layers");
    let reversed = Range { start: 2, end: 1 };
    for layers in [0..0, 1..1, 2..2, reversed, 1..3, 2..3, 6..9] {
        match QodecLayerAdapter::new(&model, layers.clone()) {
            Err(QodecLayerAdapterError::NoSuchLayers {
                layers: refused,
                qodec_layers,
            }) => assert_eq!((refused, qodec_layers), (layers, 2)),
            Err(error) => panic!("{layers:?}: refused otherwise: {error}"),
            Ok(_) => panic!("{layers:?}: made"),
        }
    }
    for layers in [0..1, 1..2, 0..2] {
        assert!(
            QodecLayerAdapter::new(&model, layers.clone()).is_ok(),
            "{layers:?}"
        );
    }
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

/// Below several block types, which Qodec's encoder labels in one space
/// and the protocol counts each on its own, the layers ask the part below
/// for as many blocks of each type they lower to as the labels they may
/// hold at once: the sum of each type's most blocks. On the two-type
/// fixture, a pair (an `a` and a `b` block) and a single (a `b` block)
/// lower to one `a` and two `b` blocks, so the layers ask for three of
/// each; a single alone lowers to one `b` block, and the layers ask for no
/// `a`.
#[test]
fn below_several_block_types_each_is_asked_for_every_label_the_layers_may_hold() {
    let model = qodecs::model(TWO_LOWER_TYPES);
    let adapter = QodecLayerAdapter::whole(&model).expect("the fixture's layers assemble");
    let needs = |blocks: &[(&str, u32)]| {
        blocks.iter().fold(Needs::default(), |needs, &(name, n)| {
            needs.blocks(BlockType::new(name), n)
        })
    };
    // Each case: the blocks above, and the blocks below.
    for (above, below) in [
        (
            needs(&[("pair", 1), ("single", 1)]),
            needs(&[("a", 3), ("b", 3)]),
        ),
        (needs(&[("single", 1)]), needs(&[("b", 1)])),
    ] {
        assert_eq!(adapter.needs_below(above.clone()), below, "{above:?}");
    }
}

/// A driver that runs a shot of an earlier run again, as a debugger
/// stepping into the shot that failed does, needs the part below the
/// layers to run it again too: the layers pass the need below, with the
/// blocks they lower the driver's to, or alone when Qodec's model cannot
/// lower them (a block type Steane's top ISA does not have), where
/// [`ready`](executor_protocol::Adapt::ready) then answers why; a driver
/// that does not need it does not pass it.
#[test]
fn a_need_to_run_a_shot_again_reaches_the_part_below() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let adapter = QodecLayerAdapter::whole(&model).expect("the example's layers assemble");
    // Each case: the driver's block type, and whether the layers lower it.
    for (block_type, lowered) in [("steane", true), ("not_steanes", false)] {
        for replayable in [false, true] {
            let above = Needs::default().blocks(BlockType::new(block_type), 1);
            let above = if replayable {
                above.with_replay()
            } else {
                above
            };

            let below = adapter.needs_below(above);

            assert_eq!(
                below.blocks.is_empty(),
                !lowered,
                "precondition: {block_type}: {below:?}"
            );
            assert_eq!(below.replayable, replayable, "{block_type}: {below:?}");
        }
    }
}

/// The layers offer the driver above them the shots, seeding and replay
/// of the part below, since they run each shot on it: over a part below
/// that takes one shot a run, cannot fix its seed, and cannot run a shot
/// again, as a quantum computer may, the layers offer the same; over one
/// with no such limit, none.
#[test]
fn the_layers_offer_the_shots_seeding_and_replay_of_the_part_below() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let adapter = QodecLayerAdapter::whole(&model).expect("the example's layers assemble");
    for below in [
        Offer::unlimited(),
        Offer::unlimited()
            .max_shots(1)
            .not_seedable()
            .not_replayable(),
    ] {
        let above = adapter.offers_above(below.clone());

        assert_eq!(
            (above.max_shots, above.seedable, above.replayable),
            (below.max_shots, below.seedable, below.replayable),
            "{below:?}"
        );
    }
}

/// A rotation by an angle the program states, of no effect (0), a Pauli
/// (pi) or non-Clifford (1.234), runs on a full-state simulator below
/// repetition3's layers, which lower it to a rotation of a data qubit by
/// the same angle: the part below gets that angle as the program wrote it.
/// The shots' bits cannot show the angle, as the rotation is of a Z
/// eigenstate, so the witness is the number the part below receives.
#[test]
fn a_rotations_angle_reaches_the_part_below_as_the_program_states_it() {
    let model = qodecs::model("repetition3/repetition3.qodec.yaml");
    let plan = qodecs::plan(&model);
    for angle in [0.0, std::f64::consts::PI, 1.234] {
        let program = Program::parse(&format!(
            "prepare_z 0; rotate_z 0 theta={angle}; measure_z 0"
        ));
        let mut below = ExecutorSpy::new(
            ActionSimulatorExecutor::full_state(&model.bottom, QUBITS)
                .expect("the example's bottom ISA is a protocol set"),
        );

        let runs = over(
            &model,
            TopIsaDriver::new(&model.top, &program),
            &mut below,
            &ShotSet::ids(0..4),
        );

        for run in runs {
            let mut direct = runtime(
                &plan,
                simulator(plan.bottom(), full_state(run.stream, QUBITS as usize)),
            );
            assert_eq!(
                run.readouts,
                run_directly(plan.top(), &program, &mut direct),
                "precondition: angle {angle}: the shots read what Qodec's runtime reads"
            );
        }
        let numbers: Vec<f64> = below
            .received
            .iter()
            .flat_map(|batch| batch.numbers.iter().copied())
            .collect();
        assert!(
            !numbers.is_empty()
                && numbers
                    .iter()
                    .all(|number| number.to_bits() == angle.to_bits()),
            "angle {angle}: the numbers sent below: {numbers:?}"
        );
    }
}

/// A Pauli a gadget passes to an instruction below reaches the part below
/// as Qodec reads it, sign included, though the sign of a Pauli applied to
/// a block is a global phase that no bit shows: the two-type fixture's `x`
/// gadget passes `X`, which Qodec reads as `X_0`, and `-X_0`.
#[test]
fn a_gadgets_pauli_reaches_the_part_below_as_qodec_reads_it() {
    let model = qodecs::model(TWO_LOWER_TYPES);
    let mut below = spied_stabilizer(&model);

    over(
        &model,
        TopIsaDriver::new(&model.top, &Program::parse("prepare_z 0; x 0; measure_z 0")),
        &mut below,
        &ShotSet::ids([0]),
    );

    let paulis: Vec<&str> = below
        .received
        .iter()
        .flat_map(|batch| batch.paulis.iter().map(String::as_str))
        .collect();
    assert_eq!(paulis, ["X_0", "-X_0"]);
}

/// An error below the layers fails shot 3 and loses it there and in them:
/// the shot sent again, as a debugger stepping on would, together with
/// shot 1, in one batch or after shot 1's in a batch of its own, is
/// refused as an earlier failure before anything of that `execute` runs,
/// so shot 1 reaches no part below either. The error is a rotation by a
/// non-Clifford angle, which a stabilizer simulator cannot run, sent whole
/// or in batches of one instruction after the earlier ones of its layer's
/// batch ran below; a refusal of a part below that takes no instruction
/// at all: the layers' work on the shot is lost with it, so the refusal
/// fails the shot rather than being passed up as one; or a fault of the
/// layers, a Steane Hadamard on a block that is not live, which Qodec's
/// runtime raises while it runs the batch. The last error is a known
/// difference from Qodec's runtime driven directly, which runs its program:
/// the two-type fixture's layers release a `b` block without ending it
/// below, as its lower ISA has no release instruction, and prepare an `a`
/// block at its label; a part below cannot tell that block from one a
/// program prepares at the index of a live block of another type, so it
/// refuses it, naming both blocks and the label.
#[test]
fn a_shot_an_error_lost_is_refused_when_sent_again() {
    /// How the first `execute` must fail shot 3.
    type Fails = fn(&ExecuteEnd) -> bool;
    let rotation = (
        "repetition3/repetition3.qodec.yaml",
        "prepare_z 0; rotate_z 0 theta=0.5 | prepare_z 0; measure_z 0",
    );
    let label_released_lazily = (
        TWO_LOWER_TYPES,
        "prepare_single 0; release_single 0; prepare_z 1 | prepare_z 1; measure_z 1",
    );
    qodecs_runtime_runs(label_released_lazily);
    let together: &[&[usize]] = &[&[0, 1]];
    let shot_3_after_shot_1: &[&[usize]] = &[&[0], &[1]];
    // Each case: its qodec and program, the most instructions the part
    // below takes per batch, the batches, by position in shots [1, 3], of
    // the second `execute`, and how the first fails shot 3.
    let cases: [(_, _, _, _, Fails); 6] = [
        (
            "the simulator fails",
            rotation,
            None,
            together,
            failed_below,
        ),
        (
            "the simulator fails; shot 3 sent again in a batch after shot 1's",
            rotation,
            None,
            shot_3_after_shot_1,
            failed_below,
        ),
        (
            "the simulator fails a later batch of a split",
            rotation,
            Some(1),
            together,
            failed_below,
        ),
        (
            "the part below refuses",
            rotation,
            Some(0),
            together,
            failed_on_a_refusal_below,
        ),
        (
            "a layer faults",
            (
                "steane/steane.qodec.yaml",
                "prepare_z 0; h 1 | prepare_z 0; measure_z 0",
            ),
            None,
            together,
            failed_on_a_fault_of_layer_0,
        ),
        (
            "the part below refuses a label the layers released lazily",
            label_released_lazily,
            None,
            together,
            failed_below_on_a_label_of_two_types,
        ),
    ];
    for (case, (path, text), most, batches, fails) in cases {
        let model = qodecs::model(path);
        let program = Program::parse(text);
        let driver = TopIsaDriver::new(&model.top, &program)
            .sending_execute_to(0, &[1])
            .sending_execute_in_batches(1, batches)
            .going_on_after_errors();

        let mut below = spied_stabilizer(&model);
        if let Some(most) = most {
            below = below.taking_at_most(most);
        }
        let runs = over(&model, driver, &mut below, &ShotSet::ids([1, 3]));

        let [first, second] = runs[1].ends.as_slice() else {
            panic!("{case}: two `execute`s for shot 3: {:?}", runs[1].ends);
        };
        assert!(fails(first), "{case}: the shot fails: {first:?}");
        if most == Some(1) {
            batches_of_shot_3_ran_before_the_failing_one(&below.received);
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

/// Precondition: Qodec's runtime driven directly runs the qodec at `path`
/// on `text` (`run_directly` panics otherwise).
fn qodecs_runtime_runs((path, text): (&str, &str)) {
    let plan = qodecs::plan(&qodecs::model(path));
    run_directly(
        plan.top(),
        &Program::parse(text),
        &mut runtime(&plan, simulator(plan.bottom(), stabilizer(SEED))),
    );
}

/// Precondition: the part below `received` more than one batch of shot 3,
/// so a later one failed after earlier ones ran.
fn batches_of_shot_3_ran_before_the_failing_one(received: &[ReceivedBatch]) {
    assert!(
        received
            .iter()
            .filter(|batch| batch.shots.contains(&3))
            .count()
            > 1,
        "precondition: batches of shot 3 ran below before the failing one: {received:?}"
    );
}

/// Whether `end` is a failure raised below the layers, neither a fault of
/// theirs nor a refusal.
fn failed_below(end: &ExecuteEnd) -> bool {
    matches!(
        end,
        ExecuteEnd::Failed {
            fault: None,
            refusal: None,
            ..
        }
    )
}

/// Whether `end` is a failure raised below the layers on a new `a` block
/// at the label of a live `b` block, 0, naming both blocks and the label.
fn failed_below_on_a_label_of_two_types(end: &ExecuteEnd) -> bool {
    failed_below(end)
        && matches!(end, ExecuteEnd::Failed { message, .. } if
            ["`a` block 0", "`b` block 0", "label 0"]
                .iter()
                .all(|named| message.contains(named)))
}

/// Whether `end` is a failure on a part below's refusal to take any
/// instruction.
fn failed_on_a_refusal_below(end: &ExecuteEnd) -> bool {
    matches!(
        end,
        ExecuteEnd::Failed {
            refusal: Some(ExecuteRefusal::TooManyInstructions { max: 0, .. }),
            ..
        }
    )
}

/// Whether `end` is a failure on an invalid instruction that Qodec's runtime
/// raised in layer 0 of its qodec.
fn failed_on_a_fault_of_layer_0(end: &ExecuteEnd) -> bool {
    matches!(
        end,
        ExecuteEnd::Failed {
            fault: Some(fault),
            refusal: None,
            ..
        } if fault.kind == FaultKind::InvalidInstruction && fault.layer == Some(0)
    )
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

/// A call the layers reject before running anything fails its `execute`
/// without losing the shot: the next `execute` reads what it reads with
/// the failing one never sent. Qodec's runtime rejects a rotation sent
/// without its angle, in an unchecked batch, as a fault of its top layer
/// naming the instruction; a Pauli correction conditioned on the parity of
/// a readout, in an unchecked batch, is an argument Qodec's calls have no
/// form for, so the layers cannot translate it, and fail naming the
/// instruction. A new surface patch 0 prepared while the merged patch 0 a
/// lattice-surgery merge gave is live is two blocks of the protocol, at
/// one index of two types, but one label in Qodec, whose runtime would end
/// the merged patch to prepare the new one: the layers refuse it, naming
/// both types and the label, whether the merge came in an earlier
/// `execute` or earlier in the same one, and in a checked batch, which the
/// protocol's boundary lets through, as much as in an unchecked one.
#[test]
fn a_call_the_layers_reject_fails_its_execute_and_keeps_the_shot() {
    /// A driver made to send its program otherwise: with one of its calls
    /// malformed, or in checked batches.
    type Altered = fn(TopIsaDriver) -> TopIsaDriver;
    let clash: &[&str] = &[
        "prepare_z",
        "`surface` block 0",
        "`merged` block 0",
        "label 0",
    ];
    // Each case: its qodec, its program, that program without the failing
    // `execute`, how the driver sends it otherwise, if it does, what the
    // failure names, and the kind and layer of the Qodec fault it is, if
    // one.
    let cases: [(_, _, _, Altered, &[&str], _); 5] = [
        (
            "repetition3/repetition3.qodec.yaml",
            "prepare_z 0 | rotate_z 0 theta=0.5 | measure_z 0",
            "prepare_z 0 | measure_z 0",
            |driver| driver.without_its_arguments(1, 0),
            &["rotate_z"],
            Some((FaultKind::InvalidInstruction, Some(0))),
        ),
        (
            "c4c6/qodec.yaml",
            "prepare_z_all 0 | x0_if 0 correction=1 | measure_z_all 0",
            "prepare_z_all 0 | measure_z_all 0",
            |driver| driver.with_a_readout_parity(1, 0),
            &["x0_if"],
            None,
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | prepare_z 0 | measure_merged_z 0",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | measure_merged_z 0",
            |driver| driver,
            clash,
            None,
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | prepare_z 0 | measure_merged_z 0",
            "prepare_z 0; prepare_z 1; merge_zz 0 1 | measure_merged_z 0",
            TopIsaDriver::checked,
            clash,
            None,
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1 | merge_zz 0 1; prepare_z 0 | measure_z 0; measure_z 1",
            "prepare_z 0; prepare_z 1 | measure_z 0; measure_z 1",
            |driver| driver,
            clash,
            None,
        ),
    ];
    for (path, text, without, altered, named, qodec_fault) in cases {
        let model = qodecs::model(path);
        let driver = |text| TopIsaDriver::new(&model.top, &Program::parse(text));

        let runs = on_stabilizer(&model, altered(driver(text)).going_on_after_errors());
        let expected = on_stabilizer(&model, driver(without));

        for (run, expected) in runs.iter().zip(&expected) {
            let [
                ExecuteEnd::Ran,
                ExecuteEnd::Failed {
                    message,
                    fault,
                    refusal: None,
                },
                ExecuteEnd::Ran,
            ] = run.ends.as_slice()
            else {
                panic!(
                    "{path}: {text}: only the rejected call fails: {:?}",
                    run.ends
                );
            };
            for named in named {
                assert!(message.contains(named), "{path}: names {named}: {message}");
            }
            assert_eq!(
                fault.as_ref().map(|fault| (fault.kind, fault.layer)),
                qodec_fault,
                "{path}: {message}"
            );
            assert_eq!(run.readouts, expected.readouts, "{path}: {text}");
        }
    }
}

/// An unchecked batch the layers cannot read: a Steane measurement sent
/// with an opcode beyond the layers' top ISA, or stating no readout slot
/// for it. The core lets a receiver of such a batch panic, and gives the
/// part above the panic as the `execute`'s failure; the layers keep the
/// shot and answer its next batch. How much of the failing batch ran
/// before the panic is not promised, so the next `execute` here prepares
/// its block afresh, and reads what it reads sent alone.
#[test]
fn an_unchecked_batch_the_layers_cannot_read_panics_and_keeps_the_shot() {
    let model = qodecs::model("steane/steane.qodec.yaml");
    let driver = |text| TopIsaDriver::new(&model.top, &Program::parse(text));
    let text = "prepare_z 0; measure_z 0 | prepare_z 0; measure_z 0";
    let cases = [
        (
            "an opcode beyond the set",
            driver(text).sending_opcode(0, 1, an_opcode_beyond(&model.top)),
        ),
        ("no readout slot", driver(text).stating_readout_slots(0, 0)),
    ];
    let expected = on_stabilizer(&model, driver("prepare_z 0; measure_z 0"));
    for (case, malformed) in cases {
        let runs = on_stabilizer(&model, malformed.going_on_after_errors());

        for (run, expected) in runs.iter().zip(&expected) {
            assert!(
                matches!(
                    run.ends.as_slice(),
                    [ExecuteEnd::Panicked { .. }, ExecuteEnd::Ran]
                ),
                "{case}: only the unreadable batch fails, as a panic: {:?}",
                run.ends
            );
            assert_eq!(run.readouts, expected.readouts, "{case}");
        }
    }
}

/// The first opcode beyond `isa`'s instructions, as the protocol numbers
/// them: an opcode of a set one instruction longer.
fn an_opcode_beyond(isa: &Isa) -> Opcode {
    let count = isa.instructions.len();
    InstructionSet::new(
        "longer",
        [],
        (0..=count).map(|k| InstructionDef::new(format!("i{k}"))),
    )
    .expect("a set of distinct mnemonics")
    .opcode(&format!("i{count}"))
    .expect("the longer set has its last instruction")
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

/// A part below whose answers are pending when first asked, as a device's
/// are, woken from another thread or before it says they are pending:
/// the layers wait for each answer, and the shots read what they read over
/// a part below answering at once. The run is bounded: a wake the layers
/// lose fails the test rather than hanging it.
#[test]
fn a_part_below_answering_late_reads_what_one_answering_at_once_reads() {
    const PATH: &str = "steane/steane.qodec.yaml";
    const TEXT: &str =
        "prepare_z 0; prepare_x 1 | idle_ft 0; h 0; cnot 0 1 | measure_z 0; measure_x 1";
    let model = qodecs::model(PATH);
    let mut at_once = spied_stabilizer(&model);
    let expected = over(
        &model,
        TopIsaDriver::new(&model.top, &Program::parse(TEXT)),
        &mut at_once,
        &ShotSet::ids(0..SHOTS),
    );
    for wake in [Wake::FromAnotherThread, Wake::BeforePending] {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let model = qodecs::model(PATH);
            let mut late = LateAnswerExecutor::new(spied_stabilizer(&model), wake);
            let runs = over_late(&model, TEXT, &mut late);
            sender
                .send((runs, late.late_answers))
                .expect("the test waits for the run");
        });

        let (runs, late_answers) = receiver
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("{wake:?}: the run ends"));

        assert!(
            late_answers > 0,
            "precondition: {wake:?}: answers came late"
        );
        assert_eq!(
            runs.iter().map(|run| &run.readouts).collect::<Vec<_>>(),
            expected.iter().map(|run| &run.readouts).collect::<Vec<_>>(),
            "{wake:?}"
        );
    }
}

/// Runs `text` on every layer of `model` over `late`.
fn over_late(
    model: &QodecModel,
    text: &str,
    late: &mut LateAnswerExecutor<ExecutorSpy<ActionSimulatorExecutor>>,
) -> Vec<ShotRun> {
    (Driver::new(TopIsaDriver::new(&model.top, &Program::parse(text)))
        >> Adapter::new(QodecLayerAdapter::whole(model).expect("the example's layers assemble"))
        >> Executor::new(late))
    .expect("the layers fit the driver above and the part below")
    .run(&ShotSet::ids(0..SHOTS), SEED)
    .expect("the run gives its outputs")
}

/// A program in checked batches reads what it reads unchecked: the layers
/// state the blocks they need below, and send the part below batches its
/// boundary accepts. So for Steane's program, and for two logical qubits
/// of the two-type fixture, whose blocks below Qodec numbers in one label
/// space across their types: the labels of each type reach past the count
/// of its live blocks.
#[test]
fn checked_batches_read_what_unchecked_ones_read() {
    for (path, text) in [
        (
            "steane/steane.qodec.yaml",
            "prepare_z 0; prepare_x 1; idle_ft 0 | h 0; cnot 0 1 | measure_z 0; measure_x 1",
        ),
        (TWO_LOWER_TYPES, TWO_PAIRS_ONE_FLIPPED),
    ] {
        let model = qodecs::model(path);
        let program = Program::parse(text);
        let readouts = |driver| {
            on_stabilizer(&model, driver)
                .into_iter()
                .map(|run| run.readouts)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            readouts(TopIsaDriver::new(&model.top, &program).checked()),
            readouts(TopIsaDriver::new(&model.top, &program)),
            "{path}: {text}"
        );
    }
}
