//! The layer's refusals, its failures when its runtime errs or the executor
//! below misreads, the end of a layer whose runtime failed, and its misuse
//! with one state: its runtime keys readouts by position in a shot, so one
//! state must not serve several shots.

use std::path::Path;
use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, Driver, DriverExt, Executor, ExecutorLogic, InstructionBatch,
    InstructionBatchRef, InstructionCall, InstructionSet, MakeState, Needs, OnePerShot,
    OnePerShotExecutor, Operand, Outcome, Outcomes, ShotContext, ShotOutcome, ShotSet, conformance,
    executor_fn, static_driver, wait,
};
use qodec::Qodec;

use super::QodecLayer;
use crate::call_counter::{Calls, call_counter};
use crate::coin::coin_device;
use crate::program::Program;
use crate::stim;

/// The repetition3 bundle of the qodec checkout that the workspace builds
/// the library from, as the integration tests load it.
fn repetition3() -> Qodec {
    stim::register().expect("the parser registers");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../qdk-ec/qodec/examples/repetition3/repetition3.qodec.yaml");
    Qodec::load(path).expect("the repetition3 bundle loads from the qdk-ec checkout")
}

/// The layer above the bottom one builds: repetition3's layer 0 sends to
/// its bottom layer, 1.
#[test]
fn the_layer_above_the_bottom_builds() {
    let qodec = repetition3();
    assert_eq!(
        qodec.layers().len(),
        2,
        "precondition: repetition3 has two layers"
    );
    QodecLayer::new(&qodec, 0).expect("layer 0 has layer 1 below it");
}

/// The bottom layer is refused, since nothing is below it to send to:
/// repetition3's layer 1 is its bottom one.
#[test]
fn the_bottom_layer_is_refused_since_nothing_is_below_it() {
    let qodec = repetition3();
    assert_eq!(
        qodec.layers().len(),
        2,
        "precondition: repetition3 has two layers"
    );
    let Err(error) = QodecLayer::new(&qodec, 1) else {
        panic!("the bottom layer is refused");
    };
    assert_eq!(error, "layer 1 has nothing below it: the bottom is layer 1");
}

/// A layer beyond the bottom is refused as the bottom is, naming the
/// bottom, rather than read past the qodec's layers: repetition3 has no
/// layer 2.
#[test]
fn a_layer_beyond_the_bottom_is_refused() {
    let qodec = repetition3();
    assert_eq!(
        qodec.layers().len(),
        2,
        "precondition: repetition3 has two layers"
    );
    let Err(error) = QodecLayer::new(&qodec, 2) else {
        panic!("a layer beyond the bottom is refused");
    };
    assert_eq!(error, "layer 2 has nothing below it: the bottom is layer 1");
}

/// The last index there is, `usize::MAX`, is refused as any layer beyond
/// the bottom is: the bound does not overflow.
#[test]
fn the_last_index_is_refused_as_beyond_the_bottom() {
    let qodec = repetition3();
    let Err(error) = QodecLayer::new(&qodec, usize::MAX) else {
        panic!("a layer beyond the bottom is refused");
    };
    let expected = format!(
        "layer {} has nothing below it: the bottom is layer 1",
        usize::MAX
    );
    assert_eq!(error, expected);
}

/// repetition3 whose physical set has lost `CX`, so it is no longer the set
/// its layer 0's gadgets target: the Qodec runtime cannot plan layer 0, and
/// the layer is refused with the runtime's error, naming the layer.
#[test]
fn a_layer_the_runtime_cannot_plan_is_refused_naming_it() {
    let mut qodec = repetition3();
    let below = &mut qodec.layers_mut()[1].instruction_set;
    let mut set = (**below).clone();
    set.instructions.retain(|i| i.mnemonic != "CX");
    *below = Arc::new(set);

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer the runtime cannot plan is refused");
    };
    assert_eq!(
        error,
        "layer 0: gadget `idle`: the circuit must target the next layer's ISA"
    );
}

/// repetition3 whose `measure_z` gadget has one readout more than its
/// instruction's outcomes: the runtime plans the layer, but its frame
/// decoder cannot, and the layer is refused with the decoder's error,
/// naming the layer.
#[test]
fn a_layer_whose_decoder_cannot_be_planned_is_refused_naming_it() {
    let mut qodec = repetition3();
    let measure_z = (qodec.layers_mut()[0].gadgets.get_mut("measure_z"))
        .expect("repetition3 has a measure_z gadget");
    let readout = measure_z.readouts[0].clone();
    measure_z.readouts.push(readout);

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer whose decoder cannot be planned is refused");
    };
    assert_eq!(
        error,
        "layer 0: gadget `measure_z`: needs one readout per observe outcome and flag"
    );
}

/// `set` with two instructions the protocol's operands cannot express:
/// `label`, whose angle is text, and `wide`, rotating any number of blocks.
fn with_two_inexpressible(set: &qodec::InstructionSet) -> Arc<qodec::InstructionSet> {
    let mut set = set.clone();
    let rotate_z = (set.instructions.iter())
        .find(|i| i.mnemonic == "rotate_z")
        .expect("both sets of repetition3 have rotate_z")
        .clone();
    let mut label = rotate_z.clone();
    label.mnemonic = "label".to_owned();
    label.parameters[0].kind = qodec::ParameterKind::String;
    let mut wide = rotate_z;
    wide.mnemonic = "wide".to_owned();
    wide.inputs[0].is_variadic = true;
    set.instructions.extend([label, wide]);
    Arc::new(set)
}

/// The faults of [`with_two_inexpressible`] in layer 0's set `which`
/// (`its set` or `the set below`), as `QodecLayer::new` names and joins
/// them.
fn two_faults(which: &str) -> String {
    format!(
        "layer 0, {which}: label: text argument `theta`; \
         layer 0, {which}: wide: variadic block operand"
    )
}

/// repetition3 whose logical set also declares two instructions no gadget
/// implements, which the protocol's operands cannot express: the runtime
/// plans layer 0, and the layer is refused with both faults.
#[test]
fn a_layer_whose_set_the_protocol_cannot_express_is_refused_with_each_fault() {
    let mut qodec = repetition3();
    let own = &mut qodec.layers_mut()[0].instruction_set;
    *own = with_two_inexpressible(own);

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer whose set the protocol cannot express is refused");
    };
    assert_eq!(error, two_faults("its set"));
}

/// repetition3 whose physical set also declares two instructions no
/// circuit uses, which the protocol's operands cannot express, and whose
/// gadgets target that set: the runtime plans layer 0, and the layer is
/// refused with both faults.
#[test]
fn a_layer_over_a_set_the_protocol_cannot_express_is_refused_with_each_fault() {
    let mut qodec = repetition3();
    let below = with_two_inexpressible(&qodec.layers()[1].instruction_set);
    qodec.layers_mut()[1].instruction_set = Arc::clone(&below);
    for gadget in qodec.layers_mut()[0].gadgets.values_mut() {
        gadget.circuit.instruction_set = Arc::clone(&below);
    }

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer over a set the protocol cannot express is refused");
    };
    assert_eq!(error, two_faults("the set below"));
}

/// repetition3 whose logical and physical sets both declare the two
/// instructions, and whose gadgets target the physical set: the runtime
/// plans layer 0, and the layer is refused with the faults of both sets,
/// each naming its set.
#[test]
fn a_layer_with_faults_in_its_set_and_the_set_below_is_refused_with_each_naming_its_set() {
    let mut qodec = repetition3();
    let own = &mut qodec.layers_mut()[0].instruction_set;
    *own = with_two_inexpressible(own);
    let below = with_two_inexpressible(&qodec.layers()[1].instruction_set);
    qodec.layers_mut()[1].instruction_set = Arc::clone(&below);
    for gadget in qodec.layers_mut()[0].gadgets.values_mut() {
        gadget.circuit.instruction_set = Arc::clone(&below);
    }

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer with faults in both sets is refused");
    };
    let expected = [two_faults("its set"), two_faults("the set below")];
    assert_eq!(error, expected.join("; "));
}

/// repetition3 whose physical set has lost `CX`, so it is no longer the set
/// its layer 0's gadgets target, and whose logical and physical sets both
/// also declare the two instructions: the runtime cannot plan layer 0, and
/// the layer is refused with the runtime's error first, then its set's
/// faults, then the set below's.
#[test]
fn a_layer_the_runtime_cannot_plan_is_refused_with_its_error_then_each_sets_faults() {
    let mut qodec = repetition3();
    let mut below = (*qodec.layers()[1].instruction_set).clone();
    below.instructions.retain(|i| i.mnemonic != "CX");
    qodec.layers_mut()[1].instruction_set = with_two_inexpressible(&below);
    let own = &mut qodec.layers_mut()[0].instruction_set;
    *own = with_two_inexpressible(own);

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer the runtime cannot plan is refused");
    };
    let plan = "layer 0: gadget `idle`: the circuit must target the next layer's ISA";
    let expected = [
        plan.to_owned(),
        two_faults("its set"),
        two_faults("the set below"),
    ];
    assert_eq!(error, expected.join("; "));
}

/// repetition3 whose `measure_z` gadget has one readout more than its
/// instruction's outcomes, and whose logical and physical sets both also
/// declare the two instructions, its gadgets targeting that physical set:
/// the runtime plans layer 0 but its frame decoder cannot, and the layer is
/// refused with the decoder's error first, then its set's faults, then the
/// set below's.
#[test]
fn a_layer_whose_decoder_cannot_be_planned_is_refused_with_its_error_then_each_sets_faults() {
    let mut qodec = repetition3();
    let own = &mut qodec.layers_mut()[0].instruction_set;
    *own = with_two_inexpressible(own);
    let below = with_two_inexpressible(&qodec.layers()[1].instruction_set);
    qodec.layers_mut()[1].instruction_set = Arc::clone(&below);
    for gadget in qodec.layers_mut()[0].gadgets.values_mut() {
        gadget.circuit.instruction_set = Arc::clone(&below);
    }
    let measure_z = (qodec.layers_mut()[0].gadgets.get_mut("measure_z"))
        .expect("repetition3 has a measure_z gadget");
    let readout = measure_z.readouts[0].clone();
    measure_z.readouts.push(readout);

    let Err(error) = QodecLayer::new(&qodec, 0) else {
        panic!("a layer whose decoder cannot be planned is refused");
    };
    let decoder = "layer 0: gadget `measure_z`: needs one readout per observe outcome and flag";
    let expected = [
        decoder.to_owned(),
        two_faults("its set"),
        two_faults("the set below"),
    ];
    assert_eq!(error, expected.join("; "));
}

/// One state serving two shots runs the runtime on both shots' readouts
/// as one shot's: their equal tags collide, and the layer fails the call
/// as for a readout that came twice, instead of answering every shot with
/// one shot's bits.
#[test]
fn a_layer_of_one_state_fails_a_call_on_two_shots() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let bottom = layer.sent().set.clone();
    let chain = (static_driver() >> layer.one_state() >> coin_device(bottom, 0x5EED))
        .expect("every boundary matches");
    let program = Program::parse(&qodec, &["prepare_z 0", "idle 0", "measure_z 0"]);
    let shots = ShotSet::new(vec![2, 5]).expect("distinct shots");

    let (outcomes, _) = chain
        .run(program.to_static(Needs::default()), shots)
        .expect("the job fits the chain")
        .wait();
    let error = outcomes.expect_err("the call fails").to_string();
    assert!(error.contains("or that came twice"), "{error}");
}

/// A program's batches of its set, sent as they are, with none of the
/// checks of a static program when it is given to the chain: so that the
/// layer's own checks are what refuses them.
struct Unchecked(Program);

/// Why a batch did not run: refused at its call, or failed in its reply.
#[derive(Debug)]
enum Ended {
    Refused(String),
    Failed(String),
}

impl executor_protocol::Program for Unchecked {
    type Instruction = InstructionCall;
    type Output = Result<Vec<ShotOutcome>, Ended>;

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::clone(&self.0.top.set)))
    }
}

/// The driver of an [`Unchecked`]: sends its batches in order, each on
/// every shot, and finds no fault before the calls.
struct UncheckedDriver;

impl Driver<Unchecked> for UncheckedDriver {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: Unchecked,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Result<Vec<ShotOutcome>, Ended> {
        let mut outcomes = Vec::new();
        for instructions in &program.0.batches {
            let batch = InstructionBatchRef {
                shots,
                instructions,
            };
            let reply = (executor.call(batch)).map_err(|r| Ended::Refused(r.to_string()))?;
            wait(reply, &mut outcomes).map_err(|e| Ended::Failed(e.to_string()))?;
        }
        Ok(outcomes)
    }
}

/// How the batches of `program` ended on shot 0, through the layer over
/// `below`.
macro_rules! ended {
    ($layer:expr, $program:expr, $below:expr) => {{
        let chain = (UncheckedDriver.into_chain() >> $layer.adapter() >> $below)
            .expect("every boundary matches");
        let shots = ShotSet::new(vec![0]).expect("one shot");
        let (ended, _) = (chain.run(Unchecked($program), shots))
            .expect("the job fits the chain")
            .wait();
        ended.expect_err("a batch does not run")
    }};
}

/// repetition3 measuring two blocks in one batch, with the layer to run it.
fn two_reads() -> (QodecLayer, Program) {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let program = Program::parse(
        &qodec,
        &["prepare_z 0; prepare_z 1", "measure_z 0; measure_z 1"],
    );
    (layer, program)
}

/// A batch whose two readouts share a tag is refused before it runs:
/// their bits could not be told apart.
#[test]
fn a_call_repeating_a_tag_is_refused() {
    let (layer, mut program) = two_reads();
    let bottom = layer.sent().set.clone();
    let reads = &mut program.batches[1];
    let first = reads[0].operands.clone();
    let tag = (first.iter())
        .find_map(|o| match *o {
            Operand::Tag(t) => Some(t),
            _ => None,
        })
        .expect("measure_z reads");
    reads[1].operands = (reads[1].operands.iter())
        .map(|o| match *o {
            Operand::Tag(_) => Operand::Tag(tag),
            other => other,
        })
        .collect();

    let ended = ended!(layer, program, coin_device(bottom, 0x5EED));
    assert!(
        matches!(&ended, Ended::Refused(r) if r.contains("tag")),
        "{ended:?}"
    );
}

/// A call of an instruction of the upper set on operands it does not take
/// is refused before it runs.
#[test]
fn a_call_on_operands_its_instruction_does_not_take_is_refused() {
    let (layer, mut program) = two_reads();
    let bottom = layer.sent().set.clone();
    let call = &mut program.batches[1][1];
    call.operands = (call.operands.iter())
        .filter(|o| matches!(o, Operand::Block(_)))
        .copied()
        .collect();

    let ended = ended!(layer, program, coin_device(bottom, 0x5EED));
    assert!(
        matches!(&ended, Ended::Refused(r) if r.contains("operand")),
        "{ended:?}"
    );
}

/// Coins of `set` that read zero under each tag `answer` makes of a call's
/// readout tags, in call order.
fn coins_answering(
    set: Arc<InstructionSet>,
    answer: fn(&mut Vec<u32>),
) -> OnePerShotExecutor<
    InstructionCall,
    (),
    impl MakeState<()>,
    impl ExecutorLogic<InstructionCall, ()>,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_instruction_set(Some(set));
    let toss =
        move |(): &mut (), input: &CallInput<'_, InstructionCall>, outcomes: &mut Outcomes<'_>| {
            let mut tags: Vec<u32> = (input.instructions().iter())
                .flat_map(|c| c.operands.iter())
                .filter_map(|o| match *o {
                    Operand::Tag(t) => Some(t),
                    _ => None,
                })
                .collect();
            answer(&mut tags);
            outcomes.extend(tags.into_iter().map(|tag| Outcome::new(tag, false.into())));
            Ok::<_, String>(())
        };
    executor_fn(promise, |_: ShotContext| Ok(()), toss)
        .seed(0x5EED)
        .shots(OnePerShot)
}

/// Coins of `set` that give the readouts of each call but its last.
fn coins_dropping_the_last_readout(
    set: Arc<InstructionSet>,
) -> OnePerShotExecutor<
    InstructionCall,
    (),
    impl MakeState<()>,
    impl ExecutorLogic<InstructionCall, ()>,
> {
    coins_answering(set, |tags| {
        tags.pop();
    })
}

/// A readout the executor below does not give fails the call instead of
/// reading as a bit it never had.
#[test]
fn a_readout_missing_below_fails_the_call() {
    let (layer, program) = two_reads();
    let bottom = layer.sent().set.clone();

    let ended = ended!(layer, program, coins_dropping_the_last_readout(bottom));
    assert!(
        matches!(&ended, Ended::Failed(e) if e.contains("no outcome under tag")),
        "{ended:?}"
    );
}

/// An outcome the executor below gives under a tag its call did not use,
/// beside each readout asked for, fails the call instead of being
/// dropped.
#[test]
fn an_outcome_not_asked_for_below_fails_the_call() {
    let (layer, program) = two_reads();
    let bottom = layer.sent().set.clone();
    let coins = coins_answering(bottom, |tags| {
        if let Some(&last) = tags.iter().max() {
            tags.push(last + 1);
        }
    });

    let ended = ended!(layer, program, coins);
    assert!(
        matches!(&ended, Ended::Failed(e) if e.contains("an outcome not asked for")),
        "{ended:?}"
    );
}

/// A program measuring a block it never prepared: the layer's runtime
/// refuses the instruction while placing it, before any side effect, so it
/// sends nothing below and is not poisoned (`QodecError::InvalidInstruction`).
/// An adapter's logic can fail a call but not refuse it, so the call fails
/// with the runtime's error, and every later call on the layer is refused
/// naming it, on any shot, as after a failure part-way.
#[test]
fn a_runtime_error_that_ran_nothing_fails_the_call_and_every_later_call() {
    let qodec = repetition3();
    let program = Program::parse(&qodec, &["measure_z 0"]);
    let layer = || QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let bottom = layer().sent().set.clone();
    let counted = UncheckedDriver.into_chain() >> layer().adapter() >> call_counter();
    let chain = (counted >> coin_device(bottom.clone(), 0x5EED)).expect("every boundary matches");
    let shot = ShotSet::new(vec![0]).expect("one shot");
    let (ended, (Calls(calls), _)) = (chain.run(Unchecked(program.clone()), shot))
        .expect("the job fits the chain")
        .wait();
    assert_eq!(calls, 0, "precondition: the runtime sends nothing below");
    assert!(
        matches!(&ended, Err(Ended::Failed(e)) if e.contains("invalid instruction")),
        "the call fails with the runtime's error: {ended:?}"
    );

    let mut executor =
        (layer().adapter() >> coin_device(bottom, 0x5EED)).expect("the boundary matches");
    let instructions = &program.batches[0];
    let shots = [vec![0], vec![1], vec![0, 1]].map(|s| ShotSet::new(s).expect("distinct shots"));
    let [failed, other, both] = shots.each_ref().map(|shots| InstructionBatchRef {
        shots,
        instructions,
    });

    conformance::refuses_calls_after_a_failure::<InstructionBatch<InstructionCall>, _>(
        &mut executor,
        failed,
        &[failed, other, both],
    );
}

/// A readout lost below while the layer's runtime flushes, part-way
/// through the batch: the call fails, and every later call on the layer is
/// refused naming that failure, on the failed shot and on any other.
#[test]
fn a_layer_whose_runtime_failed_part_way_refuses_every_later_call() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let bottom = layer.sent().set.clone();
    let mut executor =
        (layer.adapter() >> coins_dropping_the_last_readout(bottom)).expect("the boundary matches");
    let program = Program::parse(&qodec, &["prepare_z 0; measure_z 0"]);
    let instructions = &program.batches[0];
    let shots = [vec![0], vec![1], vec![0, 1]].map(|s| ShotSet::new(s).expect("distinct shots"));
    let [failed, other, both] = shots.each_ref().map(|shots| InstructionBatchRef {
        shots,
        instructions,
    });

    conformance::refuses_calls_after_a_failure::<InstructionBatch<InstructionCall>, _>(
        &mut executor,
        failed,
        &[failed, other, both],
    );
}
