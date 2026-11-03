//! How a Qodec layer groups a call's shots into calls below, by the
//! adapter facility's rule for a state per shot (`Below::send`,
//! `Below::run`). A batch that reads goes below on its shot alone, since
//! the shot's runtime waits for each read (QD1, mode (c)). An upper call
//! whose lowering reads nothing goes below on its shots at once: each
//! shot's runtime sends its batches without waiting, and the shots' equal
//! sends go below in one call. Every bottom batch of c4c6 reads, so below
//! c4c6 every one goes alone. No program of the demo sends a batch before
//! or after a read in one upper call: the protocol's tests pin those
//! branches (`a_shots_run_leaves_the_other_shots_sends_merged`,
//! `a_shots_sends_after_its_run_merge_with_the_others`). Merging a shot's
//! batches that read with the other shots' needs a lockstep per-shot
//! facility whose shots' logic can stop at a read: async, resumable, or a
//! thread per shot (open, Q67).
//!
//! The calls are counted by a counter with one state ([`call_counter`]),
//! which sees each call whole, and recorded by [`recorder`], which keeps
//! each call, also one the executor below refuses once the shot has a
//! state. Both send each call below as it is.

mod common;

use std::collections::BTreeMap;

use executor_protocol::{InstructionCall, Needs, Operand, static_driver};
use qodec_demo::call_counter::{Calls, call_counter};
use qodec_demo::coin::coin_device;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::on_the_stack;

use common::two_reads_over_coins_refusing;
use common::{
    CODE_SHOTS, SEED, bottom, c4c6, c4c6_program, c422, c422_program, repetition3,
    repetition3_program, shot_set,
};

/// Whether `batch` reads: whether a call of it has a tag.
fn reads(batch: &[InstructionCall]) -> bool {
    (batch.iter()).any(|call| call.operands.iter().any(|o| matches!(o, Operand::Tag(_))))
}

/// Below c4c6's two layers, on 4 shots, every bottom batch reads, so each
/// goes below on its shot alone: the device gets as many calls as the
/// Qodec runtime's own stack sends batches over the shots, 69 a shot.
#[test]
fn below_c4c6_every_bottom_batch_goes_below_on_its_shot_alone() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let batches: Vec<_> = (stack.backends.iter())
        .flat_map(|(_, coins)| coins.sent())
        .collect();
    assert_eq!(batches.len(), 4 * 69, "the stack sends 69 batches a shot");
    assert!(
        batches.iter().all(|batch| reads(batch)),
        "every bottom batch of c4c6 reads"
    );
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> call_counter() >> coins)
        .expect("every boundary matches");
    let (outcomes, (calls, _)): (_, (Calls, _)) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    assert_eq!(calls, Calls(276));
}

/// Below c422's one layer, on 4 shots, each of the program's 3 upper calls
/// lowers to one bottom batch a shot: the first, whose lowering reads
/// nothing, goes below in one call on all 4 shots; each of the two later,
/// which read, goes below on its shot alone: 9 calls for the 12 batches
/// the Qodec runtime's own stack sends.
#[test]
fn below_a_layer_only_an_upper_call_whose_lowering_reads_nothing_goes_below_on_its_shots_at_once() {
    let qodec = c422();
    let program = c422_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let sent: Vec<_> = (stack.backends.iter())
        .map(|(_, coins)| coins.sent().len())
        .collect();
    assert_eq!(
        sent, [3; 4],
        "precondition: the stack sends a batch a shot for each upper call"
    );
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> layer.adapter() >> call_counter() >> coins)
        .expect("every boundary matches");
    let (outcomes, (calls, _)): (_, (Calls, _)) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    assert_eq!(calls, Calls(1 + 2 * 4));
}

/// Below repetition3's layer, on 4 shots, no upper call sends a batch
/// before or after a read: each of the program's 3 upper calls lowers to
/// one bottom batch a shot, the first reading nothing and going below in
/// one call on all 4 shots, each later one a read on its shot alone. A
/// send beside a read would go below in a call of its own: 9 calls for the
/// 12 batches the Qodec runtime's own stack sends.
#[test]
fn below_repetition3_no_upper_call_sends_a_batch_beside_a_read() {
    let qodec = repetition3();
    let program = repetition3_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let sent: Vec<_> = (stack.backends.iter())
        .map(|(_, coins)| coins.sent().iter().map(|b| reads(b)).collect::<Vec<_>>())
        .collect();
    assert_eq!(
        sent,
        [[false, true, true]; 4],
        "precondition: the stack sends a batch a shot for each upper call, \
         the first reading nothing"
    );
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> layer.adapter() >> call_counter() >> coins)
        .expect("every boundary matches");
    let (outcomes, (calls, _)): (_, (Calls, _)) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    assert_eq!(calls, Calls(1 + 2 * 4));
}

/// The counter sends each call below as it is: below c422's layer, a
/// recorder under the counter receives the batches a recorder over it
/// sent. The two recorders compare them shot by shot, not how the shots
/// are grouped into calls, which the counter cannot change: its one
/// state's `send` takes no shots, and goes below on the call's.
#[test]
fn the_call_counter_sends_each_call_below_as_it_is() {
    let qodec = c422();
    let program = c422_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let chain =
        (static_driver() >> layer.adapter() >> recorder() >> call_counter() >> recorder() >> coins)
            .expect("every boundary matches");
    let (outcomes, (over, (_, (under, _)))) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    let (over, under): (Sent, Sent) = (over, under);
    outcomes.expect("the run succeeds");
    assert!(
        over.0.len() == shots.len() && over.0.values().all(|batches| !batches.is_empty()),
        "precondition: each shot sends batches below"
    );

    assert_eq!(under, over);
}

/// The recorder sends each call below as it is: below c422's layer, a
/// second recorder under it keeps the batches the first kept. The records
/// are compared shot by shot, not by how the shots are grouped into
/// calls, which is the facility's: each shot's state sends the call's
/// batch, and the shots' equal sends go below in one call.
#[test]
fn the_recorder_sends_each_call_below_as_it_is() {
    let qodec = c422();
    let program = c422_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let chain = (static_driver() >> layer.adapter() >> recorder() >> recorder() >> coins)
        .expect("every boundary matches");
    let (outcomes, (over, (under, _))) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    let (over, under): (Sent, Sent) = (over, under);
    outcomes.expect("the run succeeds");
    assert!(
        over.0.len() == shots.len() && over.0.values().all(|batches| !batches.is_empty()),
        "precondition: each shot sends batches below"
    );

    assert_eq!(under, over);
}

/// Once a shot has a state, the recorder keeps a call the executor below
/// refuses: over coins that refuse the shot's second call, its record
/// holds both reads, though the coins ran one.
#[test]
fn once_a_shot_has_a_state_the_recorder_keeps_a_call_refused_below() {
    let (reads, kept, ran): (_, Sent, _) = two_reads_over_coins_refusing!(recorder(), 1);
    assert_eq!(ran.batches, 1, "precondition: the coins ran one");

    let expected = Sent(BTreeMap::from([(2, reads.map(|r| vec![r]).to_vec())]));
    assert_eq!(kept, expected);
}

/// A shot's first call, refused below, is not kept: the state made for it
/// goes with the refusal, as the adapter facility keeps no state made for
/// a refused call. Over coins that refuse the shot's first call, its
/// record holds the second read alone.
#[test]
fn the_recorder_keeps_no_shots_first_call_refused_below() {
    let (reads, kept, ran): (_, Sent, _) = two_reads_over_coins_refusing!(recorder(), 0);
    assert_eq!(ran.batches, 1, "precondition: the coins ran one");

    let [_, second] = reads;
    assert_eq!(kept, Sent(BTreeMap::from([(2, vec![vec![second]])])));
}
