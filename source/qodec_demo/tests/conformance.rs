//! The protocol's conformance checks (`executor_protocol::conformance`)
//! run on the demo's parts: the coins (`coin_device`) of `stim+rz`, and
//! repetition3's Qodec layer composed with them. Each check panics, naming
//! the rule broken, when the part does not keep it.

mod common;

use std::sync::Arc;

use executor_protocol::{
    Executor, InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, Opcode,
    Operand, ShotSet, conformance,
};
use qodec_demo::coin::coin_device;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;

use common::{SEED, repetition3, stim_rz};

/// `stim+rz`'s `M` on block `block`, read under `tag`.
fn measure(set: &InstructionSet, block: u32, tag: u32) -> InstructionCall {
    set.call("M", [Operand::Block(block), Operand::Tag(tag)])
        .expect("stim+rz reads a block under a tag")
}

/// Sixteen readouts of block 0, under tags `first_tag` to `first_tag + 15`:
/// sixteen coins.
fn sixteen_readouts(set: &InstructionSet, first_tag: u32) -> Vec<InstructionCall> {
    (first_tag..first_tag + 16)
        .map(|tag| measure(set, 0, tag))
        .collect()
}

/// A call one opcode beyond `set`, reading block 0 under tag 0.
fn beyond(set: &InstructionSet) -> InstructionCall {
    let opcode = u32::try_from(set.mnemonics().count()).expect("a few instructions");
    InstructionCall::new(Opcode::new(opcode), [Operand::Block(0), Operand::Tag(0)])
}

/// The coins, once closed, refuse a read.
#[test]
fn the_coins_reject_calls_once_closed() {
    let set = stim_rz().set;
    let read = [measure(&set, 0, 0)];
    let shot = ShotSet::one(0);
    conformance::rejects_calls_once_closed::<InstructionBatch<InstructionCall>, _>(
        &mut coin_device(Arc::clone(&set), SEED),
        InstructionBatchRef {
            shots: &shot,
            instructions: &read,
        },
    );
}

/// A read at an opcode beyond `stim+rz` is refused, and the coins then
/// read a block of the set.
#[test]
fn the_coins_accept_calls_after_a_refusal() {
    let set = stim_rz().set;
    let (refused, read) = ([beyond(&set)], [measure(&set, 0, 0)]);
    let shot = ShotSet::one(0);
    let batch = |instructions| InstructionBatchRef {
        shots: &shot,
        instructions,
    };
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<InstructionCall>, _>(
        &mut coin_device(Arc::clone(&set), SEED),
        batch(&refused),
        batch(&read),
    );
}

/// The coins run `R`, a reset that reads nothing, and refuse calls outside
/// `stim+rz`.
#[test]
fn the_coins_refuse_calls_outside_their_set() {
    let set = stim_rz().set;
    let reset = set
        .call("R", [Operand::Block(0)])
        .expect("a reset of stim+rz");
    conformance::refuses_calls_outside_its_set(
        &mut coin_device(Arc::clone(&set), SEED),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        &set,
        &reset,
    );
}

/// The coins refuse a batch that reads twice under one tag, at the read
/// that repeats it, and take one that reuses a tag of a batch before.
#[test]
fn the_coins_refuse_a_tag_repeated_within_a_batch() {
    let set = stim_rz().set;
    let mut coins = coin_device(Arc::clone(&set), SEED);
    let (a, b, a_again) = (
        measure(&set, 0, 0),
        measure(&set, 1, 1),
        measure(&set, 1, 0),
    );
    conformance::refuses_a_tag_repeated_within_a_batch(
        &mut coins,
        &ShotSet::one(3),
        &a,
        &b,
        &a_again,
    );
}

/// The coins refuse a repeated tag at the read that repeats it, after a
/// reset, which reads nothing, too.
#[test]
fn the_coins_refuse_a_repeated_tag_at_the_index_of_its_instruction() {
    let set = stim_rz().set;
    let mut coins = coin_device(Arc::clone(&set), SEED);
    let (a, reset, a_again) = (
        measure(&set, 0, 0),
        set.call("R", [Operand::Block(1)])
            .expect("a reset of stim+rz"),
        measure(&set, 1, 0),
    );
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut coins,
        &ShotSet::one(3),
        &a,
        &reset,
        &a_again,
    );
}

/// Coins that read once report what they ran when they close, and nothing
/// when they close again.
#[test]
fn the_coins_report_once() {
    let set = stim_rz().set;
    let mut coins = coin_device(Arc::clone(&set), SEED);
    let shot = ShotSet::one(0);
    let reply = coins
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[measure(&set, 0, 0)],
        })
        .expect("the coins take a read of their set");
    executor_protocol::wait(reply, &mut Vec::new()).expect("the coins fail no read");
    conformance::reports_once(&mut coins);
}

/// Each of shots 0 to 3 reads alone, and with the shots split into calls
/// in another order, the sixteen coins it reads among the others: the
/// coins are `replayable`.
#[test]
fn the_coins_give_each_shot_alone_what_they_give_among_others() {
    let set = stim_rz().set;
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || coin_device(Arc::clone(&set), SEED),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &[&sixteen_readouts(&set, 0)],
    );
}

/// A second call of sixteen reads gives the coins that follow those of the
/// first, as one batch of both gives them: the coins offer
/// `resumable_shots`.
#[test]
fn the_coins_continue_a_shot_from_one_call_to_the_next() {
    let set = stim_rz().set;
    conformance::continues_a_shot_from_one_call_to_the_next(
        || coin_device(Arc::clone(&set), SEED),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        &sixteen_readouts(&set, 0),
        &sixteen_readouts(&set, 16),
    );
}

/// Sixteen reads on shots 1 and 3 leave the coins of shots 0 and 2, which
/// read before, where they were.
#[test]
fn the_coins_change_no_shot_a_call_does_not_name() {
    let set = stim_rz().set;
    conformance::changes_no_shot_a_call_does_not_name(
        || coin_device(Arc::clone(&set), SEED),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &sixteen_readouts(&set, 0),
        &sixteen_readouts(&set, 16),
    );
}

/// After coins read sixteen times on each of shots 0 to 3 and were closed,
/// or dropped unclosed, new coins read each shot's coins from its start.
#[test]
fn new_coins_start_their_shots_as_made_whatever_earlier_ones_left() {
    let set = stim_rz().set;
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || coin_device(Arc::clone(&set), SEED),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &sixteen_readouts(&set, 0),
        &sixteen_readouts(&set, 16),
    );
}

/// The calls of repetition3's program `batches`, in its top set.
fn top_calls(batches: &[&str]) -> Vec<Vec<InstructionCall>> {
    Program::parse(&repetition3(), batches).batches
}

/// Two batches of repetition3's program, each of eight rounds that
/// prepare block 0 and measure it: the second reads under other tags, and
/// on coins each round reads coins that follow the round before's, so a
/// shot's results differ between the batches, between shots, and from a
/// shot that starts again.
fn two_batches_of_eight_rounds() -> Vec<Vec<InstructionCall>> {
    let rounds = ["prepare_z 0; measure_z 0"; 8].join("; ");
    top_calls(&[&rounds, &rounds])
}

/// Repetition3's Qodec layer 0, as an adapter composed with the coins of
/// `stim+rz`: an executor of the top set.
macro_rules! layer_over_coins {
    () => {{
        let qodec = repetition3();
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        (layer.adapter() >> coin_device(stim_rz().set, SEED)).expect("the boundary matches")
    }};
}

/// The layer over the coins, once closed, refuses a preparation.
#[test]
fn the_layer_over_coins_rejects_calls_once_closed() {
    let [prepare] = &top_calls(&["prepare_z 0"])[..] else {
        panic!("one batch");
    };
    let shot = ShotSet::one(0);
    conformance::rejects_calls_once_closed::<InstructionBatch<InstructionCall>, _>(
        &mut layer_over_coins!(),
        InstructionBatchRef {
            shots: &shot,
            instructions: prepare,
        },
    );
}

/// A call at an opcode beyond the top set is refused, and the layer over
/// the coins then prepares a block.
#[test]
fn the_layer_over_coins_accepts_calls_after_a_refusal() {
    let [prepare] = &top_calls(&["prepare_z 0"])[..] else {
        panic!("one batch");
    };
    let qodec = repetition3();
    let top = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let refused = [beyond(&top.accepted().set)];
    let shot = ShotSet::one(0);
    let batch = |instructions| InstructionBatchRef {
        shots: &shot,
        instructions,
    };
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<InstructionCall>, _>(
        &mut layer_over_coins!(),
        batch(&refused),
        batch(prepare),
    );
}

/// A measurement of a block never prepared fails in the layer's runtime;
/// a preparation on another shot, called before that reply is read, ends
/// at that failure, since the failure ends the whole layer.
#[test]
fn the_layer_over_coins_ends_calls_in_flight_at_an_earlier_failure() {
    let [measure] = &top_calls(&["measure_z 0"])[..] else {
        panic!("one batch");
    };
    let [prepare] = &top_calls(&["prepare_z 0"])[..] else {
        panic!("one batch");
    };
    let (shot_0, shot_1) = (ShotSet::one(0), ShotSet::one(1));
    conformance::ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<InstructionCall>, _>(
        &mut layer_over_coins!(),
        InstructionBatchRef {
            shots: &shot_0,
            instructions: measure,
        },
        InstructionBatchRef {
            shots: &shot_1,
            instructions: prepare,
        },
    );
}

/// The layer over the coins runs `prepare_z`, which reads nothing, and
/// refuses calls outside the top set.
#[test]
fn the_layer_over_coins_refuses_calls_outside_its_set() {
    let [prepare] = &top_calls(&["prepare_z 0"])[..] else {
        panic!("one batch");
    };
    let qodec = repetition3();
    let top = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    conformance::refuses_calls_outside_its_set(
        &mut layer_over_coins!(),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        &top.accepted().set,
        &prepare[0],
    );
}

/// The layer over the coins, after a call, reports what the coins ran when
/// it closes, and nothing when it closes again.
#[test]
fn the_layer_over_coins_reports_once() {
    let [program] = &top_calls(&["prepare_z 0; measure_z 0"])[..] else {
        panic!("one batch");
    };
    let mut chain = layer_over_coins!();
    let shot = ShotSet::one(0);
    let reply = chain
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: program,
        })
        .expect("the layer takes a call of its set");
    executor_protocol::wait(reply, &mut Vec::new()).expect("the layer over coins fails no call");
    conformance::reports_once(&mut chain);
}

/// Each of shots 0 to 3 reads alone, and with the shots split into calls
/// in another order, what it reads among the others in two batches of
/// eight rounds: the layer over replayable coins is `replayable`.
#[test]
fn the_layer_over_coins_gives_each_shot_alone_what_it_gives_among_others() {
    let batches = two_batches_of_eight_rounds();
    let batches: Vec<&[InstructionCall]> = batches.iter().map(Vec::as_slice).collect();
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || layer_over_coins!(),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &batches,
    );
}

/// A second call of eight rounds reads what follows the first's, as one
/// batch of both reads it: the layer over the coins continues its shots.
#[test]
fn the_layer_over_coins_continues_a_shot_from_one_call_to_the_next() {
    let [first, second] = &two_batches_of_eight_rounds()[..] else {
        panic!("two batches");
    };
    conformance::continues_a_shot_from_one_call_to_the_next(
        || layer_over_coins!(),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        first,
        second,
    );
}

/// Eight rounds on shots 1 and 3 leave shots 0 and 2, which ran rounds
/// before, where they were.
#[test]
fn the_layer_over_coins_changes_no_shot_a_call_does_not_name() {
    let [first, second] = &two_batches_of_eight_rounds()[..] else {
        panic!("two batches");
    };
    conformance::changes_no_shot_a_call_does_not_name(
        || layer_over_coins!(),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        first,
        second,
    );
}

/// After a layer over coins ran eight rounds on each of shots 0 to 3 and
/// was closed, or dropped unclosed, a new one starts each shot as made.
#[test]
fn a_new_layer_over_coins_starts_its_shots_as_made_whatever_an_earlier_one_left() {
    let [first, second] = &two_batches_of_eight_rounds()[..] else {
        panic!("two batches");
    };
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || layer_over_coins!(),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        first,
        second,
    );
}

/// On coins, which read 1 half the time, the layer's decoder marks some
/// shots' measurements with a condition; each such shot keeps it on what
/// it reads in a later call.
#[test]
fn the_layer_over_coins_keeps_each_shots_conditions_from_one_call_to_the_next() {
    let batches = top_calls(&["prepare_z 0; measure_z 0", "prepare_z 0; measure_z 0"]);
    let batches: Vec<&[InstructionCall]> = batches.iter().map(Vec::as_slice).collect();
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut layer_over_coins!(),
        &ShotSet::range(0..=7).expect("0..=7 is not empty"),
        &batches,
    );
}
