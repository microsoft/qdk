//! What a Qodec layer needs of the executor below it: its set, and the
//! order of its readouts, on repetition3 over coins, which read a coin per
//! tag whatever the instruction, so any set of the physical instructions
//! runs on them.
//!
//! The layer states the set it sends, in its own numbering; the protocol
//! matches it in the set below when the chain is built, by mnemonic, and
//! renumbers its calls on their way down. So the layer's glue needs only
//! its own sets, not the one below. Below, the layer files each readout
//! by its tag, so the order the executor gives them in does not matter.
//!
//! A Qodec set becomes the protocol's set only where the protocol's
//! operands can express it ([`Boundary::new`]).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, ExecutorLogic, ExecutorReport, InstructionCall, InstructionSet,
    MakeState, Needs, OnePerShot, OnePerShotExecutor, Operand, OperandKind, Outcome, Outcomes,
    ShotContext, executor_fn, static_driver,
};
use qodec::{Parameter, ParameterKind};
use qodec_demo::boundary::{Boundary, Ins, opcode};
use qodec_demo::coin::{coin, coin_device};
use qodec_demo::profiler::Profile;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_runtime::OwnedParam;

use common::{
    SEED, SHOTS, bottom, c4c6, c4c6_program, qodec_stim_rz, repetition3, repetition3_program,
    shot_set, sorted_with_conditions,
};

/// `stim+rz` under its own name, numbered otherwise: two instructions the
/// layer never sends first, then its own in reverse order.
fn stim_rz_numbered_otherwise() -> Arc<InstructionSet> {
    let qodec_set = qodec_stim_rz();
    let own = common::stim_rz().set;
    let mut set = InstructionSet::new(&qodec_set.name)
        .with("H", [OperandKind::Block])
        .with("S", [OperandKind::Block]);
    let mnemonics: Vec<_> = own.mnemonics().map(str::to_owned).collect();
    for mnemonic in mnemonics.iter().rev() {
        let operands = own.operands(mnemonic).expect("its own mnemonic");
        set = set.with(mnemonic, operands.iter().copied());
    }
    Arc::new(set)
}

/// What repetition3 gave on the chain over `executor`: the outcomes by
/// shot, each marked with its result's status, and the executor's report.
macro_rules! repetition3_over {
    ($executor:expr) => {{
        let qodec = repetition3();
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let chain =
            (static_driver() >> layer.adapter() >> $executor).expect("every boundary matches");
        let program = repetition3_program(&qodec).to_static(Needs::default());
        let (outcomes, report) = chain
            .run(program, shot_set(&SHOTS))
            .expect("the job fits the chain")
            .wait();
        (
            sorted_with_conditions(outcomes.expect("the run succeeds")),
            report,
        )
    }};
}

/// `profile`'s counts by mnemonic, in `set`'s numbering.
fn by_mnemonic(profile: &Profile, set: &InstructionSet) -> BTreeMap<String, u64> {
    (profile.counts.iter())
        .map(|(&opcode, &n)| {
            (
                set.mnemonic(opcode)
                    .expect("an opcode of the set")
                    .to_owned(),
                n,
            )
        })
        .collect()
}

/// Over an executor whose set holds the layer's instructions among others,
/// numbered otherwise, the layer runs as over one of its own set: the same
/// outcomes and statuses, and each instruction run as often, by mnemonic.
#[test]
fn the_layer_runs_over_a_set_that_numbers_its_instructions_otherwise() {
    let own = common::stim_rz().set;
    let otherwise = stim_rz_numbered_otherwise();
    assert_ne!(
        own.opcode("R"),
        otherwise.opcode("R"),
        "the sets number R apart"
    );

    let (outcomes, report): (_, ExecutorReport<Profile>) =
        repetition3_over!(coin_device(Arc::clone(&own), SEED));
    let (renumbered, renumbered_report): (_, ExecutorReport<Profile>) =
        repetition3_over!(coin_device(Arc::clone(&otherwise), SEED));

    assert_eq!(renumbered, outcomes);
    assert_eq!(
        by_mnemonic(&renumbered_report.states, &otherwise),
        by_mnemonic(&report.states, &own)
    );
}

/// Over an executor whose set lacks an instruction the layer sends
/// (`rotate_z`), the chain is not built: the layer's set is checked against
/// the one below before anything runs.
#[test]
fn the_layer_over_a_set_lacking_an_instruction_it_sends_is_refused_at_build() {
    let qodec = repetition3();
    let own = common::stim_rz().set;
    let mut lacking = InstructionSet::new(&qodec_stim_rz().name);
    for mnemonic in own.mnemonics().filter(|&m| m != "rotate_z") {
        lacking = lacking.with(
            mnemonic,
            own.operands(mnemonic).expect("its own").iter().copied(),
        );
    }
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let built = static_driver() >> layer.adapter() >> coin_device(Arc::new(lacking), SEED);
    let Err(faults) = built else {
        panic!("a set without rotate_z is refused");
    };
    assert!(faults.to_string().contains("rotate_z"), "{faults}");
}

/// One shot's coins, read as [`coin_device`]'s are, given in reverse.
struct Reversed {
    seed: u64,
    drawn: u64,
}

/// Coins of `set` that draw as [`coin_device`]'s do, a coin per tag in
/// call order from the shot's seed, and give each call's readouts in
/// reverse order.
fn coins_given_in_reverse(
    set: Arc<InstructionSet>,
) -> OnePerShotExecutor<
    InstructionCall,
    Reversed,
    impl MakeState<Reversed>,
    impl ExecutorLogic<InstructionCall, Reversed>,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_instruction_set(Some(set));
    let toss = |coins: &mut Reversed,
                input: &CallInput<'_, InstructionCall>,
                outcomes: &mut Outcomes<'_>| {
        let mut read = Vec::new();
        for operand in input.instructions().iter().flat_map(|c| c.operands.iter()) {
            if let Operand::Tag(tag) = *operand {
                let value = coin(coins.seed, coins.drawn).into();
                coins.drawn += 1;
                read.push(Outcome::new(tag, value));
            }
        }
        outcomes.extend(read.into_iter().rev());
        Ok::<_, String>(())
    };
    let make = |shot: ShotContext| {
        Ok(Reversed {
            seed: shot.seed(),
            drawn: 0,
        })
    };
    executor_fn(promise, make, toss)
        .seed(SEED)
        .shots(OnePerShot)
}

/// c4c6's two layers over `executor`: the outcomes by shot, each marked
/// with its result's status.
macro_rules! c4c6_over {
    ($qodec:expr, $executor:expr) => {{
        let qodec = $qodec;
        let top = QodecLayer::new(qodec, 0).expect("c4c6 plans");
        let middle = QodecLayer::new(qodec, 1).expect("c4c6 plans");
        let chain = (static_driver() >> top.adapter() >> middle.adapter() >> $executor)
            .expect("every boundary matches");
        let program = c4c6_program(qodec).to_static(Needs::default());
        let (outcomes, _) = chain
            .run(program, shot_set(&SHOTS))
            .expect("the job fits the chain")
            .wait();
        sorted_with_conditions(outcomes.expect("the run succeeds"))
    }};
}

/// Over coins that give each call's readouts in reverse order, c4c6's
/// layers give the outcomes and statuses they give over coins in call
/// order: each files a readout by its tag. (On repetition3, readouts
/// reversed and filed by position read alike too, so it would not show.)
#[test]
fn the_layers_read_the_executors_readouts_in_any_order() {
    let qodec = c4c6();
    let set = || Arc::clone(&bottom(&qodec).set);
    let in_order = c4c6_over!(&qodec, coin_device(set(), SEED));
    let reversed = c4c6_over!(&qodec, coins_given_in_reverse(set()));

    assert_eq!(reversed, in_order);
}

/// `stim+rz` with its `R` resetting any number of qubits (a variadic
/// output, its only block) and its `CX` taking any number of qubits as its
/// first input: a call's operand list is fixed, so the set is refused,
/// naming each instruction, whichever side its variadic operand is on.
#[test]
fn a_qodec_set_with_a_variadic_block_operand_is_refused() {
    let mut isa = (*qodec_stim_rz()).clone();
    for (mnemonic, output) in [("R", true), ("CX", false)] {
        let instruction = (isa.instructions.iter_mut())
            .find(|i| i.mnemonic == mnemonic)
            .expect("stim+rz has R and CX");
        let side = if output {
            &mut instruction.outputs
        } else {
            &mut instruction.inputs
        };
        side[0].is_variadic = true;
    }
    let r = (isa.instructions.iter())
        .find(|i| i.mnemonic == "R")
        .expect("stim+rz has R");
    assert!(
        r.inputs.is_empty(),
        "precondition: R's variadic operand is its output alone"
    );

    let Err(faults) = Boundary::new(&isa) else {
        panic!("a variadic block operand is refused");
    };
    assert_eq!(
        faults,
        ["R: variadic block operand", "CX: variadic block operand"]
    );
}

/// `stim+rz` with a Pauli argument on `M` and a string one on `R`: the
/// protocol has no text operand, so the set is refused, naming each
/// instruction and argument.
#[test]
fn a_qodec_set_with_a_text_argument_is_refused() {
    let mut isa = (*qodec_stim_rz()).clone();
    for (mnemonic, name, kind) in [
        ("R", "label", ParameterKind::String),
        ("M", "basis", ParameterKind::Pauli),
    ] {
        let instruction = (isa.instructions.iter_mut())
            .find(|i| i.mnemonic == mnemonic)
            .expect("stim+rz has R and M");
        instruction.parameters.push(Parameter {
            name: name.to_owned(),
            kind,
        });
    }

    let Err(faults) = Boundary::new(&isa) else {
        panic!("a text argument is refused");
    };
    assert_eq!(
        faults,
        ["R: text argument `label`", "M: text argument `basis`"]
    );
}

/// `stim+rz`'s `rotate_z` with an `integer` argument before its `number`
/// one, both given as integers, as Qodec parses `n=3 theta=-37`: the call
/// keeps the integer an `Integer`, and gives the number as the nearest
/// `Real`, the operand the set declares, so the set takes the call. The
/// `Real` is the integer itself up to 2^53; beyond, a neighbour.
#[test]
fn an_integer_given_for_a_number_is_sent_as_the_nearest_real() {
    let mut isa = (*qodec_stim_rz()).clone();
    let index = (isa.instructions.iter())
        .position(|i| i.mnemonic == "rotate_z")
        .expect("stim+rz has rotate_z");
    isa.instructions[index].parameters.insert(
        0,
        Parameter {
            name: "n".to_owned(),
            kind: ParameterKind::Integer,
        },
    );
    let boundary = Boundary::new(&isa).expect("the operands express it");
    let beyond_2_53 = (1 << 53) + 1;
    for (number, real) in [
        (-37, -37.0),
        (0, 0.0),
        (1, 1.0),
        (beyond_2_53, 9_007_199_254_740_992.0),
        (i64::MAX, 9_223_372_036_854_775_808.0),
    ] {
        let ins = Ins {
            instruction_id: index,
            parameters: vec![
                OwnedParam::Block(0),
                OwnedParam::Int(3),
                OwnedParam::Int(number),
            ]
            .into(),
        };

        let call = boundary.call(&ins, opcode(index), &mut 0);
        let expected = [Operand::Block(0), Operand::Integer(3), Operand::Real(real)];
        assert_eq!(call.operands[..], expected);
        let checked = boundary.set.check_batch(std::slice::from_ref(&call));
        assert!(checked.is_ok(), "{number}: {checked:?}");
    }
}

/// `stim+rz`'s `rotate_z` with a `number`, a `bit` and an `integer`
/// argument before its angle, as Qodec parses `phi=0.5 b=1 n=3 theta=1`:
/// each argument is sent as its own parameter declares, whatever comes
/// before it, so the integer after the number and the bit stays an
/// `Integer` and the angle is a `Real`.
#[test]
fn each_argument_is_sent_as_its_own_parameter_declares() {
    let mut isa = (*qodec_stim_rz()).clone();
    let index = (isa.instructions.iter())
        .position(|i| i.mnemonic == "rotate_z")
        .expect("stim+rz has rotate_z");
    for (name, kind) in [
        ("n", ParameterKind::Integer),
        ("b", ParameterKind::Bit),
        ("phi", ParameterKind::Number),
    ] {
        let parameter = Parameter {
            name: name.to_owned(),
            kind,
        };
        isa.instructions[index].parameters.insert(0, parameter);
    }
    let boundary = Boundary::new(&isa).expect("the operands express it");
    let ins = Ins {
        instruction_id: index,
        parameters: vec![
            OwnedParam::Block(0),
            OwnedParam::Float(0.5),
            OwnedParam::Bool(true),
            OwnedParam::Int(3),
            OwnedParam::Int(1),
        ]
        .into(),
    };

    let call = boundary.call(&ins, opcode(index), &mut 0);

    let expected = [
        Operand::Block(0),
        Operand::Real(0.5),
        Operand::Integer(1),
        Operand::Integer(3),
        Operand::Real(1.0),
    ];
    assert_eq!(call.operands[..], expected);
}

/// `stim+rz`'s `rotate_z` with a `number`, a `boolean`, an `integer` and a
/// `bit` argument before its angle, each in the form its parameter
/// declares, as in `phi=0.5 on=false n=3 b=true theta=1.0`: the call made
/// of it comes back up as the same instruction, each operand taken as the
/// parameter at its place declares, whatever parameters come before it.
#[test]
fn a_call_comes_back_as_the_instruction_it_was_made_from() {
    let mut isa = (*qodec_stim_rz()).clone();
    let index = (isa.instructions.iter())
        .position(|i| i.mnemonic == "rotate_z")
        .expect("stim+rz has rotate_z");
    for (name, kind) in [
        ("b", ParameterKind::Bit),
        ("n", ParameterKind::Integer),
        ("on", ParameterKind::Boolean),
        ("phi", ParameterKind::Number),
    ] {
        let parameter = Parameter {
            name: name.to_owned(),
            kind,
        };
        isa.instructions[index].parameters.insert(0, parameter);
    }
    let boundary = Boundary::new(&isa).expect("the operands express it");
    let ins = Ins {
        instruction_id: index,
        parameters: vec![
            OwnedParam::Block(0),
            OwnedParam::Float(0.5),
            OwnedParam::Bool(false),
            OwnedParam::Int(3),
            OwnedParam::Bool(true),
            OwnedParam::Float(1.0),
        ]
        .into(),
    };
    let call = boundary.call(&ins, opcode(index), &mut 0);

    let (back, _) = boundary.instruction(&call);
    assert_eq!(back, ins);
}
