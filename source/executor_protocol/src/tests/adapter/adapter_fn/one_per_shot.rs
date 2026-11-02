// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::task::Waker;

use super::{
    DropLog, Dropped, DroppedBelow, HeldOpen, Offering, Pulse, Senders, Unmade, after_a_stop,
    ample_simulator, close_counted, count_and_pass, decodable, decode, device, device_promising,
    each_capability_alone, finds_no_syndrome, flip_and_toss_flip_first, flip_first_set,
    holding_until_busy, lower, no_calls, one, outcomes, panics_decoding, pass_unless_empty,
    pulse_device, recorded, refusal_after_a_panic, scant_simulator, send_down,
    unmet_needs_of_a_job_beyond_the_scant_simulator,
};
use crate::tests::executor_fn::{
    Freed, Op, PowerDownFailed, PowersDownBadly, ReleasesBadly, freed_so_far, panic_text,
    powering_down_badly, promise,
};
use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel, run, step};
use crate::tests::{
    Bits, PanicsOnClose, bits_set, ends_soon, flip_and_toss, reversed_set, shots, values,
};
use crate::{
    AdapterError, AdapterLogic, Below, BuildError, CallInput, Capabilities, Close, ComponentId,
    Describe, Executor, ExecutorFn, ExecutorLogic, IncrementalReply, InstructionBatch,
    InstructionBatchRef, InstructionCall, MakeState, Need, OnePerShot, OnePerShotAdapter,
    OnePerShotExecutor, Outcome, Outcomes, OwnThread, Rejected, Reply, ReplyStatus, ShotContext,
    ShotId, ShotOutcome, ShotSet, Value, adapter_fn, compose, conformance, executor_fn,
    reply_channel, wait,
};

/// The shots of the target chain's run: not a range.
const SHOTS: [ShotId; 4] = [2, 5, 6, 11];

/// The tests' executor of `model`, seeded with 7.
fn below(model: TestDoubleModel) -> TestDoubleExecutor<TestDoubleModel> {
    TestDoubleExecutor::with_model(model, 7)
}

/// A lowering from [`flip_first_set`] to [`reversed_set`] ([`lower`]), a
/// state per shot.
fn lowering() -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    (),
    impl MakeState<()>,
    impl AdapterLogic<InstructionCall, InstructionCall, ()>,
> {
    adapter_fn(no_state, lower)
        .accepts(flip_first_set())
        .sends(reversed_set())
        .shots(OnePerShot)
}

/// The state of an adapter that keeps none.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn no_state(_: ShotContext) -> Result<(), Rejected> {
    Ok(())
}

/// The logic of an observer that sends each call down as it is, whatever
/// its state.
fn pass<S>(
    _: &mut S,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The logic of a lowering whose shots send what their parity says: the
/// even shots read once, the odd ones skip a bit, then read.
fn by_parity(
    shot: &mut ShotId,
    _: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    if shot.is_multiple_of(2) {
        below.send(&[true])
    } else {
        below.send(&[false, true])
    }
}

/// The state of a shot that knows its shot.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn its_shot(context: ShotContext) -> Result<ShotId, Rejected> {
    Ok(context.shot())
}

/// A debugger that logs each shot's calls in its own count: called on
/// three shots, then on two of them, each shot's count goes on from where
/// its last call left it, and three states are made, one per shot.
#[test]
fn each_shot_has_a_state_of_its_own_kept_for_its_later_calls() {
    let made = Rc::new(Cell::new(0));
    let log = Rc::new(RefCell::new(Vec::new()));
    let debugger = {
        let (made, log) = (Rc::clone(&made), Rc::clone(&log));
        adapter_fn(
            move |context: ShotContext| {
                made.set(made.get() + 1);
                Ok((context.shot(), 0))
            },
            move |(shot, calls): &mut (ShotId, u32),
                  input: &CallInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                *calls += 1;
                log.borrow_mut().push((*shot, *calls));
                below.send(input.instructions())
            },
        )
        .shots(OnePerShot)
    };
    let mut executor = (debugger >> below(TestDoubleModel::default())).expect("bools fit");
    step(
        &mut executor,
        &shots(&[0, 1, 2]),
        &[true],
        "three shots run",
    );
    step(&mut executor, &shots(&[1, 2]), &[true], "two run again");
    assert_eq!(*log.borrow(), [(0, 1), (1, 1), (2, 1), (1, 2), (2, 2)]);
    assert_eq!(made.get(), 3, "one state per shot");
}

/// A profiler's counts, which every state adds to, shared with the test.
type Counts = Rc<RefCell<BTreeMap<bool, u64>>>;

/// The logic of a profiler: counts the instructions it is called with by
/// kind, then sends the call down.
fn profile(
    counts: &mut Counts,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    for &read in input.instructions() {
        *counts.borrow_mut().entry(read).or_default() += 1;
    }
    below.send(input.instructions())
}

/// The values `program` reads on `shots` of `executor`, batch after
/// batch, each accepted and ending well.
fn read<E: Executor<InstructionBatch<bool>>>(
    executor: &mut E,
    shots: &ShotSet,
    program: &[&[bool]],
) -> Vec<(u32, u32, Value)> {
    let mut read = Vec::new();
    for batch in program {
        let (out, end) = run(executor, shots, batch).expect("the batch is accepted");
        end.expect("the batch ends well");
        read.extend(values(&out));
    }
    read
}

/// The target chain's profiler, run on its four shots with a state per
/// shot and with one state: per shot it counts the program once per shot,
/// with one state once per call, and the outcomes are those of no adapter
/// either way.
#[test]
fn the_profilers_shot_form_changes_its_counts_not_the_outcomes() {
    let program: [&[bool]; 2] = [&[true, false], &[true, true]];
    let four = shots(&SHOTS);
    let profiler = |counts: &Counts| {
        let counts = Rc::clone(counts);
        adapter_fn(move |_| Ok(Rc::clone(&counts)), profile)
    };
    let per_shot_counts = Counts::default();
    let mut per_shot = (profiler(&per_shot_counts).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    let one_state_counts = Counts::default();
    let mut one_state =
        (profiler(&one_state_counts) >> below(TestDoubleModel::default())).expect("bools fit");
    let without = read(&mut below(TestDoubleModel::default()), &four, &program);
    assert!(!without.is_empty(), "the program reads bits");
    assert_eq!(read(&mut per_shot, &four, &program), without);
    assert_eq!(read(&mut one_state, &four, &program), without);
    let per_shot_counts = per_shot_counts.borrow();
    assert_eq!(*per_shot_counts, BTreeMap::from([(false, 4), (true, 12)]));
    let one_state_counts = one_state_counts.borrow();
    assert_eq!(*one_state_counts, BTreeMap::from([(false, 1), (true, 3)]));
}

/// The target chain's profiler on its four shots: every shot sends the
/// call it is called with, so the executor below sees one call over the
/// four shots, as with no adapter.
#[test]
fn equal_sends_go_below_as_one_call_over_their_shots() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut profiled =
        (adapter_fn(no_state, pass).shots(OnePerShot) >> executor).expect("bools fit");
    step(
        &mut profiled,
        &shots(&SHOTS),
        &[true, false],
        "the call runs",
    );
    assert_eq!(*calls.borrow(), [(SHOTS.to_vec(), vec![true, false])]);
}

/// The profiler called twice, on shots 2 and 5, then on shot 6 with
/// another program: each call goes below with its own shots' sends only.
#[test]
fn a_call_sends_below_what_its_own_shots_sent_only() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut profiled =
        (adapter_fn(no_state, pass).shots(OnePerShot) >> executor).expect("bools fit");
    step(
        &mut profiled,
        &shots(&[2, 5]),
        &[true],
        "the first call runs",
    );
    step(
        &mut profiled,
        &ShotSet::one(6),
        &[false],
        "the second call runs",
    );
    assert_eq!(
        *calls.borrow(),
        [(vec![2, 5], vec![true]), (vec![6], vec![false])]
    );
}

/// A lowering whose even shots send one batch and odd ones another, on
/// shots whose parities interleave: each batch goes below once, over the
/// shots that sent it, the one of the first shot first.
#[test]
fn unequal_sends_go_below_apart_each_over_the_shots_that_sent_it() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut lowered =
        (adapter_fn(its_shot, by_parity).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut lowered, &shots(&SHOTS), &[true], "the call runs");
    assert_eq!(
        *calls.borrow(),
        [(vec![2, 6], vec![true]), (vec![5, 11], vec![false, true])]
    );
}

/// A lowering whose shots send parts of the call's instructions, `[read,
/// read, skip]`, or a batch of their own: shot 2 the first read, shot 5
/// the second, shot 11 a read of its own, and shot 6 both reads, from where
/// shot 2's starts. Sends merge by what they hold, wherever they lie: the
/// three single reads go as one call, and shot 6's apart.
#[test]
fn sends_merge_by_what_they_hold_wherever_it_lies() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let parts = |shot: &mut ShotId,
                 input: &CallInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        let instructions = input.instructions();
        match *shot {
            2 => below.send(&instructions[..1]),
            5 => below.send(&instructions[1..2]),
            6 => below.send(&instructions[..2]),
            _ => below.send(&[true]),
        }
    };
    let mut lowered =
        (adapter_fn(its_shot, parts).shots(OnePerShot) >> executor).expect("bools fit");
    step(
        &mut lowered,
        &shots(&SHOTS),
        &[true, true, false],
        "the call runs",
    );
    assert_eq!(
        *calls.borrow(),
        [(vec![2, 5, 11], vec![true]), (vec![6], vec![true, true])]
    );
}

/// A lowering whose even shots send a read, then a skip and a read, and
/// whose odd shots send the same two batches the other way round: each
/// shot's sends reach below in the order it made them, so the first sends
/// of every shot go before the second sends of any.
#[test]
fn a_shots_sends_reach_below_in_the_order_it_made_them() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let both_ways = |shot: &mut ShotId,
                     _: &CallInput<'_, bool>,
                     below: &mut Below<'_, bool>,
                     _: &mut Outcomes<'_>| {
        let (read, skip_read): (&[bool], &[bool]) = (&[true], &[false, true]);
        if shot.is_multiple_of(2) {
            below.send(read)?;
            below.send(skip_read)
        } else {
            below.send(skip_read)?;
            below.send(read)
        }
    };
    let mut lowered =
        (adapter_fn(its_shot, both_ways).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut lowered, &shots(&SHOTS), &[], "the call runs");
    assert_eq!(
        *calls.borrow(),
        [
            (vec![2, 6], vec![true]),
            (vec![5, 11], vec![false, true]),
            (vec![2, 6], vec![false, true]),
            (vec![5, 11], vec![true]),
        ]
    );
}

/// A lowering whose shots write their batch into one buffer they share,
/// to spare an allocation per shot: the even shots a read, the odd ones a
/// skip, so the buffer is at one place with one length at every send. Each
/// send is kept as it is when sent: the reads go as one call, the skips as
/// another. The buffer and the call's input are the two halves of one
/// array, the buffer first and then last, so that the buffer lies just
/// outside the input, before it and after it, wherever the array is.
#[test]
fn a_buffer_sent_again_is_sent_as_it_is_at_each_send() {
    for buffer_first in [true, false] {
        let (executor, calls) = recorded(below(TestDoubleModel::default()));
        let mut memory = [true, true];
        let (buffer, input) = if buffer_first {
            let (buffer, input) = memory.split_at_mut(1);
            (buffer, &*input)
        } else {
            let (input, buffer) = memory.split_at_mut(1);
            (buffer, &*input)
        };
        let buffer = RefCell::new(buffer);
        let sharing = |shot: &mut ShotId,
                       _: &CallInput<'_, bool>,
                       below: &mut Below<'_, bool>,
                       _: &mut Outcomes<'_>| {
            buffer.borrow_mut()[0] = shot.is_multiple_of(2);
            below.send(&buffer.borrow())
        };
        let mut lowered =
            (adapter_fn(its_shot, sharing).shots(OnePerShot) >> executor).expect("bools fit");
        step(&mut lowered, &shots(&SHOTS), input, "the call runs");
        assert_eq!(
            *calls.borrow(),
            [(vec![2, 6], vec![true]), (vec![5, 11], vec![false])],
            "the buffer first: {buffer_first}"
        );
    }
}

/// The logic of an adapter that sends a batch whose length is its shot's
/// calls so far, then writes how many it sent, as tag 9: what it sends
/// and writes depends on its shot's state.
fn growing(
    calls: &mut u32,
    _: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    *calls += 1;
    let batch = vec![true; *calls as usize];
    below.send(&batch)?;
    outcomes.push(Outcome::new(9, (*calls % 2 == 1).into()));
    Ok(())
}

/// Shot 6 of the target chain's run, run twice alone and twice among 2, 5
/// and 11, the shots that join it differing at each call, through an
/// adapter whose sends and outcomes depend on each shot's state: it gives
/// the same outcomes either way, since its state and its sends are its own.
#[test]
fn a_shot_gives_alone_what_it_gives_among_the_others() {
    let adapter = || adapter_fn(|_| Ok(0), growing).shots(OnePerShot);
    let mut alone = (adapter() >> below(TestDoubleModel::default())).expect("bools fit");
    let mut among = (adapter() >> below(TestDoubleModel::default())).expect("bools fit");
    let six = ShotSet::one(6);
    let mut its_own = Vec::new();
    for others in [&[2, 6, 11][..], &[5, 6]] {
        let (out, end) = run(&mut among, &shots(others), &[]).expect("accepted");
        end.expect("the call ends well");
        its_own.extend(values(&out).into_iter().filter(|&(j, ..)| j == 6));
    }
    let by_itself = read(&mut alone, &six, &[&[], &[]]);
    assert_eq!(by_itself.len(), 5, "two writes and three reads");
    assert_eq!(sorted(its_own), sorted(by_itself));
}

/// `outcomes`, sorted: a shot's outcomes come in another order among
/// others, the writes of every shot before the reads of any.
fn sorted(mut outcomes: Vec<(u32, u32, Value)>) -> Vec<(u32, u32, Value)> {
    outcomes.sort_by_key(|&(j, tag, _)| (j, tag));
    outcomes
}

/// A decoder that writes its shot's logical outcome, tag 9, its value the
/// shot's parity, on shots 2 and 5: each outcome answers its own shot only.
#[test]
fn an_outcome_written_answers_its_own_shot_only() {
    let decoding = |shot: &mut ShotId,
                    _: &CallInput<'_, bool>,
                    _: &mut Below<'_, bool>,
                    outcomes: &mut Outcomes<'_>| {
        outcomes.push(Outcome::new(9, (*shot % 2 == 1).into()));
        Ok(())
    };
    let mut executor = (adapter_fn(its_shot, decoding).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    let (written, end) = run(&mut executor, &shots(&[2, 5]), &[]).expect("accepted");
    end.expect("the logic does not fail");
    assert_eq!(values(&written), [(2, 9, Value::Zero), (5, 9, Value::One)]);
}

/// The run's seed.
const SEED: u64 = 0x5EED;

/// The id a noise adapter draws with.
const NOISE: ComponentId = ComponentId::named("noise");

/// The id a second noise adapter draws with, a readout error's.
const READOUT: ComponentId = ComponentId::named("readout");

/// A simulator of bools made with `executor_fn`, one state, that reads
/// nothing, for the tests of what reaches an adapter above it: unseeded,
/// and with no shot strategy, for each test to give its own.
fn unseeded_simulator() -> ExecutorFn<bool, (), impl MakeState<()>, impl ExecutorLogic<bool, ()>> {
    executor_fn(
        Capabilities::default().with_qubits(4),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
}

/// The [`unseeded_simulator`], given the run's seed, a state per shot.
fn simulator() -> OnePerShotExecutor<bool, (), impl MakeState<()>, impl ExecutorLogic<bool, ()>> {
    unseeded_simulator().seed(SEED).shots(OnePerShot)
}

/// A noise adapter that draws with [`NOISE`], over a simulator given the
/// run's seed, run on the target chain's shots: shot `j`'s state seeds
/// from shot `j`'s context in the run, bound to the noise's id, so it is
/// its own, and is the same whatever the other shots.
#[test]
fn a_drawing_adapters_states_seed_from_their_shot_bound_to_its_id() {
    let seeds = Rc::new(RefCell::new(Vec::new()));
    let noise = {
        let seeds = Rc::clone(&seeds);
        adapter_fn(
            move |context: ShotContext| {
                seeds.borrow_mut().push((context.shot(), context.seed()));
                Ok(())
            },
            pass,
        )
        .drawing(NOISE)
        .shots(OnePerShot)
    };
    let mut executor = (noise >> simulator()).expect("bools fit");
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    let expected: Vec<_> = (SHOTS.iter())
        .map(|&j| (j, ShotContext::new(SEED, j).rng_seed(NOISE)))
        .collect();
    assert_eq!(*seeds.borrow(), expected);
    assert_ne!(
        expected[0].1,
        ShotContext::new(SEED, 2).seed(),
        "not the simulator's"
    );
}

/// A simulator of bools made with `executor_fn`, given the run's seed,
/// whose read `i` reads bit `i` of its shot's seed, under tag `i`, so it
/// offers replayable shots; with no shot strategy yet, so that a test can
/// give it one and a placement, in either order.
fn seeded_reading_simulator() -> ExecutorFn<
    bool,
    u64,
    impl MakeState<u64> + Clone + Send + 'static,
    impl ExecutorLogic<bool, u64> + Clone + Send + 'static,
> {
    executor_fn(
        Capabilities::default().with_qubits(4).with_replayable(true),
        |context: ShotContext| Ok(context.seed()),
        |seed: &mut u64, input: &CallInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for (i, &read) in input.instructions().iter().enumerate() {
                if read {
                    outcomes.push(Outcome::new(
                        u32::try_from(i).expect("a short batch"),
                        (*seed >> i & 1 == 1).into(),
                    ));
                }
            }
            Ok(())
        },
    )
    .seed(SEED)
}

/// The [`seeded_reading_simulator`], a state per shot.
fn reading_simulator()
-> OnePerShotExecutor<bool, u64, impl MakeState<u64>, impl ExecutorLogic<bool, u64>> {
    seeded_reading_simulator().shots(OnePerShot)
}

/// The logic of a noise adapter that loses reads: at each call, its shot's
/// generator steps, and each read whose bit the step gives is zero is sent
/// as a skip.
fn losing_reads(
    generator: &mut u64,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    *generator = generator
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let drawn = *generator >> 32;
    let noisy: Vec<bool> = (input.instructions().iter().enumerate())
        .map(|(i, &read)| read && drawn >> i & 1 == 1)
        .collect();
    below.send(&noisy)
}

/// `noise >> simulator`: a noise adapter whose states draw with [`NOISE`],
/// losing reads, over a simulator given the run's seed, [`SEED`], which
/// reads its shots' seeds.
fn noisy() -> impl Executor<InstructionBatch<bool>> + Describe {
    let noise = adapter_fn(drawing, losing_reads)
        .drawing(NOISE)
        .shots(OnePerShot);
    (noise >> reading_simulator()).expect("bools fit")
}

/// What `executor` reads on the target chain's shots, eight reads each.
fn reads(executor: &mut impl Executor<InstructionBatch<bool>>) -> Vec<(ShotId, u32, Value)> {
    let (read, end) = run(executor, &shots(&SHOTS), &[true; 8]).expect("the call runs");
    end.expect("the call ends");
    values(&read)
}

/// `noise >> simulator` ([`noisy`]), over a simulator that offers
/// replayable shots: each shot's noise is its own, so the chain offers
/// replayable shots too.
#[test]
fn a_drawing_adapter_offers_replayable_shots() {
    assert!(
        reading_simulator().capabilities().replayable,
        "the simulator replays its shots"
    );
    assert!(noisy().capabilities().replayable);
}

/// A noise adapter whose states draw with [`NOISE`], over a simulator given
/// the run's seed, a state per shot, which keeps each shot's state across
/// calls: a program with feedback, run through the noise, continues each
/// shot in a later call, so the adapter offers resumable shots upstream.
#[test]
fn a_drawing_adapter_passes_resumable_shots_up() {
    let simulator = executor_fn(
        Capabilities::default()
            .with_qubits(4)
            .with_resumable_shots(true),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(SEED)
    .shots(OnePerShot);
    assert!(
        simulator.capabilities().resumable_shots,
        "the simulator keeps each shot's state across calls"
    );
    let noise = adapter_fn(drawing, losing_reads)
        .drawing(NOISE)
        .shots(OnePerShot);
    let executor = (noise >> simulator).expect("bools fit");
    assert!(executor.capabilities().resumable_shots);
}

/// [`ample_simulator`], which offers every capability it can, under an
/// adapter with a state per shot: an observer that draws nothing offers all
/// the simulator offers; a noise adapter whose states draw with [`NOISE`]
/// offers it all but the stated exception (no log probability), with
/// [`NOISE`] added to the ids drawn with.
#[test]
fn it_offers_what_the_executor_below_offers() {
    let ample = ample_simulator().capabilities();
    let observer = adapter_fn(no_state, pass).shots(OnePerShot);
    let executor = (observer >> ample_simulator()).expect("bools fit");
    assert_eq!(executor.capabilities(), ample);
    let noise = adapter_fn(drawing, losing_reads)
        .drawing(NOISE)
        .shots(OnePerShot);
    let executor = (noise >> ample_simulator()).expect("the simulator is given the run's seed");
    let mut expected = ample
        .drawing(NOISE)
        .expect("nothing below draws with NOISE");
    expected.log_probability = false;
    assert_eq!(executor.capabilities(), expected);
}

/// Two [`scant_simulator`]s, which offer only what they must, one bounded
/// (3 qubits, 2 shots at once) and one not, each under an adapter with a
/// state per shot, an observer that draws nothing or a noise adapter whose
/// states draw with [`NOISE`]: it offers nothing the simulator does not,
/// [`NOISE`] among the ids drawn with apart; and a job beyond the bounded
/// simulator in every need, given to the chain, is refused with each need
/// unmet, as it is with no adapter.
#[test]
fn it_offers_nothing_the_executor_below_does_not() {
    let noise = || {
        adapter_fn(drawing, losing_reads)
            .drawing(NOISE)
            .shots(OnePerShot)
    };
    for (qubits, max_shots) in [(Some(3), Some(2)), (None, None)] {
        let scant = || scant_simulator(qubits, max_shots);
        let offered = scant().capabilities();
        let observer = adapter_fn(no_state, pass).shots(OnePerShot);
        let executor = (observer >> scant()).expect("bools fit");
        assert_eq!(executor.capabilities(), offered);
        let executor = (noise() >> scant()).expect("the simulator is given the run's seed");
        let expected = offered
            .drawing(NOISE)
            .expect("nothing below draws with NOISE");
        assert_eq!(executor.capabilities(), expected);
    }
    let bounded = || scant_simulator(Some(3), Some(2));
    let every_need = [
        Need::Qubits,
        Need::ResumableShots,
        Need::Resets,
        Need::Replayable,
        Need::LogProbability,
        Need::InstructionSet,
        Need::Shots,
    ];
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(bounded()),
        every_need,
        "the simulator meets none of the job's needs"
    );
    let observer = adapter_fn(no_state, pass).shots(OnePerShot);
    let executor = (observer >> bounded()).expect("bools fit");
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(executor),
        every_need
    );
    let executor = (noise() >> bounded()).expect("the simulator is given the run's seed");
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(executor),
        every_need
    );
}

/// A device below, given the run's seed, that offers one yes/no capability
/// alone ([`each_capability_alone`]), each in turn, under an adapter with a
/// state per shot: an observer that draws nothing offers that capability
/// alone, as the device does; a noise adapter whose states draw with
/// [`NOISE`] offers it too, unless it is the stated exception (the log
/// probability), with [`NOISE`] added to the ids drawn with.
#[test]
fn it_passes_each_capability_up_on_its_own() {
    for lone in each_capability_alone() {
        let observer = adapter_fn(no_state, pass).shots(OnePerShot);
        let executor = compose(observer, Offering(lone.clone())).expect("bools fit");
        assert_eq!(executor.capabilities(), lone);
        let noise = adapter_fn(drawing, losing_reads)
            .drawing(NOISE)
            .shots(OnePerShot);
        let executor =
            compose(noise, Offering(lone.clone())).expect("the device is given the run's seed");
        let mut expected = lone.drawing(NOISE).expect("nothing below draws with NOISE");
        expected.log_probability = false;
        assert_eq!(executor.capabilities(), expected);
    }
}

/// A lowering with a state per shot, from a set that names the flip `flip`
/// and puts it first to one that numbers the bits' instructions otherwise,
/// over the bits ([`lowering`]): it offers upstream the set it accepts, not
/// the one below.
#[test]
fn a_rewriter_offers_the_set_it_accepts() {
    let executor = compose(
        lowering(),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("the bits have every instruction it sends");
    let offered = executor.capabilities().instruction_set;
    assert_eq!(offered.as_deref(), Some(&flip_first_set()));
}

/// A lowering with a state per shot that renames the flip and numbers the
/// calls otherwise ([`lowering`]), run on two shots, gives what the
/// program written in the bits' own set gives alone: each call reaches the
/// bits as the instruction it names, and each read keeps its tag.
#[test]
fn a_logic_that_rewrites_each_call_keeps_its_tags() {
    let mut lowered = compose(
        lowering(),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("the bits have every instruction it sends");
    let mut bare = TestDoubleExecutor::with_model(Bits::default(), 7);
    let two = shots(&[0, 1]);
    for (program, written) in flip_and_toss_flip_first()
        .iter()
        .zip(flip_and_toss(&bits_set()))
    {
        let through = values(&outcomes(&mut lowered, &two, program));
        assert!(!through.is_empty(), "each batch reads a bit");
        assert_eq!(through, values(&outcomes(&mut bare, &two, &written)));
    }
}

/// `noise >> simulator` ([`noisy`]) keeps the promise of `replayable`:
/// each of the target chain's shots draws alone, among the others, and
/// split into calls, what it draws among them, so it reads the same (the
/// conformance kit's check).
#[test]
fn a_drawing_adapters_shot_draws_alone_what_it_draws_among_the_others() {
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        noisy,
        &shots(&SHOTS),
        &[&[true; 8], &[true; 8]],
    );
}

/// `noise >> simulator` ([`noisy`]), built twice from the one seed, given
/// to the simulator: the second chain reads what the first read, the
/// noise's draws included, as a run replayed from its seed does.
#[test]
fn a_chain_with_a_drawing_adapter_replays_from_its_one_seed() {
    let first = reads(&mut noisy());
    assert_ne!(
        first,
        reads(&mut reading_simulator()),
        "the noise loses reads"
    );
    assert_eq!(reads(&mut noisy()), first);
}

/// The noise of [`noisy`] over its simulator placed on a thread of its
/// own, as a whole and shot by shot: the run's seed reaches the noise
/// through the placement, so each chain reads what the chain not placed
/// reads, the noise's draws included. The shots placed one by one each
/// run on their own thread, so their reads are compared in shot order.
#[test]
fn a_drawing_adapter_over_a_placed_executor_draws_from_the_runs_seed() {
    fn in_shot_order(
        executor: &mut impl Executor<InstructionBatch<bool>>,
    ) -> Vec<(ShotId, u32, Value)> {
        let mut read = reads(executor);
        read.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
        read
    }
    let noise = || {
        adapter_fn(drawing, losing_reads)
            .drawing(NOISE)
            .shots(OnePerShot)
    };
    let mut placed_whole = (noise()
        >> seeded_reading_simulator()
            .shots(OnePerShot)
            .run_on(OwnThread))
    .expect("bools fit");
    let mut placed_per_shot = (noise()
        >> seeded_reading_simulator()
            .run_on(OwnThread)
            .shots(OnePerShot))
    .expect("bools fit");
    let unplaced = in_shot_order(&mut noisy());
    assert_eq!(
        (
            in_shot_order(&mut placed_whole),
            in_shot_order(&mut placed_per_shot)
        ),
        (unplaced.clone(), unplaced)
    );
}

/// A noise adapter whose states draw with [`NOISE`], over a simulator
/// given the run's seed, run once from one seed and once from the next:
/// each shot's state seeds otherwise, so the noise's draws follow the one
/// seed, though only the simulator is given it.
#[test]
fn a_change_of_the_runs_seed_changes_a_drawing_adapters_draws() {
    let seeds_from = |seed: u64| {
        let seeds = Rc::new(RefCell::new(Vec::new()));
        let noise = {
            let seeds = Rc::clone(&seeds);
            adapter_fn(
                move |context: ShotContext| {
                    seeds.borrow_mut().push(context.seed());
                    Ok(())
                },
                pass,
            )
            .drawing(NOISE)
            .shots(OnePerShot)
        };
        let mut executor =
            (noise >> unseeded_simulator().seed(seed).shots(OnePerShot)).expect("bools fit");
        step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
        seeds.take()
    };
    let (first, next) = (seeds_from(SEED), seeds_from(SEED + 1));
    assert_eq!(first.len(), SHOTS.len(), "a state per shot");
    for (j, (a, b)) in SHOTS.iter().zip(first.iter().zip(&next)) {
        assert_ne!(a, b, "shot {j}");
    }
}

/// `readout >> noise >> simulator`: a readout error whose states draw
/// with [`READOUT`] over a noise adapter whose states draw with [`NOISE`],
/// over a simulator given the run's seed. The run's seed reaches through
/// the noise to the readout error: each shot's state of each adapter
/// seeds from that shot's context in the run, bound to its own id.
#[test]
fn two_drawing_adapters_seed_from_the_one_seed_each_with_its_own_id() {
    let seeds = Rc::new(RefCell::new(Vec::new()));
    let recording = |id: ComponentId| {
        let seeds = Rc::clone(&seeds);
        adapter_fn(
            move |context: ShotContext| {
                seeds
                    .borrow_mut()
                    .push((id, context.shot(), context.seed()));
                Ok(())
            },
            pass,
        )
        .drawing(id)
        .shots(OnePerShot)
    };
    let mut executor =
        (recording(READOUT) >> recording(NOISE) >> simulator()).expect("the ids differ");
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    let mut made = seeds.take();
    made.sort_unstable();
    let mut expected: Vec<_> = [READOUT, NOISE]
        .into_iter()
        .flat_map(|id| {
            SHOTS
                .iter()
                .map(move |&j| (id, j, ShotContext::new(SEED, j).rng_seed(id)))
        })
        .collect();
    expected.sort_unstable();
    assert_eq!(made, expected);
}

/// A noise adapter whose states draw with [`NOISE`], over a simulator
/// that states each shot's log probability: the adapter offers no
/// log probability, so its run reports none, since one without the noise's
/// draws would not be the shot's.
#[test]
#[ignore = "the report below passes up as it is, its log probabilities included; goes with \
            `.log_probability` on adapters (planned)"]
fn a_drawing_adapters_run_reports_no_log_probability() {
    let simulator = unseeded_simulator()
        .log_probability(|(): &()| -0.5)
        .seed(SEED)
        .shots(OnePerShot);
    let noise = adapter_fn(drawing, losing_reads)
        .drawing(NOISE)
        .shots(OnePerShot);
    let mut executor = (noise >> simulator).expect("bools fit");
    assert!(
        !executor.capabilities().log_probability,
        "the adapter offers no log probability"
    );
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    let report = executor.close();
    assert!(report.log_p.is_empty(), "{:?}", report.log_p);
}

/// The message of the panic `run` panics with.
fn panic_of(run: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(run)).expect_err("it panics");
    (payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned()))
    .or_else(|| payload.downcast_ref::<String>().cloned())
    .expect("a panic with a message")
}

/// The state closure of a noise adapter: draws from its shot's seed.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn drawing(context: ShotContext) -> Result<u64, Rejected> {
    Ok(context.seed())
}

/// A noise adapter whose states draw, built without the id it draws with:
/// its first draw panics naming the option it lacks, `.drawing(id)`.
#[test]
fn a_state_that_draws_under_no_id_panics_naming_drawing() {
    let noise = adapter_fn(drawing, pass).shots(OnePerShot);
    let mut executor = (noise >> simulator()).expect("bools fit");
    let message = panic_of(|| {
        let _ = run(&mut executor, &shots(&SHOTS), &[true]);
    });
    assert!(message.contains(".drawing(id)"), "{message}");
}

/// A noise adapter that draws with [`NOISE`] over a simulator not given
/// the run's seed: the chain is refused when it is built, before any
/// draw, the adapter's id named.
#[test]
fn an_adapter_drawing_with_no_run_seed_below_is_refused() {
    let noise = adapter_fn(drawing, pass).drawing(NOISE).shots(OnePerShot);
    let Err(faults) = noise >> unseeded_simulator().shots(OnePerShot) else {
        panic!("no run's seed reaches the noise")
    };
    let errors: Vec<_> = faults.0.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::NoRunSeed(NOISE)]);
}

/// A noise adapter built to draw with the last executor's id, which the
/// simulator below draws with: the chain is refused when it is built.
#[test]
fn an_adapter_drawing_with_an_id_in_use_below_is_refused() {
    let noise = adapter_fn(drawing, pass)
        .drawing(ComponentId::LAST_EXECUTOR)
        .shots(OnePerShot);
    let Err(faults) = noise >> simulator() else {
        panic!("the id is in use below")
    };
    let errors: Vec<_> = faults.0.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::RngInUse(ComponentId::LAST_EXECUTOR)]);
}

/// `noise >> (observer >> (noise >> simulator))`: a noise adapter whose
/// states draw with [`NOISE`], over an observer, over another noise
/// adapter drawing with [`NOISE`], over a simulator. Each part builds, and
/// the upstream noise is refused when the chain is built, since an adapter
/// further down draws with its id already, though one that draws nothing
/// stands between them.
#[test]
fn an_adapter_drawing_with_an_id_an_adapter_below_draws_with_is_refused() {
    let noise = || adapter_fn(drawing, pass).drawing(NOISE).shots(OnePerShot);
    let lower = (noise() >> simulator()).expect("one noise fits");
    let observed = (adapter_fn(no_state, pass) >> lower).expect("an observer fits");
    let Err(faults) = noise() >> observed else {
        panic!("the id is in use below")
    };
    let errors: Vec<_> = faults.0.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::RngInUse(NOISE)]);
}

/// A shot's state closure, which cannot make the state of shot 5, its
/// memory in use, run with every shot new to the adapter.
fn busy_at_five(context: ShotContext) -> Result<ShotId, Rejected> {
    if context.shot() == 5 {
        Err(Rejected::Unavailable("memory in use".into()))
    } else {
        Ok(context.shot())
    }
}

/// A decoder whose state cannot be made for shot 5: a call on the target
/// chain's shots is refused with its refusal, before any shot's logic
/// runs, so shot 2's does not either, and nothing goes below.
#[test]
fn a_state_that_cannot_be_made_refuses_the_call_before_any_logic_runs() {
    let ran = Rc::new(Cell::new(0));
    let counting = {
        let ran = Rc::clone(&ran);
        move |shot: &mut ShotId,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              outcomes: &mut Outcomes<'_>| {
            ran.set(ran.get() + 1);
            pass(shot, input, below, outcomes)
        }
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded =
        (adapter_fn(busy_at_five, counting).shots(OnePerShot) >> executor).expect("bools fit");
    let refused = run(&mut decoded, &shots(&SHOTS), &[true]).expect_err("shot 5 is busy");
    assert_eq!(refused, Rejected::Unavailable("memory in use".into()));
    assert_eq!(ran.get(), 0, "no logic ran");
    assert!(calls.borrow().is_empty(), "nothing went below");
}

/// The check of a lowering that has no rule for a skip (`false`): it
/// refuses a call holding one as malformed.
fn no_skip(input: &CallInput<'_, bool>) -> Result<(), Rejected> {
    if input.instructions().contains(&false) {
        Err(Rejected::Malformed("no rule for a skip".into()))
    } else {
        Ok(())
    }
}

/// A lowering with no rule for a skip, which checks for one, called with a
/// skip on the target chain's shots: the call is refused with the check's
/// refusal, before any shot's state is made and before any logic runs, so
/// nothing goes below.
#[test]
fn the_check_refuses_a_call_before_any_state_is_made_and_any_logic_runs() {
    let (made, ran) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let lowering = {
        let (made, ran) = (Rc::clone(&made), Rc::clone(&ran));
        adapter_fn(
            move |context: ShotContext| {
                made.set(made.get() + 1);
                its_shot(context)
            },
            move |shot: &mut ShotId,
                  input: &CallInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  outcomes: &mut Outcomes<'_>| {
                ran.set(ran.get() + 1);
                pass(shot, input, below, outcomes)
            },
        )
        .check(no_skip)
        .shots(OnePerShot)
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut lowered = (lowering >> executor).expect("bools fit");
    let refused = run(&mut lowered, &shots(&SHOTS), &[true, false]).expect_err("a skip");
    assert_eq!(refused, Rejected::Malformed("no rule for a skip".into()));
    assert_eq!(
        (made.get(), ran.get()),
        (0, 0),
        "no state made, no logic run"
    );
    assert!(calls.borrow().is_empty(), "nothing went below");
}

/// The same lowering called with a skip, then with reads only: the
/// refusal leaves it serving, and the call it accepts gives what it gives
/// with no check.
#[test]
fn a_call_the_check_refuses_leaves_the_adapter_serving() {
    let checked = adapter_fn(its_shot, pass).check(no_skip).shots(OnePerShot);
    let mut lowered = (checked >> below(TestDoubleModel::default())).expect("bools fit");
    let unchecked = adapter_fn(its_shot, pass).shots(OnePerShot);
    let mut bare = (unchecked >> below(TestDoubleModel::default())).expect("bools fit");
    run(&mut lowered, &shots(&SHOTS), &[true, false]).expect_err("a skip");
    let through = read(&mut lowered, &shots(&SHOTS), &[&[true, true]]);
    assert!(!through.is_empty(), "the batch reads bits");
    assert_eq!(through, read(&mut bare, &shots(&SHOTS), &[&[true, true]]));
}

/// A logic that counts its shot's calls in its state, then sends the call
/// down.
fn count_calls(
    calls: &mut u32,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    *calls += 1;
    below.send(input.instructions())
}

/// A profiler whose call on shots 2 and 5 is refused below, shot 2 busy:
/// the states made for the call are dropped, so a later call on shot 5
/// makes its state anew.
#[test]
fn a_refused_call_keeps_no_state_made_for_it() {
    let made = Rc::new(Cell::new(0));
    let counting_states = {
        let made = Rc::clone(&made);
        move |_: ShotContext| {
            made.set(made.get() + 1);
            Ok(0)
        }
    };
    let busy = below(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let mut profiled =
        (adapter_fn(counting_states, count_calls).shots(OnePerShot) >> busy).expect("bools fit");
    run(&mut profiled, &shots(&[2, 5]), &[true]).expect_err("shot 2 is busy");
    step(
        &mut profiled,
        &ShotSet::one(5),
        &[true],
        "shot 5 runs alone",
    );
    assert_eq!(made.get(), 3, "shot 5's state was made twice");
}

/// A profiler that ran shot 5, then is called on shots 2 and 5, refused
/// below for shot 2: the call is refused, though shot 5's logic ran on its
/// kept state, which stays as the logic left it. Held for the user
/// (whether a refused call may leave a state changed): this pins the
/// current behaviour.
#[test]
fn a_refused_call_leaves_the_kept_states_as_its_logic_left_them() {
    let counts = Rc::new(RefCell::new(BTreeMap::new()));
    let logging = {
        let counts = Rc::clone(&counts);
        move |(shot, calls): &mut (ShotId, u32),
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              outcomes: &mut Outcomes<'_>| {
            let sent = count_calls(calls, input, below, outcomes);
            counts.borrow_mut().insert(*shot, *calls);
            sent
        }
    };
    let busy = below(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let state = |context: ShotContext| Ok((context.shot(), 0));
    let mut profiled = (adapter_fn(state, logging).shots(OnePerShot) >> busy).expect("bools fit");
    step(
        &mut profiled,
        &ShotSet::one(5),
        &[true],
        "shot 5 runs alone",
    );
    run(&mut profiled, &shots(&[2, 5]), &[true]).expect_err("shot 2 is busy");
    step(
        &mut profiled,
        &ShotSet::one(5),
        &[true],
        "shot 5 runs again",
    );
    assert_eq!(counts.borrow()[&5], 3, "the refused call counted");
}

/// A profiler on shots 2 and 5 that keeps what each send returns, and
/// runs nothing, shot 2 refused below: every send returns `Ok`, as the
/// shots' sends go below merged once both shots' logic ran, and the call
/// is refused all the same.
#[test]
fn a_send_returns_ok_though_the_merged_call_it_goes_down_in_is_refused() {
    let returned = Rc::new(RefCell::new(Vec::new()));
    let keeping = {
        let returned = Rc::clone(&returned);
        move |(): &mut (),
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            returned.borrow_mut().push(below.send(input.instructions()));
            Ok(())
        }
    };
    let busy = below(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let mut profiled =
        (adapter_fn(no_state, keeping).shots(OnePerShot) >> busy).expect("bools fit");
    let refused = run(&mut profiled, &shots(&[2, 5]), &[true]).expect_err("shot 2 is busy");
    assert_eq!(refused, Rejected::Unavailable("busy".into()));
    assert_eq!(*returned.borrow(), [Ok(()), Ok(())]);
}

/// What a decoder that fails on shot 5, its syndrome missing, leaves of a
/// call on the target chain's shots, each shot writing a readout and
/// sending the call down before its logic may fail.
struct FailedOnFive {
    /// The shots whose logic ran, in order.
    ran: Vec<ShotId>,
    /// The calls below.
    calls: Vec<(Vec<ShotId>, Vec<bool>)>,
    /// What the call handed over.
    written: Vec<ShotOutcome>,
    /// How the call ended.
    end: Result<(), String>,
}

/// Runs a decoder that fails on shot 5 on the target chain's shots.
fn failing_on_five() -> FailedOnFive {
    let ran = Rc::new(RefCell::new(Vec::new()));
    let decoding = {
        let ran = Rc::clone(&ran);
        move |shot: &mut ShotId,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              outcomes: &mut Outcomes<'_>| {
            ran.borrow_mut().push(*shot);
            outcomes.push(Outcome::new(9, Value::One));
            below.send(input.instructions())?;
            if *shot == 5 {
                return Err("no syndrome".into());
            }
            Ok(())
        }
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded =
        (adapter_fn(its_shot, decoding).shots(OnePerShot) >> executor).expect("bools fit");
    let (written, end) = run(&mut decoded, &shots(&SHOTS), &[true]).expect("accepted");
    let ran = ran.borrow().clone();
    let calls = calls.borrow().clone();
    FailedOnFive {
        ran,
        calls,
        written,
        end,
    }
}

/// A decoder that fails on shot 5 of the target chain's shots: shot 6 and
/// 11's logic do not run.
#[test]
fn a_logic_that_fails_on_a_shot_stops_the_later_shots() {
    assert_eq!(failing_on_five().ran, [2, 5]);
}

/// A decoder that fails on shot 5 of the target chain's shots, after shot
/// 2 and 5 sent the call down: nothing goes below, not even what they
/// sent.
#[test]
fn a_logic_that_fails_on_a_shot_sends_nothing_below() {
    assert!(failing_on_five().calls.is_empty());
}

/// A decoder that fails on shot 5 of the target chain's shots, after shot
/// 2 and 5 wrote a readout: the call hands over what they wrote, then
/// ends with the logic's failure.
#[test]
fn a_logic_that_fails_on_a_shot_ends_the_call_after_what_it_wrote() {
    let FailedOnFive { written, end, .. } = failing_on_five();
    assert_eq!(values(&written), [(2, 9, Value::One), (5, 9, Value::One)]);
    assert_eq!(end, Err("no syndrome".to_owned()));
}

/// A decoder whose shot 2 sends its syndrome read, two bits, then runs a
/// check and writes its logical outcome, tag 9, and whose shot 5 fails,
/// its syndrome missing: shot 2's send went down alone, at its run, so
/// the call hands over what it read beside what shot 2 wrote, then ends
/// with the logic's failure.
#[test]
fn a_logic_that_fails_on_a_shot_ends_the_call_after_what_its_sends_alone_read() {
    let decoding = |shot: &mut ShotId,
                    _: &CallInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    outcomes: &mut Outcomes<'_>| {
        if *shot == 5 {
            return Err("no syndrome".into());
        }
        below.send(&[true, true])?;
        below.run(&[false])?;
        outcomes.push(Outcome::new(9, Value::One));
        Ok(())
    };
    let mut decoded = (adapter_fn(its_shot, decoding).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    let (written, end) = run(&mut decoded, &shots(&[2, 5]), &[true]).expect("accepted");
    let mut handed: Vec<_> = written.iter().map(|o| (o.shot, o.tag)).collect();
    handed.sort_unstable();
    assert_eq!(handed, [(2, 0), (2, 1), (2, 9)]);
    assert_eq!(end, Err("no syndrome".to_owned()));
}

/// A decoder whose logic fails on a call: every later call is refused
/// for it.
#[test]
fn a_logic_that_fails_fails_the_adapter() {
    let failing = |(): &mut (),
                   _: &CallInput<'_, bool>,
                   _: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| { Err("no syndrome".into()) };
    let mut decoded = (adapter_fn(no_state, failing).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    let (_, end) = run(&mut decoded, &ShotSet::one(2), &[true]).expect("accepted");
    end.expect_err("the logic failed");
    let later = run(&mut decoded, &ShotSet::one(5), &[true]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("no syndrome".into()));
}

/// A lowering whose odd shots send a batch with a `false`, which the
/// executor below refuses, after the even shots' batch went down: the call
/// fails, and so does the adapter.
#[test]
fn a_merged_send_refused_after_another_fails_the_call_and_the_adapter() {
    let checking = below(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    let (executor, calls) = recorded(checking);
    let mut lowered =
        (adapter_fn(its_shot, by_parity).shots(OnePerShot) >> executor).expect("bools fit");
    let (read, end) = run(&mut lowered, &shots(&SHOTS), &[true]).expect("the even shots ran");
    assert_eq!(values(&read).len(), 2, "the even shots' reads stand");
    end.expect_err("the odd shots' batch was refused");
    assert_eq!(calls.borrow().len(), 2, "both batches were called");
    let later = run(&mut lowered, &ShotSet::one(2), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// The same lowering called on shots 5 then 2: the odd shot's batch goes
/// below first and is refused, so the call is refused with that refusal,
/// nothing having gone down, and the adapter serves on, where the order 2
/// then 5 fails both
/// (`a_merged_send_refused_after_another_fails_the_call_and_the_adapter`).
#[test]
fn a_merged_send_refused_first_refuses_the_call_and_the_adapter_lives_on() {
    let checking = below(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    let mut lowered =
        (adapter_fn(its_shot, by_parity).shots(OnePerShot) >> checking).expect("bools fit");
    let refused = run(&mut lowered, &shots(&[5, 2]), &[true]).expect_err("shot 5's skip");
    assert_eq!(
        refused,
        Rejected::Instruction {
            at: 0,
            reason: "not a read".into()
        }
    );
    step(
        &mut lowered,
        &ShotSet::one(2),
        &[true],
        "the adapter serves on",
    );
}

/// A debugger whose shots send a skip, which the executor below refuses,
/// then run a read, on shots 2 and 5: shot 2's skip goes down alone, at
/// its run, as the call's first call below, so the call is refused with
/// that refusal, as for a merged send
/// (`a_merged_send_refused_first_refuses_the_call_and_the_adapter_lives_on`).
#[test]
fn a_send_alone_refused_first_refuses_the_call() {
    let skip_then_read = |(): &mut (),
                          _: &CallInput<'_, bool>,
                          below: &mut Below<'_, bool>,
                          _: &mut Outcomes<'_>| {
        below.send(&[false])?;
        below.run(&[true]).map(drop)
    };
    let checking = below(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    let mut debugged =
        (adapter_fn(no_state, skip_then_read).shots(OnePerShot) >> checking).expect("bools fit");
    let refused = run(&mut debugged, &shots(&[2, 5]), &[true]).expect_err("shot 2's skip");
    assert_eq!(
        refused,
        Rejected::Instruction {
            at: 0,
            reason: "not a read".into()
        }
    );
}

/// A logger whose states hold a file, here a counted handle, and whose
/// logic fails on shot 5: the states made for the failed call are
/// dropped, those of shots 2 and 6 too, though they ran or not.
#[test]
fn a_failed_call_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |context: ShotContext| Ok((context.shot(), Rc::clone(&file))),
            |(shot, _): &mut (ShotId, Rc<()>),
             input: &CallInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| {
                if *shot == 5 {
                    return Err("disk full".to_owned().into());
                }
                below.send(input.instructions())
            },
        )
        .shots(OnePerShot)
    };
    let mut executor = (logging >> below(TestDoubleModel::default())).expect("bools fit");
    let (_, end) = run(&mut executor, &shots(&[2, 5, 6]), &[true]).expect("accepted");
    end.expect_err("shot 5 failed");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// What a profiler's state counts: the instructions of its shot's calls,
/// by kind.
type Tally = BTreeMap<bool, u64>;

/// The state of a profiler's shot, before its first call: nothing counted.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn no_counts(_: ShotContext) -> Result<Tally, Rejected> {
    Ok(Tally::new())
}

/// The logic of a profiler: counts in its shot's state the instructions
/// it is called with, by kind, then sends the call down.
fn count_and_send(
    tally: &mut Tally,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    for &read in input.instructions() {
        *tally.entry(read).or_default() += 1;
    }
    below.send(input.instructions())
}

/// The target chain's profiler, a state per shot, called on its four shots
/// with a skip and a read, then on shots 5 and 6 with a read: closing it
/// gives every shot's counts, folded, beside the report below.
#[test]
fn a_counting_adapters_report_is_folded_over_its_shots() {
    let counting = adapter_fn(no_counts, count_and_send)
        .state_report(|tally: &Tally| tally.clone())
        .shots(OnePerShot);
    let mut profiled = (counting >> below(TestDoubleModel::default())).expect("bools fit");
    step(
        &mut profiled,
        &shots(&SHOTS),
        &[false, true],
        "the four shots run",
    );
    step(&mut profiled, &shots(&[5, 6]), &[true], "two run again");
    let (tally, _) = profiled.close();
    assert_eq!(tally, Tally::from([(false, 4), (true, 6)]));
}

/// The target chain's profiler, called on two of its shots, then closed
/// twice: the first close takes the states' counts, so the second reports
/// none, as an executor's second report is empty.
#[test]
fn a_second_close_reports_no_state() {
    let counting = adapter_fn(no_counts, count_and_send)
        .state_report(|tally: &Tally| tally.clone())
        .shots(OnePerShot);
    let mut profiled = (counting >> below(TestDoubleModel::default())).expect("bools fit");
    step(&mut profiled, &shots(&[2, 5]), &[true], "two shots run");
    let (first, _) = profiled.close();
    assert_eq!(first, Tally::from([(true, 2)]), "the states are reported");
    let (second, _) = profiled.close();
    assert_eq!(second, Tally::new());
}

/// A profiler whose logic fails on shot 5 of a call on shots 2, 5 and 6,
/// shot 2 having run alone before: the report holds shot 2's state, as
/// the failed call left it, and no state made for that call.
#[test]
fn a_failed_call_reports_no_state_made_for_it() {
    let failing_on_five = |(shot, calls): &mut (ShotId, u64),
                           input: &CallInput<'_, bool>,
                           below: &mut Below<'_, bool>,
                           _: &mut Outcomes<'_>| {
        *calls += 1;
        if *shot == 5 {
            return Err("no syndrome".into());
        }
        below.send(input.instructions())
    };
    let counting = adapter_fn(
        |context: ShotContext| Ok((context.shot(), 0)),
        failing_on_five,
    )
    .state_report(|(_, calls): &(ShotId, u64)| *calls)
    .shots(OnePerShot);
    let mut profiled = (counting >> below(TestDoubleModel::default())).expect("bools fit");
    step(
        &mut profiled,
        &ShotSet::one(2),
        &[true],
        "shot 2 runs alone",
    );
    let (_, end) = run(&mut profiled, &shots(&[2, 5, 6]), &[true]).expect("accepted");
    end.expect_err("shot 5 failed");
    let (calls, _) = profiled.close();
    assert_eq!(calls, 2, "shot 2's two calls, and no other state");
}

/// An executor below that holds open the reply to each call with a
/// `false`, for the test to end, and answers every other call at once, a
/// one for each `true`: a send whose reply fails after the call returned,
/// beside a run that reads.
struct HoldsFalse(Senders);

impl Executor<InstructionBatch<bool>> for HoldsFalse {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        if batch.instructions.contains(&false) {
            self.0.borrow_mut().push(sender);
            return Ok(reply);
        }
        for &shot in batch.shots.as_slice() {
            for (tag, &read) in (0..).zip(batch.instructions) {
                if read {
                    sender.push(one(shot, tag));
                }
            }
        }
        sender.done();
        Ok(reply)
    }
}

impl Describe for HoldsFalse {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for HoldsFalse {
    type Report = ();

    fn close(&mut self) {}
}

/// A decoder with a state per shot whose shot sends a syndrome round,
/// then a readout: as the logic returns, the round goes below, where its
/// reply waits, and the device refuses the readout as it goes down, which
/// ends the call; the round's reply fails, lost. The call ends with the
/// refusal that ended it, the reason its later calls are refused with,
/// not with the later loss.
#[test]
fn a_call_ended_by_a_refusal_below_ends_with_it_though_an_earlier_sends_reply_fails_later() {
    let decoding = |(): &mut (),
                    _: &CallInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        below.send(&[false])?;
        below.send(&[true])
    };
    let mut executor = compose(
        adapter_fn(no_state, decoding).shots(OnePerShot),
        holding_until_busy(),
    )
    .expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(
        (end, later),
        (
            Err("unavailable: busy".to_owned()),
            Rejected::EarlierFailure("unavailable: busy".into())
        )
    );
}

/// A decoder with a state per shot whose shot sends `false`, whose reply
/// waits below, then runs, so that the send goes below alone first, then
/// fails itself; the send's reply fails once the call returned. The call
/// ends with the decoder's failure, the one its later calls are refused
/// with.
#[test]
fn a_call_that_failed_ends_with_its_own_failure_though_a_send_fails_after_it_returned() {
    let failing = |(): &mut (),
                   _: &CallInput<'_, bool>,
                   below: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| {
        below.send(&[false])?;
        below.run(&[true])?;
        Err("no syndrome".into())
    };
    let senders = Senders::default();
    let mut executor = compose(
        adapter_fn(no_state, failing).shots(OnePerShot),
        HoldsFalse(Rc::clone(&senders)),
    )
    .expect("bools fit");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let sender = senders
        .borrow_mut()
        .pop()
        .expect("the send's reply waits below");
    sender.fail("lost".into());
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(
        (end, later),
        (
            ReplyStatus::Failed("no syndrome".into()),
            Rejected::EarlierFailure("no syndrome".into())
        )
    );
}

/// A profiler with a state per shot whose call on shots 0 and 1 goes below
/// merged, as a mark whose reply waits below and fails once the call
/// returned: the call was served, so the states made for it are kept, and
/// reported, though the adapter failed.
#[test]
fn a_state_whose_merged_sends_reply_failed_later_is_reported() {
    let senders = Senders::default();
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| *calls)
        .shots(OnePerShot);
    let mut profiled = compose(counting, HoldsFalse(Rc::clone(&senders))).expect("bools fit");
    let mut reply = profiled
        .call(InstructionBatchRef {
            shots: &shots(&[0, 1]),
            instructions: &[false],
        })
        .expect("the call is accepted");
    (senders.borrow_mut().pop())
        .expect("the merged send's reply waits below")
        .fail("lost".into());
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(end, ReplyStatus::Failed("lost".into()), "the call failed");
    let (calls, ()) = profiled.close();
    assert_eq!(calls, 2, "shots 0 and 1, a call each");
}

/// A logger whose states hold a file, here a counted handle, on shots 2
/// and 5, whose batches fail below for shot 5: the call fails through
/// its merged send's reply, and the states made for it are dropped.
#[test]
fn a_call_failed_by_a_sends_reply_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(move |_: ShotContext| Ok(Rc::clone(&file)), pass).shots(OnePerShot)
    };
    let failing = below(TestDoubleModel {
        failing: Some(5),
        ..TestDoubleModel::default()
    });
    let mut executor = (logging >> failing).expect("bools fit");
    let (_, end) = run(&mut executor, &shots(&[2, 5]), &[true]).expect("accepted");
    end.expect_err("shot 5's batch failed");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// The same logger, whose batches are cancelled below for shot 5: the
/// call ends cancelled, and the states made for it are dropped.
#[test]
fn a_cancelled_call_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(move |_: ShotContext| Ok(Rc::clone(&file)), pass).shots(OnePerShot)
    };
    let cancelling = below(TestDoubleModel {
        cancelling: Some(5),
        ..TestDoubleModel::default()
    });
    let mut executor = (logging >> cancelling).expect("bools fit");
    let (_, end) = run(&mut executor, &shots(&[2, 5]), &[true]).expect("accepted");
    end.expect_err("shot 5's batch was cancelled");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// A lowering with a state per shot, whose shot 0 reads one bit and shot
/// 1 two, so that their sends go below as two calls: shot 0's reply waits
/// below, and shot 1's fails. The call's reply ends failed at its first
/// poll, though shot 0's is still pending.
#[test]
fn a_send_whose_reply_fails_ends_the_call_though_another_is_pending() {
    let lowering = adapter_fn(
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId,
         _: &CallInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            let reads = usize::try_from(*shot).expect("a small shot") + 1;
            below.send(&vec![true; reads])
        },
    )
    .shots(OnePerShot);
    let model = TestDoubleModel {
        deferred: vec![0],
        failing: Some(1),
        ..TestDoubleModel::default()
    };
    let mut executor = (lowering >> below(model)).expect("bools fit");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shots(&[0, 1]),
            instructions: &[true],
        })
        .expect("both shots accept");
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(end, ReplyStatus::Failed("lost".into()));
}

/// A probe with a state per shot, each knowing its shot, whose call on
/// shot 0 sends a skip, whose reply below fails once the call returned,
/// then called on shot 1 before that reply is read: the adapter has not
/// recorded the failure, so it admits the call, and keeps and reports the
/// state made for it, though an executor would not.
#[test]
fn a_state_made_for_a_call_admitted_before_an_earlier_failure_was_recorded_is_reported() {
    let senders = Senders::default();
    let probing = adapter_fn(its_shot, pass)
        .state_report(|shot: &ShotId| BTreeMap::from([(*shot, 1_u32)]))
        .shots(OnePerShot);
    let mut probed = compose(probing, HoldsFalse(Rc::clone(&senders))).expect("bools fit");
    let mut first = probed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[false],
        })
        .expect("shot 0 accepts");
    (senders.borrow_mut().pop())
        .expect("shot 0's reply waits below")
        .fail("lost".into());
    step(&mut probed, &ShotSet::one(1), &[true], "shot 1's call runs");
    let end = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        end,
        ReplyStatus::Failed("lost".into()),
        "shot 0's call failed"
    );
    let (reported, ()) = probed.close();
    assert_eq!(reported.get(&1), Some(&1), "{reported:?}");
}

/// A profiler whose report has a bug, called on two of the target chain's
/// shots, then closed: the close closes the executor below, then the
/// report's panic reaches the closer.
#[test]
fn a_close_whose_state_report_panics_closes_below_then_raises_it() {
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let counting = adapter_fn(no_counts, count_and_send)
        .state_report(|_: &Tally| -> Tally { panic!("the report has a bug") })
        .shots(OnePerShot);
    let mut profiled = compose(counting, counted).expect("bools fit");
    step(&mut profiled, &shots(&[2, 5]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(panic_text(&*panicked), Some("the report has a bug"));
    assert_eq!(closes.get(), 1, "the closes below");
}

/// A profiler with a state per shot whose report type has a bug: its
/// `Default` panics. Called on two of the target chain's shots, then
/// closed: the close frees both states and closes the executor below,
/// then the panic reaches the closer.
#[test]
fn a_close_whose_reports_default_panics_closes_below_then_raises_it() {
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| Unmade(*calls))
        .shots(OnePerShot);
    let mut profiled = compose(counting, counted).expect("bools fit");
    step(&mut profiled, &shots(&[2, 5]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(
        panic_text(&*panicked),
        Some("the report's default has a bug")
    );
    assert_eq!(closes.get(), 1, "the closes below");
}

/// A profiler with a state per shot whose report has a bug, over an
/// executor whose close has one too: the report's panic, the first,
/// reaches the closer once the executor below is closed, and the close
/// below's is left to the panic hook.
#[test]
fn a_state_reports_panic_wins_over_the_close_belows() {
    let (counted, closes) = close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|_: &u64| -> u64 { panic!("the report has a bug") })
        .shots(OnePerShot);
    let mut profiled = compose(counting, counted).expect("bools fit");
    step(&mut profiled, &shots(&[2, 5]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(
        (panic_text(&*panicked), closes.get()),
        (Some("the report has a bug"), 1)
    );
}

/// An observer and a profiler with a report, each with a state per shot
/// that fails to power down (a bug of its code), over an executor whose
/// close has a bug too, called on shot 1, then on shot 0, then closed:
/// the first state's panic, shot 1's, reaches the closer once both states
/// are freed and the executor below closed, the close below's left to the
/// panic hook.
#[test]
fn the_first_state_drops_panic_wins_over_the_close_belows() {
    let freed = Freed::default();
    let (counted, observed_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down).shots(OnePerShot);
    let mut observed = compose(observing, counted).expect("bools fit");
    let (counted, profiled_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| 0_u64)
        .shots(OnePerShot);
    let mut profiled = compose(profiling, counted).expect("bools fit");
    for shot in [1, 0] {
        step(&mut observed, &ShotSet::one(shot), &[true], "the call runs");
        step(&mut profiled, &ShotSet::one(shot), &[true], "the call runs");
    }
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the drops panic");
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the drops panic");
    assert_eq!(
        (
            observed_panic.downcast_ref(),
            profiled_panic.downcast_ref(),
            freed_so_far(&freed)
        ),
        (
            Some(&PowerDownFailed(1)),
            Some(&PowerDownFailed(1)),
            vec![1, 0, 1, 0]
        ),
        "the panics raised, and the states freed"
    );
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// An observer and a profiler with a report, each with a state per shot
/// and no bug of its code, over an executor whose close has one: the close
/// below's panic reaches the closer.
#[test]
fn a_close_belows_panic_reaches_the_closer_when_nothing_panicked_before() {
    let mut observed = compose(
        adapter_fn(no_state, pass).shots(OnePerShot),
        PanicsOnClose(below(TestDoubleModel::default())),
    )
    .expect("bools fit");
    step(&mut observed, &shots(&[0, 1]), &[true], "the call runs");
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the close panics");
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| *calls)
        .shots(OnePerShot);
    let mut profiled =
        compose(counting, PanicsOnClose(below(TestDoubleModel::default()))).expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the close panics");
    assert_eq!(
        (panic_text(&*observed_panic), panic_text(&*profiled_panic)),
        (Some("the close has a bug"), Some("the close has a bug"))
    );
}

/// A profiler with two bugs: its report panics as `close` reads the first
/// of two states that fail to power down. The report's panic reaches the
/// closer once both states are freed and the executor below closed, and
/// the states', each freed in a catch of its own, are left to the panic
/// hook, rather than abort the process.
#[test]
fn a_state_reports_panic_reaches_the_closer_though_the_states_drops_panic_too() {
    let freed = Freed::default();
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| -> u64 { panic!("the report has a bug") })
        .shots(OnePerShot);
    let mut profiled = compose(profiling, counted).expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let panicked =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the report panics");
    assert_eq!(
        (panic_text(&*panicked), freed_so_far(&freed), closes.get()),
        (Some("the report has a bug"), vec![0, 1], 1)
    );
}

/// An observer and a profiler with a report, each with a state per shot
/// that fails to power down (a bug of its code), called on shot 1, then
/// on shot 0, then closed twice: the first close frees both states, in the
/// order their shots first ran, and closes the executor below, then the
/// first state's panic, shot 1's, reaches the closer; the second closes
/// nothing below.
#[test]
fn a_close_whose_states_drops_panic_closes_below_then_raises_the_first() {
    let freed = Freed::default();
    let (counted, observed_closes) = close_counted(below(TestDoubleModel::default()));
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down).shots(OnePerShot);
    let mut observed = compose(observing, counted).expect("bools fit");
    let (counted, profiled_closes) = close_counted(below(TestDoubleModel::default()));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| 0_u64)
        .shots(OnePerShot);
    let mut profiled = compose(profiling, counted).expect("bools fit");
    for shot in [1, 0] {
        step(&mut observed, &ShotSet::one(shot), &[true], "the call runs");
        step(&mut profiled, &ShotSet::one(shot), &[true], "the call runs");
    }
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the drops panic");
    let observed_first = observed_closes.get();
    observed.close();
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the drops panic");
    let profiled_first = profiled_closes.get();
    profiled.close();
    assert_eq!(
        (
            observed_panic.downcast_ref(),
            profiled_panic.downcast_ref(),
            freed_so_far(&freed)
        ),
        (
            Some(&PowerDownFailed(1)),
            Some(&PowerDownFailed(1)),
            vec![1, 0, 1, 0]
        ),
        "the panics raised, and the states freed"
    );
    assert_eq!(
        (
            (observed_first, observed_closes.get()),
            (profiled_first, profiled_closes.get())
        ),
        ((1, 1), (1, 1)),
        "the observer's and the profiler's closes below: after the panic, after a second close"
    );
}

/// An observer whose states fail to power down ran a call on shots 0 and
/// 1, and is dropped unclosed: its drop frees both and raises neither
/// panic, which only a close reports.
#[test]
fn dropped_it_frees_every_state_raising_no_panic_of_their_drops() {
    let freed = Freed::default();
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down).shots(OnePerShot);
    let mut observed = compose(observing, below(TestDoubleModel::default())).expect("bools fit");
    step(&mut observed, &shots(&[0, 1]), &[true], "the call runs");
    catch_unwind(AssertUnwindSafe(|| drop(observed)))
        .expect("a drop raises no panic of the states' drops");
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// An observer with a state per shot, each powering its shot's device down
/// through a connection the logic holds, ran a call on shots 0 and 1, and
/// is dropped unclosed: both states are dropped first, while the logic,
/// with what it holds, and the executor below are still there, as with
/// one state.
#[test]
fn dropped_it_frees_its_states_before_its_closures_and_the_executor_below() {
    let log = DropLog::default();
    let (making, connection) = (Rc::clone(&log), Dropped("logic", Rc::clone(&log)));
    let observing = adapter_fn(
        move |_: ShotContext| Ok(Dropped("state", Rc::clone(&making))),
        move |_: &mut Dropped,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            let _held = &connection;
            below.send(input.instructions())
        },
    )
    .shots(OnePerShot);
    let mut observed = compose(observing, DroppedBelow::new(&log)).expect("bools fit");
    step(&mut observed, &shots(&[0, 1]), &[true], "the call runs");
    drop(observed);
    let dropped = log.borrow();
    assert_eq!(dropped[..2], ["state", "state"]);
    assert_eq!(
        dropped.len(),
        4,
        "two states, the logic and below: {dropped:?}"
    );
}

/// A profiler with a state per shot, whose state closure, logic, check
/// and report maker each hold a handle on its connection that fails to
/// release (a bug), ran a call on shots 0 and 1, and is dropped unclosed:
/// its drop releases the four handles, each closure in a catch of its
/// own, and raises no panic of their drops, as with one state.
#[test]
fn dropped_it_frees_what_its_closures_capture_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let (making, logic, checking, reporting) =
        (handle.another(), handle.another(), handle.another(), handle);
    let profiling = adapter_fn(
        move |_: ShotContext| {
            let _held = &making;
            Ok(0_u64)
        },
        move |calls: &mut u64,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            let _held = &logic;
            *calls += 1;
            below.send(input.instructions())
        },
    )
    .check(move |_: &CallInput<'_, bool>| {
        let _held = &checking;
        Ok(())
    })
    .state_report(move |calls: &u64| {
        let _held = &reporting;
        *calls
    })
    .shots(OnePerShot);
    let mut profiled = compose(profiling, below(TestDoubleModel::default())).expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    catch_unwind(AssertUnwindSafe(|| drop(profiled)))
        .expect("a drop raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 4);
}

/// A lowering to pulses, with a state per shot, on shots 2 and 5: shot 2
/// sends two pulses down, kept to be merged with the other shots' sends,
/// then shot 5's logic panics (a bug). The logic's panic reaches the
/// caller once the call freed both pulses' copies, each in a catch of its
/// own, rather than abort the process; dropped, the adapter raises no
/// panic.
#[test]
fn a_failed_calls_kept_sends_are_freed_raising_no_panic_of_their_drops() {
    let released = Rc::new(Cell::new(0));
    let mut lowered = lowering_failing_at_5(&released);
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut lowered, &shots(&[2, 5]), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the lowering has a bug"));
    assert_eq!(released.get(), 2, "both copies, freed by the call");
    catch_unwind(AssertUnwindSafe(|| drop(lowered))).expect("a drop raises no panic");
}

/// As [`a_failed_calls_kept_sends_are_freed_raising_no_panic_of_their_drops`],
/// with the adapter dropped as the logic's panic unwinds, as a chain's run
/// drops its executor: the logic's panic reaches the caller, and the
/// process does not abort. The call already freed the copies as the panic
/// left it, so the adapter's drop during the unwind finds none.
#[test]
fn dropped_as_a_panic_unwinds_it_frees_the_kept_sends_raising_none() {
    let released = Rc::new(Cell::new(0));
    let mut lowered = lowering_failing_at_5(&released);
    let panicked = catch_unwind(AssertUnwindSafe(move || {
        run(&mut lowered, &shots(&[2, 5]), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the lowering has a bug"));
    assert_eq!(released.get(), 2);
}

/// A lowering to pulses whose shot 2 sends two pulses down, and whose shot
/// 5's logic panics (a bug), over a [`pulse_device`]; `released` counts the
/// copies' drops.
fn lowering_failing_at_5(
    released: &Rc<Cell<usize>>,
) -> impl Executor<InstructionBatch<bool>> + use<> {
    let pulses = [Pulse::on(0, released), Pulse::on(1, released)];
    let lowering = adapter_fn(
        |context: ShotContext| Ok(context.shot()),
        move |shot: &mut ShotId,
              _: &CallInput<'_, bool>,
              below: &mut Below<'_, Pulse>,
              _: &mut Outcomes<'_>| {
            assert!(*shot != 5, "the lowering has a bug");
            below.send(&pulses)
        },
    )
    .shots(OnePerShot);
    compose(lowering, pulse_device()).expect("pulses fit")
}

/// A lowering to pulses, with a state per shot, sends two pulses down,
/// the second of which panics as the adapter copies it to keep it (a
/// bug): the copy's panic reaches the caller once the copy made before it
/// is freed, in a catch of its own, rather than abort the process.
#[test]
fn a_send_whose_copy_panics_frees_the_copies_made_before_it() {
    let released = Rc::new(Cell::new(0));
    let pulses = [
        Pulse::on(0, &released),
        Pulse::failing_to_copy(1, &released),
    ];
    let lowering = adapter_fn(
        |_: ShotContext| Ok(()),
        move |(): &mut (),
              _: &CallInput<'_, bool>,
              below: &mut Below<'_, Pulse>,
              _: &mut Outcomes<'_>| below.send(&pulses),
    )
    .shots(OnePerShot);
    let mut lowered = compose(lowering, pulse_device()).expect("pulses fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut lowered, &shots(&[0]), &[true])
    }))
    .expect_err("the copy panics");
    assert_eq!(panic_text(&*panicked), Some("a pulse's copy has a bug"));
    assert_eq!(released.get(), 1, "the first pulse's copy");
}

/// A lowering to pulses, with a state per shot, sends two pulses down in
/// each call, whose copies it keeps until it sends them below: each call
/// frees both once sent, each in a catch of its own, absorbing their
/// drops' panics after the panic hook reported them, as a placement does
/// with its copy of a batch; its reply is unharmed, the adapter goes on
/// serving, and its drop raises no panic.
#[test]
fn a_kept_sends_drop_that_panics_after_its_call_ran_leaves_the_adapter_serving() {
    let released = Rc::new(Cell::new(0));
    let pulses = [Pulse::on(0, &released), Pulse::on(1, &released)];
    let lowering = adapter_fn(
        |_: ShotContext| Ok(()),
        move |(): &mut (),
              _: &CallInput<'_, bool>,
              below: &mut Below<'_, Pulse>,
              _: &mut Outcomes<'_>| below.send(&pulses),
    )
    .shots(OnePerShot);
    let mut lowered = compose(lowering, pulse_device()).expect("pulses fit");
    step(&mut lowered, &shots(&[0]), &[true], "the call runs");
    assert_eq!(released.get(), 2, "both copies, freed by the call");
    step(&mut lowered, &shots(&[0]), &[true], "the next call runs");
    assert_eq!(released.get(), 4);
    catch_unwind(AssertUnwindSafe(|| drop(lowered))).expect("a drop raises no panic");
}

/// A lowering to pulses, with a state per shot, sends a pulse down, then
/// runs another, in each call: the copy it kept of the sent pulse goes
/// below alone before the run, and is freed then, in a catch of its own,
/// absorbing its drop's panic, and the adapter goes on serving.
#[test]
fn a_shots_sends_that_go_alone_are_freed_at_its_run_leaving_the_adapter_serving() {
    let released = Rc::new(Cell::new(0));
    let after_run = Rc::new(Cell::new(0));
    let pulses = [Pulse::on(0, &released), Pulse::on(1, &released)];
    let seen = Rc::clone(&after_run);
    let counted = Rc::clone(&released);
    let lowering = adapter_fn(
        |_: ShotContext| Ok(()),
        move |(): &mut (),
              _: &CallInput<'_, bool>,
              below: &mut Below<'_, Pulse>,
              _: &mut Outcomes<'_>| {
            below.send(&pulses[..1])?;
            below.run(&pulses[1..])?;
            seen.set(counted.get());
            Ok(())
        },
    )
    .shots(OnePerShot);
    let mut lowered = compose(lowering, pulse_device()).expect("pulses fit");
    step(&mut lowered, &shots(&[0]), &[true], "the call runs");
    assert_eq!(
        after_run.get(),
        1,
        "the sent pulse's copy, freed at the run"
    );
    step(&mut lowered, &shots(&[0]), &[true], "the next call runs");
    assert_eq!(released.get(), 2);
}

/// A decoder whose states fail to power down finds no syndrome in a call
/// on three new shots: the call frees each state made for it, and the
/// first one's panic reaches the caller once every state is freed, rather
/// than abort the process.
#[test]
fn a_failed_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let decoding =
        adapter_fn(powering_down_badly(&freed, None), finds_no_syndrome).shots(OnePerShot);
    let mut decoded = compose(decoding, below(TestDoubleModel::default())).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut decoded, &shots(&[0, 1, 2]), &[true])
    }))
    .expect_err("the states' drops panic");
    assert_eq!(
        (panicked.downcast_ref(), freed_so_far(&freed)),
        (Some(&PowerDownFailed(0)), vec![0, 1, 2])
    );
}

/// An observer whose states fail to power down has no room for shot 2: a
/// call on shots 0, 1 and 2 frees the states it made for shots 0 and 1,
/// and the first one's panic reaches the caller once both are freed,
/// rather than abort the process.
#[test]
fn a_refused_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let observing = adapter_fn(powering_down_badly(&freed, Some(2)), send_down).shots(OnePerShot);
    let mut observed = compose(observing, below(TestDoubleModel::default())).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut observed, &shots(&[0, 1, 2]), &[true])
    }))
    .expect_err("the states' drops panic");
    assert_eq!(
        (panicked.downcast_ref(), freed_so_far(&freed)),
        (Some(&PowerDownFailed(0)), vec![0, 1])
    );
}

/// A decoder whose states fail to power down, and whose logic panics (two
/// bugs), in a call on two new shots: the logic's panic reaches the
/// caller, and the states, freed as it unwinds, leave theirs to the panic
/// hook, rather than abort the process.
#[test]
fn a_logics_panic_reaches_the_caller_though_its_new_states_drops_panic_too() {
    let freed = Freed::default();
    let decoding = adapter_fn(powering_down_badly(&freed, None), panics_decoding).shots(OnePerShot);
    let mut decoded = compose(decoding, below(TestDoubleModel::default())).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut decoded, &shots(&[0, 1]), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(
        (panic_text(&*panicked), freed_so_far(&freed)),
        (Some("the logic has a bug"), vec![0, 1])
    );
}

/// A lowering with no rule for a skip, with a state per shot, which checks
/// for one and counts its checks, closed, then called with a skip: the call
/// is refused as closed, before the check runs.
#[test]
fn a_closed_adapter_refuses_a_call_before_its_check_runs() {
    let checks = Rc::new(Cell::new(0));
    let counted_check = {
        let checks = Rc::clone(&checks);
        move |input: &CallInput<'_, bool>| {
            checks.set(checks.get() + 1);
            no_skip(input)
        }
    };
    let checked = adapter_fn(its_shot, pass)
        .check(counted_check)
        .shots(OnePerShot);
    let mut lowered = (checked >> below(TestDoubleModel::default())).expect("bools fit");
    lowered.close();
    let refused = run(&mut lowered, &shots(&SHOTS), &[true, false]).expect_err("closed");
    assert_eq!((refused, checks.get()), (Rejected::Closed, 0));
}

/// An observer, and a profiler given a report, each with a state per shot
/// and closed: each refuses calls, and makes no state, whether the adapter
/// reports or not.
#[test]
fn a_closed_adapter_refuses_calls() {
    let made = Rc::new(Cell::new(0));
    let counting_states = {
        let made = Rc::clone(&made);
        move |_: ShotContext| {
            made.set(made.get() + 1);
            Ok(())
        }
    };
    let mut observed = (adapter_fn(counting_states.clone(), pass).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    observed.close();
    let observed_refusal = run(&mut observed, &ShotSet::one(2), &[true]).expect_err("closed");
    let profiling = adapter_fn(counting_states, pass)
        .state_report(|(): &()| 0_u64)
        .shots(OnePerShot);
    let mut profiled = (profiling >> below(TestDoubleModel::default())).expect("bools fit");
    profiled.close();
    let profiled_refusal = run(&mut profiled, &ShotSet::one(2), &[true]).expect_err("closed");
    assert_eq!(
        (observed_refusal, profiled_refusal),
        (Rejected::Closed, Rejected::Closed)
    );
    assert_eq!(made.get(), 0, "neither adapter made a state");
}

/// A logger whose states each hold a file, here a counted handle, run on
/// two shots, then closed: the states are dropped.
#[test]
fn closing_drops_the_states() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(move |_| Ok(Rc::clone(&file)), pass).shots(OnePerShot)
    };
    let mut logged = (logging >> below(TestDoubleModel::default())).expect("bools fit");
    step(&mut logged, &shots(&[2, 5]), &[true], "the call runs");
    assert_eq!(Rc::strong_count(&file), 4, "each state holds the file");
    logged.close();
    assert_eq!(Rc::strong_count(&file), 2, "the states were dropped");
}

/// An observer with a state per shot, run on two shots, then closed: the
/// report is the executor below's, as if no adapter were there.
#[test]
fn closing_gives_the_report_below() {
    let observing = adapter_fn(its_shot, pass).shots(OnePerShot);
    let mut observed = (observing >> below(TestDoubleModel::default())).expect("bools fit");
    let mut bare = below(TestDoubleModel::default());
    step(
        &mut observed,
        &shots(&[2, 5]),
        &[true],
        "the call runs observed",
    );
    step(&mut bare, &shots(&[2, 5]), &[true], "the call runs alone");
    assert_eq!(
        format!("{:?}", observed.close()),
        format!("{:?}", bare.close())
    );
}

/// A logic with a bug panics on shot 5: the panic reaches the caller, and
/// the adapter fails, so later calls are refused.
#[test]
fn a_logic_that_panics_fails_the_adapter() {
    let buggy = |shot: &mut ShotId,
                 input: &CallInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 outcomes: &mut Outcomes<'_>| {
        assert!(*shot != 5, "the logic has a bug");
        pass(shot, input, below, outcomes)
    };
    let mut executor = (adapter_fn(its_shot, buggy).shots(OnePerShot)
        >> below(TestDoubleModel::default()))
    .expect("bools fit");
    panic_of(|| {
        let _ = run(&mut executor, &shots(&SHOTS), &[true]);
    });
    let later = run(&mut executor, &ShotSet::one(2), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A lowering with a state per shot whose check has a bug, and one whose
/// state closure has a bug on shot 5, each called on the target chain's
/// shots: the panic reaches the caller, and the adapter fails, so later
/// calls are refused, as after a logic's panic.
#[test]
fn a_check_or_a_state_closure_that_panics_fails_the_adapter() {
    let buggy_check =
        |_: &CallInput<'_, bool>| -> Result<(), Rejected> { panic!("the check has a bug") };
    let checking = adapter_fn(its_shot, pass)
        .check(buggy_check)
        .shots(OnePerShot);
    let mut checked = (checking >> below(TestDoubleModel::default())).expect("bools fit");
    let buggy_state = |context: ShotContext| {
        assert!(context.shot() != 5, "the state has a bug");
        its_shot(context)
    };
    let making = adapter_fn(buggy_state, pass).shots(OnePerShot);
    let mut made = (making >> below(TestDoubleModel::default())).expect("bools fit");
    let refusals = (
        refusal_after_a_panic(&mut checked, &shots(&SHOTS)),
        refusal_after_a_panic(&mut made, &shots(&SHOTS)),
    );
    assert!(
        matches!(
            refusals,
            (Rejected::EarlierFailure(_), Rejected::EarlierFailure(_))
        ),
        "the checking and the state-making adapters' later calls: {refusals:?}"
    );
}

/// The logic of a decoder that runs each call below and writes what it
/// reads, whatever its state.
fn run_each<S>(
    _: &mut S,
    input: &CallInput<'_, bool>,
    below: &mut Below<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    outcomes.extend(below.run(input.instructions())?);
    Ok(())
}

/// A decoder with a state per shot, on the target chain's four shots: each
/// shot's run goes below at once, as a call on that shot alone.
#[test]
fn a_shots_run_goes_below_on_that_shot_alone() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded =
        (adapter_fn(no_state, run_each).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut decoded, &shots(&SHOTS), &[true], "the call runs");
    let alone: Vec<_> = SHOTS.iter().map(|&j| (vec![j], vec![true])).collect();
    assert_eq!(*calls.borrow(), alone);
}

/// A debugger whose shots send two batches, then run a third, on shots 1
/// and 2: each shot's sends go below before its run, alone and in the
/// order the shot made them.
#[test]
fn a_shots_sends_go_below_before_its_run() {
    let send_send_run = |(): &mut (),
                         _: &CallInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         _: &mut Outcomes<'_>| {
        below.send(&[true])?;
        below.send(&[true, true])?;
        below.run(&[false]).map(drop)
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut debugged =
        (adapter_fn(no_state, send_send_run).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut debugged, &shots(&[1, 2]), &[true], "the call runs");
    let each = |j| {
        [
            (vec![j], vec![true]),
            (vec![j], vec![true, true]),
            (vec![j], vec![false]),
        ]
    };
    assert_eq!(*calls.borrow(), [each(1), each(2)].concat());
}

/// A decoder whose shots run a batch, then send another, on shots 1, 2
/// and 3: the runs go below shot by shot, and the sends after them are
/// merged into one call over the three shots.
#[test]
fn a_shots_sends_after_its_run_merge_with_the_others() {
    let run_then_send = |(): &mut (),
                         _: &CallInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         _: &mut Outcomes<'_>| {
        below.run(&[true])?;
        below.send(&[false])
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded =
        (adapter_fn(no_state, run_then_send).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut decoded, &shots(&[1, 2, 3]), &[true], "the call runs");
    let expected = [
        (vec![1], vec![true]),
        (vec![2], vec![true]),
        (vec![3], vec![true]),
        (vec![1, 2, 3], vec![false]),
    ];
    assert_eq!(*calls.borrow(), expected);
}

/// A debugger whose odd shots send a probe, then run a check, and whose
/// shots all then send a read, on shots 1, 2 and 3: a shot's sends after
/// its run rank from the run on, so the three reads merge into one call,
/// though the read is the even shot's first send and the odd shots'
/// second.
#[test]
fn a_shots_sends_after_its_run_rank_anew() {
    let probing_odd = |shot: &mut ShotId,
                       _: &CallInput<'_, bool>,
                       below: &mut Below<'_, bool>,
                       _: &mut Outcomes<'_>| {
        if !shot.is_multiple_of(2) {
            below.send(&[true])?;
            below.run(&[false])?;
        }
        below.send(&[true, true])
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded =
        (adapter_fn(its_shot, probing_odd).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut decoded, &shots(&[1, 2, 3]), &[true], "the call runs");
    let expected = [
        (vec![1], vec![true]),
        (vec![1], vec![false]),
        (vec![3], vec![true]),
        (vec![3], vec![false]),
        (vec![1, 2, 3], vec![true, true]),
    ];
    assert_eq!(*calls.borrow(), expected);
}

/// A decoder whose shots all send a read, and whose odd shots then run a
/// check, on the target chain's four shots: each odd shot's read goes
/// below alone, before its run, and the even shots' reads, kept, go below
/// as one call over 2 and 6.
#[test]
fn a_shots_run_leaves_the_other_shots_sends_merged() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let checking_odd = |shot: &mut ShotId,
                        _: &CallInput<'_, bool>,
                        below: &mut Below<'_, bool>,
                        _: &mut Outcomes<'_>| {
        below.send(&[true])?;
        if !shot.is_multiple_of(2) {
            below.run(&[false])?;
        }
        Ok(())
    };
    let mut decoded =
        (adapter_fn(its_shot, checking_odd).shots(OnePerShot) >> executor).expect("bools fit");
    step(&mut decoded, &shots(&SHOTS), &[true], "the call runs");
    let expected = [
        (vec![5], vec![true]),
        (vec![5], vec![false]),
        (vec![11], vec![true]),
        (vec![11], vec![false]),
        (vec![2, 6], vec![true]),
    ];
    assert_eq!(*calls.borrow(), expected);
}

/// Shot 5's batches fail below, and a decoder ignores its run's failure,
/// on the target chain's four shots: the call fails at shot 5, whose run
/// ended it, and the later shots do not run.
#[test]
fn a_run_that_ends_the_call_stops_the_later_shots() {
    let model = TestDoubleModel {
        failing: Some(5),
        ..TestDoubleModel::default()
    };
    let ran = Rc::new(RefCell::new(Vec::new()));
    let ignoring = {
        let ran = Rc::clone(&ran);
        move |shot: &mut ShotId,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            ran.borrow_mut().push(*shot);
            let _ = below.run(input.instructions());
            Ok(())
        }
    };
    let mut decoded =
        (adapter_fn(its_shot, ignoring).shots(OnePerShot) >> below(model)).expect("bools fit");
    let (_, end) = run(&mut decoded, &shots(&SHOTS), &[true]).expect("the call is accepted");
    assert_eq!(end, Err("lost".to_owned()));
    assert_eq!(*ran.borrow(), [2, 5], "the later shots did not run");
}

/// Shot 5's batches fail below, and a decoder sends a batch after its run,
/// on the target chain's four shots: shot 5's send is told why the call
/// cannot go on, the run's failure.
#[test]
fn a_shots_send_after_its_run_failed_gives_the_failure() {
    let model = TestDoubleModel {
        failing: Some(5),
        ..TestDoubleModel::default()
    };
    let sent = Rc::new(RefCell::new(Vec::new()));
    let run_then_send = {
        let sent = Rc::clone(&sent);
        move |shot: &mut ShotId,
              input: &CallInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            let _ = below.run(input.instructions());
            sent.borrow_mut()
                .push((*shot, below.send(input.instructions())));
            Ok(())
        }
    };
    let mut decoded =
        (adapter_fn(its_shot, run_then_send).shots(OnePerShot) >> below(model)).expect("bools fit");
    let (_, end) = run(&mut decoded, &shots(&SHOTS), &[true]).expect("the call is accepted");
    end.expect_err("shot 5's run failed");
    assert_eq!(*sent.borrow(), [(2, Ok(())), (5, Err("lost".into()))]);
}

/// A profiler with a state per shot over the device, closed: it refuses
/// every call, as the device alone does (the conformance kit's check).
#[test]
fn a_closed_profiler_refuses_every_call() {
    let mut profiled =
        (adapter_fn(no_calls, count_and_pass).shots(OnePerShot) >> device()).expect("ops fit");
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut profiled,
        InstructionBatchRef {
            shots: &shots(&[2, 5]),
            instructions: &[Op::Flip],
        },
    );
}

/// A profiler with a state per shot over the device, reporting its
/// shots' counts of calls, called on two shots: its second close reports
/// nothing, its counts included, as the device's does (the conformance
/// kit's check).
#[test]
fn a_profiler_reports_once() {
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| *calls)
        .shots(OnePerShot);
    let mut profiled = (counting >> device()).expect("ops fit");
    step(&mut profiled, &shots(&[2, 5]), &[Op::Flip], "the call runs");
    conformance::reports_once(&mut profiled);
}

/// A decoder with a state per shot and no rule for a stop, over the
/// device: a call with a stop is refused by the decoder's check, and the
/// chain runs the next call (the conformance kit's check).
#[test]
fn a_call_the_adapter_refuses_leaves_the_chain_usable() {
    let decoding = adapter_fn(no_state, decode)
        .check(decodable)
        .shots(OnePerShot);
    let mut decoded = (decoding >> device()).expect("ops fit");
    let two = shots(&[2, 5]);
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut decoded,
        InstructionBatchRef {
            shots: &two,
            instructions: &[Op::Flip, Op::Stop],
        },
        InstructionBatchRef {
            shots: &two,
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// A profiler with a state per shot over a device that holds two shots at once,
/// called on three shots: the device refuses the merged send, the profiler
/// refuses the call with that refusal, and the chain runs a call on two shots
/// (the conformance kit's check).
#[test]
fn a_call_refused_below_leaves_the_chain_usable() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass).shots(OnePerShot)
        >> device_promising(promise().with_max_shots(2)))
    .expect("ops fit");
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut profiled,
        InstructionBatchRef {
            shots: &shots(&[2, 5, 6]),
            instructions: &[Op::Flip],
        },
        InstructionBatchRef {
            shots: &shots(&[2, 5]),
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// A profiler with a state per shot over the device, whose merged send
/// stops the device: the call fails with the device's reason, and the
/// chain refuses every later call, naming it (the conformance kit's
/// check), as the device alone does.
#[test]
fn after_a_failure_below_the_chain_refuses_every_later_call() {
    let mut profiled =
        (adapter_fn(no_calls, count_and_pass).shots(OnePerShot) >> device()).expect("ops fit");
    after_a_stop(|failing, later| {
        conformance::refuses_calls_after_a_failure(&mut profiled, failing, later);
    });
}

/// A decoder with a state per shot over the device, whose first shot's
/// run stops the device: the call fails with the device's reason, and the
/// chain refuses every later call, naming it (the conformance kit's
/// check).
#[test]
fn after_a_run_failed_below_the_chain_refuses_every_later_call() {
    let mut decoded =
        (adapter_fn(no_state, decode).shots(OnePerShot) >> device()).expect("ops fit");
    after_a_stop(|failing, later| {
        conformance::refuses_calls_after_a_failure(&mut decoded, failing, later);
    });
}

/// A profiler with a state per shot over the device placed on its own
/// thread, whose merged send stops the device there: the failure comes up
/// through the placement with its reason, and the chain refuses every
/// later call, naming it (the conformance kit's check).
#[test]
fn after_a_failure_below_on_its_own_thread_the_chain_refuses_every_later_call() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass).shots(OnePerShot)
        >> device().run_on(OwnThread))
    .expect("ops fit");
    ends_soon(move || {
        after_a_stop(|failing, later| {
            conformance::refuses_calls_after_a_failure(&mut profiled, failing, later);
        });
    });
}

/// A decoder with a state per shot over the device, its check refusing a
/// tag read twice: a tag repeated within a batch is refused, and only
/// that (the conformance kit's check).
#[test]
fn a_decoder_refuses_a_tag_repeated_within_a_batch() {
    let decoding = adapter_fn(no_state, decode)
        .check(decodable)
        .shots(OnePerShot);
    conformance::refuses_a_tag_repeated_within_a_batch(
        &mut (decoding >> device()).expect("ops fit"),
        &shots(&[2, 5]),
        &Op::Read(0),
        &Op::Read(1),
        &Op::ReadInverted(0),
    );
}

/// A profiler with a state per shot over the device: a second call reads
/// the flip of the first, on each shot, as the device alone does (the
/// conformance kit's check of `resumable_shots`). Its check of `replayable`
/// runs on a drawing adapter
/// (`a_drawing_adapters_shot_draws_alone_what_it_draws_among_the_others`).
#[test]
fn a_profiler_continues_a_shot_from_one_call_to_the_next() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || (adapter_fn(no_calls, count_and_pass).shots(OnePerShot) >> device()).expect("ops fit"),
        &shots(&[2, 5]),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// A profiler with a state per shot over a simulator of the tests' set: a
/// call outside the set is refused below, at its index, and the profiler
/// refuses it with that refusal (the conformance kit's check).
#[test]
fn a_profiler_refuses_calls_outside_the_set_below() {
    let flip = flip_and_toss(&bits_set())[0][0].clone();
    let simulator = TestDoubleExecutor::with_model(Bits::default(), 7);
    conformance::refuses_calls_outside_its_set(
        &mut (adapter_fn(no_calls, count_and_pass).shots(OnePerShot) >> simulator)
            .expect("the set fits"),
        &shots(&[2, 5]),
        &bits_set(),
        &flip,
    );
}

/// An observer and a profiler with a report, each with a state per shot
/// over an executor and closed twice: the executor below each is closed
/// once, whether the adapter reports or not, as a placed executor's close
/// hands a job to its thread.
#[test]
fn a_second_close_closes_nothing_below() {
    let (counted, observed_closes) = close_counted(below(TestDoubleModel::default()));
    let mut observed =
        compose(adapter_fn(no_state, pass).shots(OnePerShot), counted).expect("bools fit");
    observed.close();
    observed.close();
    let (counted, profiled_closes) = close_counted(below(TestDoubleModel::default()));
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| *calls)
        .shots(OnePerShot);
    let mut profiled = compose(counting, counted).expect("bools fit");
    profiled.close();
    profiled.close();
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// An observer and a profiler with a report, each with a state per shot
/// over an executor whose close panics, and closed again once the panic
/// reached the caller: the executor below each is closed once, the second
/// close leaving it be.
#[test]
fn a_close_below_that_panicked_is_not_run_again() {
    let (counted, observed_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let mut observed =
        compose(adapter_fn(no_state, pass).shots(OnePerShot), counted).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the close below panics");
    observed.close();
    let (counted, profiled_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let counting = adapter_fn(no_calls, count_and_pass)
        .state_report(|calls: &u64| *calls)
        .shots(OnePerShot);
    let mut profiled = compose(counting, counted).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the close below panics");
    profiled.close();
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// A decoder with a state per shot whose logic failed on a call: closing
/// it closes the executor below all the same.
#[test]
fn a_failed_adapter_closes_what_is_below() {
    let failing = |(): &mut (),
                   _: &CallInput<'_, bool>,
                   _: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| { Err("no syndrome".into()) };
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut decoded =
        compose(adapter_fn(no_state, failing).shots(OnePerShot), counted).expect("bools fit");
    let (_, end) = run(&mut decoded, &ShotSet::one(2), &[true]).expect("the call is accepted");
    end.expect_err("the logic failed");
    decoded.close();
    assert_eq!(closes.get(), 1);
}

/// A logic with a state per shot and a bug, that panicked on shot 5:
/// closing the adapter closes the executor below all the same.
#[test]
fn an_adapter_whose_logic_panicked_closes_what_is_below() {
    let buggy = |shot: &mut ShotId,
                 input: &CallInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 outcomes: &mut Outcomes<'_>| {
        assert!(*shot != 5, "the logic has a bug");
        pass(shot, input, below, outcomes)
    };
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut executor =
        compose(adapter_fn(its_shot, buggy).shots(OnePerShot), counted).expect("bools fit");
    panic_of(|| {
        let _ = run(&mut executor, &shots(&SHOTS), &[true]);
    });
    executor.close();
    assert_eq!(closes.get(), 1);
}

/// A profiler with a state per shot whose logic panics on shot 5 of a
/// call on shots 2, 5 and 6, after shot 2 counted it, shot 2 having run
/// alone before: the report holds shot 2's state as it was before that
/// call, and no state made for it.
#[test]
fn a_call_that_panicked_reports_no_state_made_for_it() {
    let panicking_on_five = |(shot, calls): &mut (ShotId, u64),
                             input: &CallInput<'_, bool>,
                             below: &mut Below<'_, bool>,
                             _: &mut Outcomes<'_>| {
        *calls += 1;
        assert!(*shot != 5, "the logic has a bug");
        below.send(input.instructions())
    };
    let counting = adapter_fn(
        |context: ShotContext| Ok((context.shot(), 0)),
        panicking_on_five,
    )
    .state_report(|(_, calls): &(ShotId, u64)| *calls)
    .shots(OnePerShot);
    let mut profiled = (counting >> below(TestDoubleModel::default())).expect("bools fit");
    step(
        &mut profiled,
        &ShotSet::one(2),
        &[true],
        "shot 2 runs alone",
    );
    panic_of(|| {
        let _ = run(&mut profiled, &shots(&[2, 5, 6]), &[true]);
    });
    let (calls, _) = profiled.close();
    assert_eq!(calls, 2, "shot 2's two calls, and no other state");
}

/// Shot 2's state below failed in a call whose reply did not show it: a
/// call on shot 2 is refused for that failure, and the adapter fails with
/// it, so a later call on shot 5 that would not go down is refused too.
#[test]
fn a_refusal_below_for_an_earlier_failure_fails_the_adapter() {
    let model = TestDoubleModel {
        failed_unseen: Some(2),
        ..TestDoubleModel::default()
    };
    let mut observed = (adapter_fn(no_state, pass_unless_empty).shots(OnePerShot) >> below(model))
        .expect("bools fit");
    let refused = run(&mut observed, &ShotSet::one(2), &[true]).expect_err("shot 2 failed");
    assert_eq!(refused, Rejected::EarlierFailure("lost".into()));
    let later = run(&mut observed, &ShotSet::one(5), &[]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("lost".into()));
}

/// A profiler with a state per shot composed over the device after the
/// device stopped: its first call is refused with the device's failure,
/// as the device alone refuses it.
#[test]
fn an_adapter_over_a_failed_executor_refuses_with_its_failure() {
    let mut device = device();
    let (_, end) = run(&mut device, &ShotSet::one(2), &[Op::Stop]).expect("the stop is sent");
    let reason = end.expect_err("the device stopped");
    let mut profiled =
        (adapter_fn(no_calls, count_and_pass).shots(OnePerShot) >> device).expect("ops fit");
    let refused =
        run(&mut profiled, &ShotSet::one(5), &[Op::Flip]).expect_err("the device below stopped");
    assert_eq!(refused, Rejected::EarlierFailure(reason));
}

/// An observer with a state per shot, with calls in flight on shots 2 and
/// 5: shot 2's fails below, and once its reply is seen failed, shot 5's,
/// still open below though it has a readout, ends at its next poll with
/// that failure and none of its events, rather than waiting on.
#[test]
fn a_call_in_flight_ends_at_an_earlier_failure_below() {
    let senders = Senders::default();
    let mut observed = compose(
        adapter_fn(no_state, pass).shots(OnePerShot),
        HeldOpen(Rc::clone(&senders)),
    )
    .expect("bools fit");
    let mut call = |j| {
        observed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor below accepts every call")
    };
    let (failing, mut waiting) = (call(2), call(5));
    let first = senders.borrow_mut().remove(0);
    first.fail("lost".into());
    senders.borrow()[0].push(one(5, 0));
    wait(failing, &mut Vec::new()).expect_err("shot 2 failed below");
    let mut events = Vec::new();
    let status = waiting.poll_events(&mut events, Waker::noop());
    assert_eq!(
        (status, events),
        (
            ReplyStatus::Failed("an earlier call failed: lost".into()),
            vec![]
        )
    );
}

/// Shot 2's batches fail below as they are called, and a decoder on the
/// target chain's shots sends, then runs, ignoring both errors: shot 2's
/// send goes below alone before its run and fails there, so its run does
/// not go down, and nothing more does.
#[test]
fn nothing_goes_down_after_a_shots_send_whose_reply_failed() {
    let model = TestDoubleModel {
        failing: Some(2),
        ..TestDoubleModel::default()
    };
    let ignoring = |_: &mut ShotId,
                    input: &CallInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        let _ = below.send(input.instructions());
        let _ = below.run(input.instructions());
        Ok(())
    };
    let (executor, calls) = recorded(below(model));
    let mut decoded =
        (adapter_fn(its_shot, ignoring).shots(OnePerShot) >> executor).expect("bools fit");
    let (_, end) = run(&mut decoded, &shots(&SHOTS), &[true]).expect("the call is accepted");
    assert_eq!(end, Err("lost".to_owned()));
    assert_eq!(
        *calls.borrow(),
        [(vec![2], vec![true])],
        "shot 2's send alone"
    );
}

/// Shot 2's batches fail below as they are called, and a profiler on the
/// target chain's shots sends each shot's batch, then a mark: the merged
/// batch fails as it goes down, so the merged marks do not go down.
#[test]
fn nothing_goes_down_after_a_merged_send_whose_reply_failed() {
    let model = TestDoubleModel {
        failing: Some(2),
        ..TestDoubleModel::default()
    };
    let marking = |_: &mut ShotId,
                   input: &CallInput<'_, bool>,
                   below: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        below.send(&[false])
    };
    let (executor, calls) = recorded(below(model));
    let mut profiled =
        (adapter_fn(its_shot, marking).shots(OnePerShot) >> executor).expect("bools fit");
    let (_, end) = run(&mut profiled, &shots(&SHOTS), &[true]).expect("the call is accepted");
    assert_eq!(
        *calls.borrow(),
        [(SHOTS.to_vec(), vec![true])],
        "the merged batch alone"
    );
    assert_eq!(end, Err("lost".to_owned()), "the merged batch's failure");
}
