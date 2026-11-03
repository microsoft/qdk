//! The Qodec chains over a backend that answers later (its replies ready
//! only at a later poll), against the same chains over a backend that
//! answers within its call; how many shots are in flight below a Qodec
//! layer; a program holding several calls in flight; and whether a run
//! awaited yields while a reply below is pending, over a backend answered
//! from another thread once the run yields.
//!
//! The Qodec runtime calls its backend and waits (QD1, mode (c)), so its
//! layer serves a call's shots one by one, each read below on its shot
//! alone, and the thread that serves it waits for each read's reply below:
//! an upper call whose lowering reads nothing goes below on its shots at
//! once, but each batch that reads goes below on its shot alone
//! (`tests/calls_below.rs`). A run awaited by a program that waits for each
//! call yields while that call's sends below are pending, but holds the
//! thread that polls it through each read below; a send still pending when
//! a later read parks is held too. A caller frees its thread by building
//! and running the chain on a thread of its own. Over the profiler alone,
//! no reply below is held (`tests/profiler.rs`). The runs over a backend
//! answered once the run yields are bounded in time, so that a lost wake,
//! which hangs a run over a Qodec layer, fails the test.

mod common;

use std::future::{Future, IntoFuture};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use executor_protocol::{
    Driver, DriverExt, Executor, ExecutorReport, InstructionBatch, InstructionBatchRef,
    InstructionCall, Needs, Operand, OwnThread, ShotId, ShotOutcome, ShotSet, StaticProgram, Value,
    static_driver, wait_async,
};
use qodec_demo::answers_later::{AnswersLater, InFlight};
use qodec_demo::answers_on_yield::{AnswersOnYield, YieldGate};
use qodec_demo::call_counter::{Calls, call_counter};
use qodec_demo::coin::{coin_device, coin_device_on_threads};
use qodec_demo::noisy_bits::simulator;
use qodec_demo::profiler::{Profile, profiler};
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::{QodecLayer, Statuses};
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::on_the_stack;

use common::waiting::bounded;
use common::{
    CODE_SHOTS, FLIP, SEED, SHOTS, bottom, c4c6, c4c6_program, repetition3, repetition3_program,
    shot_set, sorted, stim_rz,
};

/// The primary chain on c4c6, over `$device`: both layers, then the
/// profiler, then the device; its sorted outcomes and the profile.
macro_rules! primary_chain {
    ($qodec:expr, $device:expr) => {{
        let qodec = &$qodec;
        let top = QodecLayer::new(qodec, 0).expect("c4c6 plans");
        let middle = QodecLayer::new(qodec, 1).expect("c4c6 plans");
        let chain = (static_driver() >> top.adapter() >> middle.adapter() >> profiler() >> $device)
            .expect("every boundary matches");
        let program = c4c6_program(qodec).to_static(Needs::default());
        let shots: Vec<_> = CODE_SHOTS.collect();
        let (outcomes, (_, (_, (profile, _)))): (_, (Statuses, (Statuses, (Profile, _)))) = chain
            .run(program, shot_set(&shots))
            .expect("the job fits the chain")
            .wait();
        let profile: Profile = profile;
        (sorted(outcomes.expect("the run succeeds")), profile)
    }};
}

/// Repetition3 over the noisy bits answering later: its outcomes, its
/// layer's statuses and the bits' report, and the shots in flight below
/// the layer.
fn repetition3_answering_later() -> (
    Vec<ShotOutcome>,
    Statuses,
    ExecutorReport<()>,
    Arc<InFlight>,
) {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let in_flight = Arc::new(InFlight::default());
    let bits = AnswersLater::new(simulator(stim_rz().set, SEED, FLIP), Arc::clone(&in_flight));
    let chain = (static_driver() >> layer.adapter() >> bits).expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (outcomes, (statuses, report)) = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    let outcomes = outcomes.expect("the run succeeds");
    (outcomes, statuses, report, in_flight)
}

/// Repetition3's run is the same whether the bits answer within their call
/// or later: the outcomes, the layer's statuses and each shot's log
/// probability.
#[test]
fn repetition3_gives_the_same_run_over_a_backend_that_answers_later() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let chain = (static_driver() >> layer.adapter() >> simulator(stim_rz().set, SEED, FLIP))
        .expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (within, (within_statuses, within_report)) = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    let (later, later_statuses, later_report, _) = repetition3_answering_later();

    assert_eq!(sorted(later), sorted(within.expect("the run succeeds")));
    assert_eq!(later_statuses, within_statuses);
    assert_eq!(later_report.log_p, within_report.log_p);
}

/// The chain `static_driver() >> qodec >> profiler() >> backend`, on c4c6:
/// the same outcomes, and the same counts, whether the coins answer within
/// their call or later.
#[test]
fn the_profiled_chain_gives_the_same_run_over_a_backend_that_answers_later() {
    let qodec = c4c6();
    let coins = || coin_device(bottom(&qodec).set.clone(), SEED);
    let within = primary_chain!(qodec, coins());
    let later = primary_chain!(qodec, AnswersLater::new(coins(), Arc::default()));

    assert!(within.1.instructions() > 0, "the profiler counted");
    assert_eq!(later, within);
}

/// The chain `static_driver() >> qodec >> profiler() >> backend`, on c4c6:
/// the same outcomes, and the same counts, whether the coins answer within
/// their call or on a thread per shot, each shot's state there.
#[test]
fn the_profiled_chain_gives_the_same_run_over_a_thread_per_shot() {
    let qodec = c4c6();
    let set = bottom(&qodec).set.clone();
    let within = primary_chain!(qodec, coin_device(Arc::clone(&set), SEED));
    let on_threads = primary_chain!(qodec, coin_device_on_threads(set, SEED));

    assert!(within.1.instructions() > 0, "the profiler counted");
    assert_eq!(on_threads, within);
}

/// The chain `static_driver() >> qodec >> profiler() >> backend`, on c4c6:
/// the same outcomes, and the same counts, whether the coins answer within
/// their call or on one thread of their own, every shot's state there.
#[test]
fn the_profiled_chain_gives_the_same_run_over_coins_on_a_thread_of_their_own() {
    let qodec = c4c6();
    let set = bottom(&qodec).set.clone();
    let within = primary_chain!(qodec, coin_device(Arc::clone(&set), SEED));
    let placed = primary_chain!(qodec, coin_device(set, SEED).run_on(OwnThread));

    assert!(within.1.instructions() > 0, "the profiler counted");
    assert_eq!(placed, within);
}

/// Over coins that answer later, each shot runs the bottom batches of the
/// Qodec runtime's own stack, in the stack's order and operand for operand:
/// a later batch runs after the feed-forward of the batch before, whose
/// corrections (`Z_if`) carry the bits read below (QD3).
#[test]
fn over_a_backend_that_answers_later_each_shot_runs_the_stacks_batches_in_order() {
    let qodec = c4c6();
    let shots: Vec<_> = CODE_SHOTS.collect();
    let program = c4c6_program(&qodec);
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = AnswersLater::new(
        coin_device(bottom(&qodec).set.clone(), SEED),
        Arc::default(),
    );
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> recorder() >> coins)
        .expect("every boundary matches");
    let (outcomes, (_, (_, (sent, _)))): (_, (Statuses, (Statuses, (Sent, _)))) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    let expected = (stack.backends.iter())
        .map(|(shot, coins)| (*shot, coins.sent().to_vec()))
        .collect();
    let instructions = |s: &Sent| s.0.values().flatten().flatten().count();
    assert!(instructions(&sent) > 0, "the shots ran");
    assert_eq!(sent, Sent(expected));
}

/// The physical program `R 0; M 0` on [`SHOTS`], straight over coins that
/// answer later: the shots in flight, and the polls at which the run,
/// awaited, was pending.
fn coins_answering_later_alone() -> (Arc<InFlight>, usize) {
    let set = stim_rz().set;
    let call = |m, operands: &[Operand]| set.call(m, operands.iter().copied()).expect("stim+rz");
    let batch = vec![
        call("R", &[Operand::Block(0)]),
        call("M", &[Operand::Block(0), Operand::Tag(0)]),
    ];
    let needs = Needs::default().with_instruction_set(Some(Arc::clone(&set)));
    let program = StaticProgram::new(needs, vec![batch]);
    let in_flight = Arc::new(InFlight::default());
    let coins = AnswersLater::new(coin_device(set, SEED), Arc::clone(&in_flight));
    let chain = (static_driver() >> coins).expect("the boundary matches");
    let run = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain");
    let (outcomes, pending) = awaited(run);
    let (outcomes, _) = outcomes;
    assert_eq!(
        outcomes.expect("the run succeeds").len(),
        SHOTS.len(),
        "M reads"
    );
    (in_flight, pending)
}

/// `run`, awaited by polling it until it is ready: its output, and how many
/// polls were pending.
fn awaited<F: IntoFuture>(run: F) -> (F::Output, usize) {
    let mut run = pin!(run.into_future());
    let mut context = Context::from_waker(Waker::noop());
    let mut pending = 0;
    loop {
        match run.as_mut().poll(&mut context) {
            Poll::Ready(output) => return (output, pending),
            Poll::Pending => pending += 1,
        }
    }
}

/// A program's batches, each called before any reply is awaited, then
/// awaited from the last: a program holding all its calls in flight at
/// once, whose replies end in any order.
struct AllCallsFirst(Program);

impl executor_protocol::Program for AllCallsFirst {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::clone(&self.0.top.set)))
    }
}

/// The driver of an [`AllCallsFirst`]: calls every batch on every shot,
/// then awaits the replies from the last.
struct AllCallsFirstDriver;

impl Driver<AllCallsFirst> for AllCallsFirstDriver {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: AllCallsFirst,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let replies: Vec<_> = (program.0.batches.iter())
            .map(|instructions| {
                let batch = InstructionBatchRef {
                    shots,
                    instructions,
                };
                executor.call(batch).expect("the chain takes the batch")
            })
            .collect();
        let mut outcomes = Vec::new();
        for reply in replies.into_iter().rev() {
            let (events, end) = wait_async(reply).await;
            end.expect("the chain runs the batch");
            outcomes.extend(events);
        }
        outcomes
    }
}

/// A program may hold several calls in flight on one executor: over coins
/// that answer later, both calls' shots are in flight at once.
#[test]
fn a_program_holds_both_calls_shots_in_flight_at_once() {
    let top = Arc::new(stim_rz());
    let call =
        |m, operands: &[Operand]| (top.set.call(m, operands.iter().copied())).expect("stim+rz");
    let batch = vec![
        call("R", &[Operand::Block(0)]),
        call("M", &[Operand::Block(0), Operand::Tag(0)]),
    ];
    let set = Arc::clone(&top.set);
    let in_flight = Arc::new(InFlight::default());
    let coins = AnswersLater::new(coin_device(set, SEED), Arc::clone(&in_flight));
    let chain = (AllCallsFirstDriver.into_chain() >> coins).expect("the boundary matches");
    let program = Program {
        top,
        batches: vec![batch.clone(), batch],
    };
    let (outcomes, _) = (chain.run(AllCallsFirst(program), shot_set(&SHOTS)))
        .expect("the job fits the chain")
        .wait();

    assert_eq!(outcomes.len(), 2 * SHOTS.len(), "each M reads");
    assert_eq!(in_flight.most(), 2 * SHOTS.len());
}

/// A program holding all its calls in flight over the Qodec layers, over
/// coins that answer later, and waiting for the last first, has each shot
/// run the Qodec runtime's own stack's bottom batches, in order
/// (QD3): each later call on a shot runs after the earlier
/// call's feed-forward, since each layer serves a call within it (QD1,
/// mode (c)).
#[test]
fn a_program_holding_all_its_calls_runs_the_stacks_batches_in_order() {
    let qodec = c4c6();
    let shots: Vec<_> = CODE_SHOTS.collect();
    let program = c4c6_program(&qodec);
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = AnswersLater::new(
        coin_device(bottom(&qodec).set.clone(), SEED),
        Arc::default(),
    );
    let chain = (AllCallsFirstDriver.into_chain()
        >> top.adapter()
        >> middle.adapter()
        >> recorder()
        >> coins)
        .expect("every boundary matches");
    let (outcomes, (_, (_, (sent, _)))): (_, (Statuses, (Statuses, (Sent, _)))) = chain
        .run(AllCallsFirst(program), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();

    let expected = (stack.backends.iter())
        .map(|(shot, coins)| (*shot, coins.sent().to_vec()))
        .collect();
    assert_eq!(
        sorted(outcomes),
        sorted(stack.outcomes),
        "precondition: the run gives the stack's outcomes"
    );
    assert_eq!(sent, Sent(expected));
}

/// With no Qodec layer, a call's shots are read below at once: the
/// interpreter calls the coins once on all of [`SHOTS`].
#[test]
fn without_a_qodec_layer_a_calls_shots_are_read_at_once() {
    let (in_flight, _) = coins_answering_later_alone();
    assert_eq!(in_flight.widest_read(), SHOTS.len());
}

/// Below a Qodec layer, an upper call whose lowering reads nothing goes
/// below on its shots at once: each shot's runtime sends its batches
/// without waiting for a reply, so the shots' sends are in flight
/// together. Repetition3's first call, a prepare, is one such.
#[test]
fn below_a_qodec_layer_a_calls_shots_are_in_flight_at_once() {
    let (.., in_flight) = repetition3_answering_later();
    assert_eq!(in_flight.most(), SHOTS.len());
}

/// Below a Qodec layer, each read is on one shot (QD1, mode (c)): the layer
/// serves a call's shots one by one, and each shot's runtime waits for
/// each read below before the next shot's runtime starts.
#[test]
fn below_a_qodec_layer_each_read_is_on_one_shot() {
    let (.., in_flight) = repetition3_answering_later();
    assert_eq!(in_flight.widest_read(), 1);
}

/// FINDING (QD2, Q67): what a backend that answers later can take, a
/// call's reads merged across its shots, which repetition3 allows (its
/// batches that read are equal on every shot). A runtime that can be
/// resumed or awaited (QD1 modes (a), (b)) alone leaves a shot's read on
/// that shot alone: merging a shot's batches that read with the other
/// shots' needs a lockstep per-shot facility whose shots' logic can stop
/// at a read: async, resumable, or a thread per shot (open, Q67;
/// `tests/calls_below.rs`).
#[test]
#[ignore = "finding QD2 (Q67): below the Qodec layer, each read is on one shot"]
fn below_a_qodec_layer_a_calls_shots_are_read_at_once() {
    let (.., in_flight) = repetition3_answering_later();
    assert_eq!(in_flight.widest_read(), SHOTS.len());
}

/// With no Qodec layer, the run, awaited, yields while the coins have not
/// answered: its poll is pending, and the caller's thread is free.
#[test]
fn awaited_without_a_qodec_layer_the_run_yields_while_the_backend_answers_later() {
    let (_, pending) = coins_answering_later_alone();
    assert!(pending > 0);
}

/// How long a reply below waits for the run to yield before it is answered
/// anyway: a run that yields answers it at its next yield, well before.
const YIELD_TIMEOUT: Duration = Duration::from_millis(50);

/// `program` of repetition3 on [`SHOTS`] over a Qodec layer and noisy bits
/// answered once the run yields, awaited: the bits' gate, once the run
/// succeeded. It is bounded in time, since a wake lost below the layer
/// hangs the run's poll ([`AnswersOnYield`]).
fn repetition3_over_bits_answered_on_yield(
    program: fn(&qodec::Qodec) -> Program,
) -> Arc<YieldGate> {
    bounded(move || {
        let qodec = repetition3();
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let gate = YieldGate::new(YIELD_TIMEOUT);
        let bits = AnswersOnYield::new(simulator(stim_rz().set, SEED, FLIP), Arc::clone(&gate));
        let chain = (static_driver() >> layer.adapter() >> bits).expect("every boundary matches");
        let program = program(&qodec).to_static(Needs::default());
        let run = chain
            .run(program, shot_set(&SHOTS))
            .expect("the job fits the chain");
        let (outcomes, _) = gate.await_run(run);
        outcomes.expect("the run succeeds");
        gate
    })
}

/// Over a Qodec layer, the run, awaited by a program that waits for each
/// call, holds the thread that polls it through each read below (QD1,
/// mode (c)): the layer's runtime waits for the read's reply inside the
/// call, so the run cannot yield to let a remote device answer it.
/// Repetition3 on 4 shots reads twice a shot: each of the 8 reads waits
/// for its timeout, and only they do, the shots' merged prepare being
/// answered by a yield
/// (`awaited_over_a_qodec_layer_the_run_yields_while_a_send_below_is_pending`).
#[test]
fn awaited_over_a_qodec_layer_the_run_holds_its_thread_through_each_read_below() {
    let gate = repetition3_over_bits_answered_on_yield(repetition3_program);
    assert_eq!(
        gate.replies(),
        2 * SHOTS.len() + 1,
        "precondition: the 8 reads and the merged prepare went below"
    );
    assert_eq!(gate.timed_out(), 2 * SHOTS.len(), "each read waited");
}

/// Over a Qodec layer, the run, awaited, yields while a call that reads
/// nothing is pending below, so a remote device answers it with the
/// thread that polls the run free: repetition3's prepare on 4 shots, alone
/// in its program, goes below as one call, answered by a yield.
#[test]
fn awaited_over_a_qodec_layer_the_run_yields_while_a_send_below_is_pending() {
    let gate =
        repetition3_over_bits_answered_on_yield(|qodec| Program::parse(qodec, &["prepare_z 0"]));
    assert_eq!(
        gate.replies(),
        1,
        "precondition: the prepare went below once"
    );
    assert_eq!(gate.timed_out(), 0);
}

/// Over a Qodec layer, the run, awaited by a program holding all its calls
/// in flight, holds the thread that polls it through a send still pending
/// when a later call's read parks: the read waits inside the call, before
/// the run can yield. Repetition3 on 4 shots: the shots' merged prepare
/// times out as the 8 reads do.
#[test]
fn awaited_over_a_qodec_layer_a_send_pending_when_a_later_read_parks_is_held_too() {
    let gate = bounded(|| {
        let qodec = repetition3();
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let gate = YieldGate::new(YIELD_TIMEOUT);
        let bits = AnswersOnYield::new(simulator(stim_rz().set, SEED, FLIP), Arc::clone(&gate));
        let chain = (AllCallsFirstDriver.into_chain() >> layer.adapter() >> bits)
            .expect("every boundary matches");
        let program = AllCallsFirst(repetition3_program(&qodec));
        let run = chain
            .run(program, shot_set(&SHOTS))
            .expect("the job fits the chain");
        gate.await_run(run);
        gate
    });
    assert_eq!(gate.timed_out(), 2 * SHOTS.len() + 1);
}

/// c4c6's primary chain over coins, built and run where this is called:
/// its sorted outcomes and the calls the coins got.
fn c4c6_calls_here() -> (Vec<(ShotId, u32, Value)>, Calls) {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> call_counter() >> coins)
        .expect("every boundary matches");
    let program = c4c6_program(&qodec).to_static(Needs::default());
    let shots: Vec<_> = CODE_SHOTS.collect();
    let (outcomes, (_, (_, (calls, _)))): (_, (Statuses, (Statuses, (Calls, _)))) = chain
        .run(program, shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    (sorted(outcomes.expect("the run succeeds")), calls)
}

/// A caller that must not block, as an async one, runs c4c6's chain on a
/// thread of its own, built there, since a Qodec chain cannot move to
/// another thread: the run is the one it gives on the caller's thread,
/// outcome for outcome and call for call below.
#[test]
fn a_qodec_chain_built_and_run_on_a_thread_of_its_own_gives_the_same_run() {
    let on_its_thread = std::thread::spawn(c4c6_calls_here)
        .join()
        .expect("the chain's thread ends");
    let here = c4c6_calls_here();

    assert_eq!(
        here.1,
        Calls(276),
        "precondition: the device was called on each bottom batch"
    );
    assert_eq!(on_its_thread, here);
}

/// FINDING (QD1): what an async caller needs of a chain with a Qodec
/// layer, which waits for each read's reply below by parking the thread
/// that polls the run: no reply below waits on that thread, as over the
/// profiler alone. It needs the runtime resumable (QD1 mode (a)) or
/// awaitable (QD1 mode (b)).
#[test]
#[ignore = "finding QD1: over the Qodec layer, each read below waits on the thread that polls the run"]
fn awaited_over_a_qodec_layer_no_reply_below_waits_on_the_thread_that_polls_the_run() {
    let gate = repetition3_over_bits_answered_on_yield(repetition3_program);
    assert_eq!(gate.timed_out(), 0);
}
