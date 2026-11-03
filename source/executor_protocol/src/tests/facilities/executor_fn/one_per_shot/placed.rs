// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::Cell;
use std::mem;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::task::Waker;
use std::thread;
use std::time::{Duration, Instant};

use crate::tests::Count;
use crate::tests::facilities::executor_fn::{
    Connection, Flipped, Flips, Freed, Op as DrawingOp, PowerDownFailed, PowersDownBadly,
    ReleasesBadly, connected_device, count_flips, draw_value, flips, freed_so_far,
    holding_a_handle, narrow_promise, panic_text, panics, powering_down_badly, read_drawn_bits,
    runs, stops,
};
use crate::tests::facilities::placed_executor::{
    Bit, BitLogP, CopyPanics, Op, PanicsWhenWoken, RunBit, Threads, dropped_while_a_call_runs,
    ended, gated_device, holding, own_bit, polled_now, recording, reported_on, run_bit, seen, send,
    values,
};
use crate::tests::{PATIENT, block_on, ends_soon};
use crate::{
    CallInput, Capabilities, Close, Describe, Executor, ExecutorPlacement, ExecutorReport,
    InstructionBatch, InstructionBatchRef, LogP, OnePerShot, Outcome, Outcomes, OwnThread,
    PlacementJob, Rejected, Reply, ReplyStatus, ShotContext, ShotId, ShotOutcome, ShotSet, Value,
    check_unique_tags, conformance, executor_fn, wait, wait_async,
};

/// The run's seed in these tests.
const SEED: u64 = 7;

/// The state closure of the tests' device: a bit, zero when made, whose
/// logic records its thread in `threads`.
fn bits(threads: &Threads) -> impl FnMut(ShotContext) -> Result<Bit, Rejected> + Send + use<> {
    let threads = Arc::clone(threads);
    move |_| {
        Ok(Bit {
            one: false,
            threads: Arc::clone(&threads),
        })
    }
}

/// The log probability of the tests' device: the bit's value.
fn bit_value(bit: &Bit) -> f64 {
    f64::from(u8::from(bit.one))
}

/// The `(shot, tag, value)` of `outcomes`, by shot, each shot's in the
/// order it gave them: shots on placements of their own interleave.
fn by_shot(outcomes: &[ShotOutcome]) -> Vec<(ShotId, u32, Value)> {
    let mut values = values(outcomes);
    values.sort_by_key(|&(shot, _, _)| shot);
    values
}

/// A simulator of a whole run whose shots run in parallel,
/// `.run_on(OwnThread).shots(..)`: the placement is applied first, inside, so
/// each shot runs on a thread of its own, the same for its later calls,
/// and none on the caller's.
#[test]
fn placed_before_one_per_shot_each_shot_runs_on_a_thread_of_its_own() {
    let threads = Threads::default();
    let mut per_shot = recording(&threads).run_on(OwnThread).shots(OnePerShot);
    for shot in [0, 1, 0] {
        let (_, end) =
            send(&mut per_shot, &[shot], &[Op::Flip]).expect("an open executor runs a call");
        end.expect("the logic does not fail");
    }
    let seen = seen(&threads);
    assert_eq!(seen.len(), 3, "one logic run per call");
    assert_eq!(seen[0], seen[2], "shot 0's calls on its thread");
    assert_ne!(seen[0], seen[1], "shots 0 and 1 on threads of their own");
    assert!(!seen.contains(&thread::current().id()));
}

/// A logic that counts its calls in a `Cell`, as code on one thread does,
/// given `.run_on(OwnThread).shots(OnePerShot)`: each shot runs on a copy
/// of its own, sent to the shot's thread and never shared between
/// threads, so the logic need not be `Sync`. Each copy counts its own
/// shot's calls.
#[test]
fn placed_before_one_per_shot_takes_a_logic_that_cannot_be_shared_between_threads() {
    let calls = Cell::new(0_u32);
    let logic = move |_: &mut Bit, _: &CallInput<'_, Op>, outcomes: &mut Outcomes<'_>| {
        calls.set(calls.get() + 1);
        outcomes.push(Outcome::new(calls.get(), Value::One));
        Ok(())
    };
    let threads = Threads::default();
    let mut per_shot = executor_fn(Capabilities::default(), bits(&threads), logic)
        .run_on(OwnThread)
        .shots(OnePerShot);
    let mut tags = Vec::new();
    for shot in [0, 1, 0] {
        let (outcomes, end) =
            send(&mut per_shot, &[shot], &[Op::Flip]).expect("an open executor runs a call");
        end.expect("the logic does not fail");
        tags.extend(values(&outcomes));
    }
    assert_eq!(
        tags,
        [(0, 1, Value::One), (1, 1, Value::One), (0, 2, Value::One)]
    );
}

/// A simulator seeded with 7: shot `j`'s state is made from the run's
/// seed and the shot, on the caller's thread, before it moves to its
/// placement.
#[test]
fn each_shots_state_is_made_on_the_callers_thread_from_the_runs_seed_and_the_shot() {
    let made = Arc::new(Mutex::new(Vec::new()));
    let threads = Threads::default();
    let mut bit = bits(&threads);
    let record = Arc::clone(&made);
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            let mut made = record.lock().expect("no test thread panics holding it");
            made.push((thread::current().id(), context));
            bit(context)
        },
        run_bit as RunBit,
    )
    .seed(SEED)
    .run_on(OwnThread)
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[2, 5], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let (_, end) = send(&mut per_shot, &[5], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let caller = thread::current().id();
    assert_eq!(
        *made.lock().expect("no test thread panics holding it"),
        [
            (caller, ShotContext::last_executor(SEED, 2)),
            (caller, ShotContext::last_executor(SEED, 5))
        ],
        "made once per shot, at its first call"
    );
}

/// The two orders of `.shots` and `.run_on` change where the shots run, not
/// what they give: a run of three calls on overlapping shots gives each shot
/// the same outcomes, in the same order, and the same report.
#[test]
fn both_orders_of_the_wrappers_give_the_same_outcomes_and_report() {
    let calls: [(&[ShotId], &[Op]); 3] = [
        (&[0, 1, 2], &[Op::Flip, Op::Read(0), Op::Flip, Op::Read(1)]),
        (&[2, 3], &[Op::Flip, Op::Read(2)]),
        (&[3, 0], &[Op::Read(3)]),
    ];
    let threads = Threads::default();
    let mut on_one_thread = recording(&threads).shots(OnePerShot).run_on(OwnThread);
    let mut per_shot = recording(&threads).run_on(OwnThread).shots(OnePerShot);
    for (shots, ops) in calls {
        let (expected, end) =
            send(&mut on_one_thread, shots, ops).expect("an open executor runs a call");
        end.expect("the logic does not fail");
        let (outcomes, end) =
            send(&mut per_shot, shots, ops).expect("an open executor runs a call");
        end.expect("the logic does not fail");
        assert_eq!(by_shot(&outcomes), by_shot(&expected));
    }
    let expected = ends_soon(move || on_one_thread.close());
    assert_eq!(ends_soon(move || per_shot.close()), expected);
}

/// The states' report of `executor` after it ran `calls`, each waited
/// for; the last may fail.
fn states_report(
    mut executor: impl Executor<InstructionBatch<DrawingOp>>
    + Close<Report = ExecutorReport<Flips>>
    + Send
    + 'static,
    calls: &'static [(&'static [ShotId], &'static [DrawingOp])],
) -> Flips {
    ends_soon(move || {
        for &(shots, ops) in calls {
            let shots = ShotSet::new(shots.to_vec()).expect("the tests name each shot once");
            let reply = executor
                .call(InstructionBatchRef {
                    shots: &shots,
                    instructions: ops,
                })
                .expect("an open executor runs a call");
            let _ = wait(reply, &mut Vec::new());
        }
        executor.close().states
    })
}

/// A simulator whose states count their flips, run on overlapping shots
/// until it stops on shot 1: its states' report is the same whether its
/// shots run on the caller's thread, on one thread, or each on a thread
/// of its own, and holds shot 1's state as the failed call left it.
#[test]
fn every_form_reports_the_same_states() {
    use DrawingOp::{Flip, Stop};
    const CALLS: &[(&[ShotId], &[DrawingOp])] = &[
        (&[0, 1, 2], &[Flip]),
        (&[2, 3], &[Flip, Flip]),
        (&[3, 0], &[Flip]),
        (&[1], &[Flip, Stop]),
    ];
    let simulator = || executor_fn(Capabilities::default(), Flipped::new, count_flips);
    let expected = Flips::of([(0, 2), (1, 2), (2, 3), (3, 3)]);
    let on_the_callers_thread = simulator().state_report(flips).shots(OnePerShot);
    assert_eq!(states_report(on_the_callers_thread, CALLS), expected);
    let on_one_thread = (simulator().state_report(flips))
        .shots(OnePerShot)
        .run_on(OwnThread);
    assert_eq!(states_report(on_one_thread, CALLS), expected);
    let per_shot = (simulator().state_report(flips))
        .run_on(OwnThread)
        .shots(OnePerShot);
    assert_eq!(states_report(per_shot, CALLS), expected);
}

/// A simulator whose states count their flips, and whose logic has a
/// bug on shot 1's second call, runs shots 0 and 1 twice: the panic stops
/// shot 1's placement, so its state, made by the first call, gives neither
/// its log probability nor its report at close, while shot 0's gives both,
/// as its two calls left it.
#[test]
fn a_state_whose_logic_panicked_is_left_out_of_the_report() {
    let mut per_shot = executor_fn(
        Capabilities::default(),
        Flipped::new,
        |state: &mut Flipped, input: &CallInput<'_, DrawingOp>, out: &mut Outcomes<'_>| {
            let second = state.flips > 0;
            assert!(
                !(state.shot == 1 && second),
                "the logic has a bug on shot 1's second call"
            );
            count_flips(state, input, out)
        },
    )
    .log_probability(|state: &Flipped| f64::from(state.flips))
    .state_report(flips)
    .run_on(OwnThread)
    .shots(OnePerShot);
    let shots = ShotSet::new(vec![0, 1]).expect("two shots");
    let mut flip = || {
        let reply = per_shot
            .call(InstructionBatchRef {
                shots: &shots,
                instructions: &[DrawingOp::Flip],
            })
            .expect("an open executor queues a call");
        ended(reply).1
    };
    flip().expect("the first call runs");
    flip().expect_err("the logic panicked on shot 1");
    let report = ends_soon(move || per_shot.close());
    let mut log_p = LogP::default();
    log_p.add(0, 2.0);
    assert_eq!(report.log_p, log_p);
    assert_eq!(report.states, Flips::of([(0, 2)]));
}

/// The logic of [`Flipped`] on a device that stops on shots 0 and 3,
/// after their flips, and only there.
fn stop_on_shots_0_and_3(
    state: &mut Flipped,
    input: &CallInput<'_, DrawingOp>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    count_flips(state, input, outcomes)?;
    if state.shot == 0 || state.shot == 3 {
        return Err(format!("the device stopped on shot {}", state.shot));
    }
    Ok(())
}

/// A failed call reports no state made for it, whatever the order of the
/// wrappers: a device that counts its flips runs shot 2, then a call on
/// fresh shots 0 and 1 stops on shot 0. Placed whole, or not placed, shot
/// 1 never runs; placed per shot, its job runs before shot 0's (a holding
/// placement makes the order certain). Each form reports shot 2's state
/// alone. Chosen so that every order of the wrappers gives the same report;
/// `futures`' `try_join_all` and rayon's `collect::<Result<_, _>>()` drop the
/// others' results too. The executor waits for every shot's calls first,
/// so a failure recorded only then still decides.
#[test]
fn a_failed_call_reports_no_state_made_for_it_in_every_order() {
    use DrawingOp::Flip;
    let simulator = || {
        executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3)
            .log_probability(|state: &Flipped| f64::from(state.flips))
            .state_report(flips)
    };
    let mut expected_log_p = LogP::default();
    expected_log_p.add(2, 1.0);
    let expected = (expected_log_p, Flips::of([(2, 1)]));
    let report = states_and_log_p(simulator().shots(OnePerShot), &[Flip]);
    assert_eq!(report, expected, "not placed");
    let on_one_thread = simulator().shots(OnePerShot).run_on(OwnThread);
    assert_eq!(
        states_and_log_p(on_one_thread, &[Flip]),
        expected,
        "placed whole"
    );
    let (placement, jobs) = holding();
    let mut per_shot = simulator().run_on(placement).shots(OnePerShot);
    let shot_2 = ShotSet::one(2);
    let reply = per_shot
        .call(InstructionBatchRef {
            shots: &shot_2,
            instructions: &[Flip],
        })
        .expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 2's call hands a job"))();
    ended(reply).1.expect("shot 2 does not stop");
    let fresh = ShotSet::new(vec![0, 1]).expect("two shots");
    let reply = per_shot
        .call(InstructionBatchRef {
            shots: &fresh,
            instructions: &[Flip],
        })
        .expect("an open executor queues a call");
    let shot_0 = jobs.try_recv().expect("shot 0's call hands a job");
    (jobs.try_recv().expect("shot 1's call hands a job"))();
    shot_0();
    ended(reply).1.expect_err("shot 0 stops");
    let closing = thread::spawn(move || per_shot.close());
    // Each shot's wait for its calls, its close, then the drop of its
    // executor, are jobs: run until the executor, closed and dropped, holds
    // the placement no more.
    while let Ok(job) = jobs.recv_timeout(PATIENT) {
        job();
    }
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    assert_eq!((report.log_p, report.states), expected, "placed per shot");
}

/// A device that counts its flips, placed per shot, runs shot 2, then a
/// call on fresh shots 0 and 1 that stops on shot 0; the caller closes it
/// without polling that call's reply, before shot 0's job has run (a
/// holding placement makes the order certain). `close` waits for every
/// shot's calls before it reads any state, not only for the first
/// shot's, so the failure, known only once shot 0's call ran, still
/// decides: the report holds shot 2's state alone.
#[test]
fn close_waits_for_every_shots_calls_before_it_reads_a_state() {
    use DrawingOp::Flip;
    /// Long enough for a close that did not wait for shot 0's calls to
    /// hand the job that closes shot 2's state.
    const A_WHILE: Duration = Duration::from_millis(100);
    let (placement, jobs) = holding();
    let mut per_shot = executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3)
        .log_probability(|state: &Flipped| f64::from(state.flips))
        .state_report(flips)
        .run_on(placement)
        .shots(OnePerShot);
    let shot_2 = ShotSet::one(2);
    let reply = per_shot
        .call(InstructionBatchRef {
            shots: &shot_2,
            instructions: &[Flip],
        })
        .expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 2's call hands a job"))();
    ended(reply).1.expect("shot 2 does not stop");
    let fresh = ShotSet::new(vec![0, 1]).expect("two shots");
    let _unpolled = per_shot
        .call(InstructionBatchRef {
            shots: &fresh,
            instructions: &[Flip],
        })
        .expect("an open executor queues a call");
    let shot_0 = jobs.try_recv().expect("shot 0's call hands a job");
    (jobs.try_recv().expect("shot 1's call hands a job"))();
    let closing = thread::spawn(move || per_shot.close());
    (jobs
        .recv_timeout(PATIENT)
        .expect("close waits for shot 2's calls"))();
    // Waiting for shot 0's calls, behind the job held here, the close
    // hands no job until that job runs.
    if let Ok(job) = jobs.recv_timeout(A_WHILE) {
        job();
    }
    shot_0();
    while let Ok(job) = jobs.recv_timeout(PATIENT) {
        job();
    }
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    let mut log_p = LogP::default();
    log_p.add(2, 1.0);
    assert_eq!((report.log_p, report.states), (log_p, Flips::of([(2, 1)])));
}

/// The shots whose state a report closure read, each under the closure's
/// name: `"log_p"` for the log probability's, `"report"` for the state's
/// report's.
type ReadBy = Arc<Mutex<Vec<(&'static str, ShotId)>>>;

/// What `read_by` recorded, taken, sorted.
fn taken(read_by: &ReadBy) -> Vec<(&'static str, ShotId)> {
    let mut read = mem::take(&mut *read_by.lock().expect("no test thread panics holding it"));
    read.sort_unstable();
    read
}

/// A device that counts its flips gives its log probability and each
/// state's report through closures that log, as a device driver's might, each
/// state they read; a call on fresh shots 0 and 1 stops on shot 0, after
/// shot 2 ran alone. In every order of the wrappers, the closures read shot
/// 2's state alone, never one the report leaves out, though placed per
/// shot, shot 1 may have run there: a closure acts on no state but those
/// reported (a log, a counter, a panic), whatever the order.
#[test]
fn the_report_closures_never_read_a_state_made_for_a_failed_call() {
    use DrawingOp::Flip;
    let read_by = ReadBy::default();
    let simulator = || {
        let (by_log_p, by_report) = (Arc::clone(&read_by), Arc::clone(&read_by));
        executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3)
            .log_probability(move |state: &Flipped| {
                let mut read = by_log_p.lock().expect("no test thread panics holding it");
                read.push(("log_p", state.shot));
                f64::from(state.flips)
            })
            .state_report(move |state: &Flipped| {
                let mut read = by_report.lock().expect("no test thread panics holding it");
                read.push(("report", state.shot));
                flips(state)
            })
    };
    let shot_2 = vec![("log_p", 2), ("report", 2)];
    states_and_log_p(simulator().shots(OnePerShot), &[Flip]);
    assert_eq!(taken(&read_by), shot_2, "not placed");
    states_and_log_p(simulator().shots(OnePerShot).run_on(OwnThread), &[Flip]);
    assert_eq!(taken(&read_by), shot_2, "placed whole");
    states_and_log_p(simulator().run_on(OwnThread).shots(OnePerShot), &[Flip]);
    assert_eq!(taken(&read_by), shot_2, "placed per shot");
}

/// A device whose state's report has a bug that only the states of a
/// failed call show (it panics on shots 0 and 1): a call on fresh shots 0
/// and 1 stops on shot 0, after shot 2 ran alone. `close` never makes those
/// states' reports, so in every order of the wrappers it returns shot 2's
/// report, where reading them would raise the bug's panic in one order
/// only.
#[test]
fn a_state_report_that_panics_on_a_state_of_a_failed_call_leaves_close_unchanged() {
    use DrawingOp::Flip;
    let simulator = || {
        executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3).state_report(
            |state: &Flipped| {
                assert!(state.shot > 1, "the report has a bug on shots 0 and 1");
                flips(state)
            },
        )
    };
    let shot_2 = (LogP::default(), Flips::of([(2, 1)]));
    let report = states_and_log_p(simulator().shots(OnePerShot), &[Flip]);
    assert_eq!(report, shot_2, "not placed");
    let on_one_thread = simulator().shots(OnePerShot).run_on(OwnThread);
    assert_eq!(
        states_and_log_p(on_one_thread, &[Flip]),
        shot_2,
        "placed whole"
    );
    let per_shot = simulator().run_on(OwnThread).shots(OnePerShot);
    assert_eq!(
        states_and_log_p(per_shot, &[Flip]),
        shot_2,
        "placed per shot"
    );
}

/// A logic of [`Flipped`] that keeps state of its own, a count of its
/// runs, which it captures, and logs in `runs` each run's shot and count.
fn counting_runs(
    runs: &Arc<Mutex<Vec<(ShotId, u32)>>>,
) -> impl FnMut(&mut Flipped, &CallInput<'_, DrawingOp>, &mut Outcomes<'_>) -> Result<(), String>
+ Clone
+ Send
+ use<> {
    let runs = Arc::clone(runs);
    let mut count = 0;
    move |state, _, _| {
        count += 1;
        let mut runs = runs.lock().expect("no test thread panics holding it");
        runs.push((state.shot, count));
        Ok(())
    }
}

/// The runs that `executor`, a [`Flipped`] per shot whose logic is
/// [`counting_runs`], logs as shot 0 runs a call, then another, then shot
/// 1 runs one, each waited for.
fn runs_counted(
    mut executor: impl Executor<InstructionBatch<DrawingOp>>,
    runs: &Arc<Mutex<Vec<(ShotId, u32)>>>,
) -> Vec<(ShotId, u32)> {
    for shot in [0, 0, 1] {
        let shots = ShotSet::one(shot);
        let reply = executor
            .call(InstructionBatchRef {
                shots: &shots,
                instructions: &[DrawingOp::Flip],
            })
            .expect("an open executor runs a call");
        wait(reply, &mut Vec::new()).expect("the logic does not fail");
    }
    mem::take(&mut *runs.lock().expect("no test thread panics holding it"))
}

/// A logic that counts its runs in a counter it captures, placed before
/// `OnePerShot`: each shot runs a copy of it of its own, made at the
/// shot's first call, so shot 0 counts 1, then 2, and shot 1, run after
/// it, counts 1.
#[test]
fn placed_before_one_per_shot_each_shot_runs_a_copy_of_the_logic_of_its_own() {
    let runs = Arc::default();
    let per_shot = executor_fn(Capabilities::default(), Flipped::new, counting_runs(&runs))
        .run_on(OwnThread)
        .shots(OnePerShot);
    assert_eq!(runs_counted(per_shot, &runs), [(0, 1), (0, 2), (1, 1)]);
}

/// A logic that counts its runs in a counter it captures, placed after
/// `OnePerShot`: every shot runs the one logic, so shot 1, run after
/// shot 0's two calls, counts 3.
#[test]
fn placed_after_one_per_shot_every_shot_runs_the_one_logic() {
    let runs = Arc::default();
    let on_one_thread = executor_fn(Capabilities::default(), Flipped::new, counting_runs(&runs))
        .shots(OnePerShot)
        .run_on(OwnThread);
    assert_eq!(runs_counted(on_one_thread, &runs), [(0, 1), (0, 2), (1, 3)]);
}

/// The executor learns of a failure on a shot's placement as it closes, at
/// the latest, and leaves out the states made for the failed call then
/// too: a caller flips shot 2, then sends a flip on shots 0 and 1, polls
/// no reply, and closes; shot 0 stops only once the executor is closing
/// (as it waits for shot 2's calls). It reports shot 2's state alone, as
/// when the failure is recorded before close, since it waits for every shot's
/// calls before it decides.
#[test]
fn a_failure_that_happens_as_it_closes_leaves_out_the_states_of_its_call() {
    let mut expected_log_p = LogP::default();
    expected_log_p.add(2, 1.0);
    assert_eq!(
        closed_as_its_calls_run(&[0, 1], &[]),
        (expected_log_p, Flips::of([(2, 1)]))
    );
}

/// Of several failed calls, the earliest decides which states are left
/// out, though a later call failed after it: a caller flips shot 2, then
/// sends a flip on shots 0 and 1, then on shot 3, all accepted before
/// either ran; shot 0 stops, then shot 3. It reports shot 2's state alone,
/// as unplaced, where the call on shot 3 would have been refused.
#[test]
fn the_earliest_failed_call_decides_though_a_later_one_failed_after_it() {
    let mut expected_log_p = LogP::default();
    expected_log_p.add(2, 1.0);
    assert_eq!(
        closed_as_its_calls_run(&[0, 1], &[3]),
        (expected_log_p, Flips::of([(2, 1)]))
    );
}

/// Of several failed calls, the earliest decides which states are left
/// out, though the executor saw it only at a later call: a caller on a
/// pool calls shot 0, whose device stops, and does not poll the reply;
/// the pool runs that call while a call on shots 2 then 0 is starting,
/// and shot 0 refuses it, so the executor records the failure at that
/// later call. At close it reports no state: neither shot 2's, made for
/// the later call, nor shot 0's, made for the earliest failed call, as
/// unplaced, where the call on shots 2 and 0 would have been refused.
#[test]
fn a_shots_failure_found_by_a_later_call_leaves_out_the_states_of_its_own_call() {
    let (placement, jobs) = holding();
    let jobs = Arc::new(Mutex::new(jobs));
    let pool = Arc::clone(&jobs);
    let mut bit = bits(&Threads::default());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            if context.shot() == 2 {
                let job = pool.lock().expect("no test thread panics holding it");
                (job.try_recv().expect("shot 0's first call hands a job"))();
            }
            bit(context)
        },
        run_bit as RunBit,
    )
    .log_probability(bit_value)
    .run_on(placement)
    .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    let reply = call(&mut per_shot, &[2, 0], &[Op::Read(0)])
        .expect("shot 2 was handed the call before shot 0 refused it");
    (jobs
        .lock()
        .expect("no test thread panics holding it")
        .try_recv()
        .expect("shot 2 was handed a job"))();
    ended(reply).1.expect_err("shot 0 refused the call");
    let closing = thread::spawn(move || per_shot.close());
    // Each shot's wait for its calls, its close, then the drop of its
    // executor, are jobs.
    loop {
        let job = jobs
            .lock()
            .expect("no test thread panics holding it")
            .recv_timeout(PATIENT);
        let Ok(job) = job else { break };
        job();
    }
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    assert_eq!(report.log_p, LogP::default());
}

/// When several calls failed, a later call is refused with the earliest
/// one's reason, whatever order they failed in and whichever replies were
/// polled: a caller on a pool flips shot 0, then shot 3, whose devices
/// both stop; the pool runs shot 3's call first, then shot 0's. Before any
/// reply is polled, and after each is, in either order, a call is refused
/// with shot 0's failure, the earliest call's, as the executor's own
/// failures are ([`ExecutorLifecycle::fail`](crate::ExecutorLifecycle::fail)):
/// the refusal does not change as replies are polled, and the later call's
/// reply, polled after the earlier one, names the same failure.
#[test]
fn the_refusal_names_the_earliest_failed_call_once_it_failed() {
    let earliest = || {
        Some(Rejected::EarlierFailure(
            "the device stopped on shot 0".to_owned(),
        ))
    };
    for earlier_polled_first in [true, false] {
        let (placement, jobs) = holding();
        let mut per_shot =
            executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3)
                .run_on(placement)
                .shots(OnePerShot);
        let mut flip = |shot: ShotId| {
            let shots = ShotSet::one(shot);
            per_shot.call(InstructionBatchRef {
                shots: &shots,
                instructions: &[DrawingOp::Flip],
            })
        };
        let first = flip(0).expect("an open executor queues a call");
        let second = flip(3).expect("nothing failed yet when it is called");
        let shot_0 = jobs.try_recv().expect("shot 0's call hands a job");
        (jobs.try_recv().expect("shot 3's call hands a job"))();
        assert_eq!(
            flip(5).err(),
            Some(Rejected::EarlierFailure(
                "the device stopped on shot 3".to_owned()
            )),
            "only the later call failed yet"
        );
        shot_0();
        assert_eq!(flip(5).err(), earliest(), "no reply polled");
        if earlier_polled_first {
            ended(first).1.expect_err("shot 0 stopped");
            assert_eq!(flip(5).err(), earliest(), "the earlier reply polled");
            assert_eq!(
                ended(second).1,
                Err("an earlier call failed: the device stopped on shot 0".to_owned())
            );
        } else {
            ended(second).1.expect_err("shot 3 stopped");
            assert_eq!(flip(5).err(), earliest(), "the later reply polled");
            ended(first).1.expect_err("shot 0 stopped");
        }
        assert_eq!(flip(5).err(), earliest(), "both replies polled");
    }
}

/// The log probabilities and the states' report of a state per shot of
/// [`Flipped`], on placements that run the logic of
/// [`stop_on_shots_0_and_3`], after a flip on shot 2, run, then a flip on
/// `second`'s shots, then on `third`'s (if any), accepted, their replies
/// dropped unpolled, and run only once the executor started closing: once
/// it handed shot 2 its first job at close.
fn closed_as_its_calls_run(second: &[ShotId], third: &[ShotId]) -> (LogP, Flips) {
    let (placement, jobs) = holding();
    let mut per_shot = executor_fn(Capabilities::default(), Flipped::new, stop_on_shots_0_and_3)
        .log_probability(|state: &Flipped| f64::from(state.flips))
        .state_report(flips)
        .run_on(placement)
        .shots(OnePerShot);
    let mut held = Vec::new();
    for shots in [&[2][..], second, third] {
        if shots.is_empty() {
            continue;
        }
        let shots = ShotSet::new(shots.to_vec()).expect("the tests name each shot once");
        let reply = per_shot.call(InstructionBatchRef {
            shots: &shots,
            instructions: &[DrawingOp::Flip],
        });
        drop(reply.expect("no shot failed yet"));
        held.extend(jobs.try_iter());
    }
    let mut held = held.into_iter();
    (held.next().expect("shot 2's call hands a job"))();
    let closing = thread::spawn(move || per_shot.close());
    let close_2 = jobs.recv_timeout(PATIENT);
    for job in held {
        job();
    }
    (close_2.expect("the close hands shot 2 a job"))();
    while let Ok(job) = jobs.recv_timeout(PATIENT) {
        job();
    }
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    (report.log_p, report.states)
}

/// The log probabilities and the states' report of `executor` after it
/// ran `ops` on shot 2, then on shots 0 and 1, each call waited for.
fn states_and_log_p(
    mut executor: impl Executor<InstructionBatch<DrawingOp>>
    + Close<Report = ExecutorReport<Flips>>
    + Send
    + 'static,
    ops: &'static [DrawingOp],
) -> (LogP, Flips) {
    ends_soon(move || {
        for shots in [vec![2], vec![0, 1]] {
            let shots = ShotSet::new(shots).expect("the tests name each shot once");
            let reply = executor
                .call(InstructionBatchRef {
                    shots: &shots,
                    instructions: ops,
                })
                .expect("an open executor runs a call");
            let _ = wait(reply, &mut Vec::new());
        }
        let report = executor.close();
        (report.log_p, report.states)
    })
}

/// A device that holds two shots at once: a call that would hold a third shot
/// is refused with that shot, and runs nothing; a call within its two runs.
#[test]
fn a_call_beyond_the_promised_max_shots_is_refused_with_the_first_shot_it_has_no_room_for() {
    let threads = Threads::default();
    let mut per_shot = executor_fn(
        Capabilities::default().with_max_shots(2),
        bits(&threads),
        run_bit as RunBit,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    assert_eq!(
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(2))
    );
    assert!(seen(&threads).is_empty(), "the refused call runs nothing");
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("two shots fit");
    end.expect("the logic does not fail");
    assert_eq!(
        send(&mut per_shot, &[0, 2], &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(2))
    );
}

/// A device that holds two shots at once, a call on three new shots: the call
/// is refused for room before any state is made, so the device makes none.
#[test]
fn a_call_refused_for_room_makes_no_state() {
    let made = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&made);
    let mut bit = bits(&Threads::default());
    let mut per_shot = executor_fn(
        Capabilities::default().with_max_shots(2),
        move |context: ShotContext| {
            *counted.lock().expect("no test thread panics holding it") += 1;
            bit(context)
        },
        run_bit as RunBit,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).expect_err("two shots at once, three shots");
    assert_eq!(*made.lock().expect("no test thread panics holding it"), 0);
}

/// A device whose shot 1 cannot be made, being busy: the whole call is
/// refused with the device's refusal, and no shot runs, not even shot 0,
/// whose state could be made.
#[test]
fn a_state_that_cannot_be_made_refuses_the_whole_call_with_its_refusal() {
    let threads = Threads::default();
    let mut bit = bits(&threads);
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            if context.shot() == 1 {
                return Err(Rejected::Unavailable("shot 1 is busy".to_owned()));
            }
            bit(context)
        },
        run_bit as RunBit,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    assert_eq!(
        send(&mut per_shot, &[0, 1], &[Op::Flip]).err(),
        Some(Rejected::Unavailable("shot 1 is busy".to_owned()))
    );
    assert!(seen(&threads).is_empty());
}

/// A device busy at a first call on shots 0 and 1: the state made for
/// shot 0 is dropped with the refused call, made again at the next, and
/// reported once.
#[test]
fn the_states_made_for_a_refused_call_are_not_kept() {
    let made = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&made);
    let threads = Threads::default();
    let mut bit = bits(&threads);
    let mut busy = true;
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            *counted.lock().expect("no test thread panics holding it") += 1;
            if context.shot() == 1 && std::mem::take(&mut busy) {
                return Err(Rejected::Unavailable("the device is busy".to_owned()));
            }
            bit(context)
        },
        run_bit as RunBit,
    )
    .log_probability(bit_value as BitLogP)
    .run_on(OwnThread)
    .shots(OnePerShot);
    send(&mut per_shot, &[0, 1], &[Op::Flip]).expect_err("shot 1 is busy");
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("shot 1 is free");
    end.expect("the logic does not fail");
    assert_eq!(*made.lock().expect("no test thread panics holding it"), 4);
    let report = ends_soon(move || per_shot.close());
    let mut log_p = LogP::default();
    log_p.add(0, 1.0);
    log_p.add(1, 1.0);
    assert_eq!(report.log_p, log_p);
}

/// A simulator's check refuses a batch that reads twice under one tag: no
/// state is made and no shot runs.
#[test]
fn the_check_refuses_a_call_before_any_state_is_made() {
    let made = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&made);
    let threads = Threads::default();
    let mut bit = bits(&threads);
    let refusal = Rejected::Instruction {
        at: 1,
        reason: "tag 0 read twice".to_owned(),
    };
    let refused = refusal.clone();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            *counted.lock().expect("no test thread panics holding it") += 1;
            bit(context)
        },
        run_bit as RunBit,
    )
    .check(move |input: &CallInput<'_, Op>| {
        let reads = input
            .instructions()
            .iter()
            .filter(|op| matches!(op, Op::Read(0)));
        if reads.count() > 1 {
            return Err(refused.clone());
        }
        Ok(())
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    assert_eq!(
        send(&mut per_shot, &[0, 1], &[Op::Read(0), Op::Read(0)]).err(),
        Some(refusal)
    );
    assert_eq!(*made.lock().expect("no test thread panics holding it"), 0);
    assert!(seen(&threads).is_empty());
}

/// A device that holds two shots at once, called on three shots with a batch
/// its check refuses: the check's refusal comes first, not the shot it has no
/// room for.
#[test]
fn the_check_refuses_a_call_before_its_room_is_checked() {
    let mut per_shot = executor_fn(
        Capabilities::default().with_max_shots(2),
        bits(&Threads::default()),
        run_bit as RunBit,
    )
    .check(|input: &CallInput<'_, Op>| {
        check_unique_tags(input.instructions().iter().enumerate().filter_map(
            |(at, op)| match *op {
                Op::Read(tag) | Op::ReadInverted(tag) => Some((at, tag)),
                _ => None,
            },
        ))
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    let refused = send(&mut per_shot, &[0, 1, 2], &[Op::Read(1), Op::Read(1)]).err();
    assert!(
        matches!(refused, Some(Rejected::Instruction { at: 1, .. })),
        "{refused:?}"
    );
}

/// A device that holds two shots at once, closed, then called on three shots:
/// the call is refused as closed, not for the shot it has no room for.
#[test]
fn closed_it_refuses_a_call_before_its_room_is_checked() {
    let mut per_shot = executor_fn(
        Capabilities::default().with_max_shots(2),
        bits(&Threads::default()),
        run_bit as RunBit,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let _ = per_shot.close();
    assert_eq!(
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// Closed after a shot's failure that no reply showed, it refuses a call
/// as closed, not with the failure. Chosen because every executor checks
/// close first (`ExecutorLifecycle::admit`): the refusal then does not
/// depend on whether a reply was polled. A device stops on shot 0 in a
/// call whose reply the caller drops unpolled; the caller closes, then
/// calls again.
#[test]
fn closed_after_a_shots_unseen_failure_it_refuses_as_closed() {
    let mut per_shot = recording(&Threads::default())
        .run_on(OwnThread)
        .shots(OnePerShot);
    ends_soon(move || {
        let unpolled =
            call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
        let _ = per_shot.close();
        assert_eq!(
            call(&mut per_shot, &[1], &[Op::Flip]).err(),
            Some(Rejected::Closed)
        );
        drop(unpolled);
    });
}

/// Once a shot's device stopped, the executor refuses every call: the
/// conformance kit's check.
#[test]
fn after_a_failed_call_it_refuses_every_later_call() {
    let mut per_shot = recording(&Threads::default())
        .run_on(OwnThread)
        .shots(OnePerShot);
    ends_soon(move || {
        let shots = ShotSet::new(vec![0, 1]).expect("two shots");
        conformance::refuses_calls_after_a_failure(
            &mut per_shot,
            InstructionBatchRef {
                shots: &shots,
                instructions: &[Op::Stop],
            },
            &[
                InstructionBatchRef {
                    shots: &shots,
                    instructions: &[Op::Read(0)],
                },
                InstructionBatchRef {
                    shots: &shots,
                    instructions: &[Op::Read(0)],
                },
            ],
        );
    });
}

/// A caller sends a call on shots 0 and 1, whose devices stop, then a read
/// on shot 2 before it waits for the first reply: the read is refused,
/// naming the stop, when the failure is recorded already (shared by the
/// shots, or as the executor's), or else queued on
/// shot 2's thread, where it may even run, but it ends with the failure,
/// handing over nothing, since the failure ends the whole executor. The
/// conformance kit's check.
#[test]
fn calls_in_flight_on_other_shots_end_at_an_earlier_failure() {
    let mut per_shot = recording(&Threads::default())
        .run_on(OwnThread)
        .shots(OnePerShot);
    ends_soon(move || {
        let (failing, later) = (
            ShotSet::new(vec![0, 1]).expect("two shots"),
            ShotSet::one(2),
        );
        conformance::ends_calls_in_flight_at_an_earlier_failure(
            &mut per_shot,
            InstructionBatchRef {
                shots: &failing,
                instructions: &[Op::Stop],
            },
            InstructionBatchRef {
                shots: &later,
                instructions: &[Op::Read(0)],
            },
        );
    });
}

/// The tests' device on `placement`, per shot, whose check and state
/// closure count their runs in `checked` and `made`.
fn counting(
    placement: impl ExecutorPlacement + Clone,
    checked: &Arc<AtomicUsize>,
    made: &Arc<AtomicUsize>,
) -> impl Executor<InstructionBatch<Op>> {
    let (checked, made) = (Arc::clone(checked), Arc::clone(made));
    let mut bit = bits(&Threads::default());
    executor_fn(
        Capabilities::default(),
        move |context| {
            made.fetch_add(1, Ordering::Relaxed);
            bit(context)
        },
        run_bit as RunBit,
    )
    .check(move |_: &CallInput<'_, Op>| {
        checked.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })
    .run_on(placement)
    .shots(OnePerShot)
}

/// Calls `shots` of `executor` with `ops`.
fn call<E: Executor<InstructionBatch<Op>>>(
    executor: &mut E,
    shots: &[ShotId],
    ops: &[Op],
) -> Result<E::Reply, Rejected> {
    let shots = ShotSet::new(shots.to_vec()).expect("the tests name each shot once");
    executor.call(InstructionBatchRef {
        shots: &shots,
        instructions: ops,
    })
}

/// A caller on a pool, which does not poll the reply of a call whose
/// logic stops shot 0's device: once the pool ran it, a call on shots 0
/// and 2 is refused with that failure before the executor's check, and
/// before shot 2's state is made.
#[test]
fn after_a_shots_failure_a_call_is_refused_before_its_check_and_states() {
    let (placement, jobs) = holding();
    let (checked, made) = (Arc::default(), Arc::default());
    let mut per_shot = counting(placement, &checked, &made);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 0's first call hands a job"))();
    let refused = call(&mut per_shot, &[0, 2], &[Op::Read(0)]).err();
    assert_eq!(
        refused,
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
    let runs = |count: &AtomicUsize| count.load(Ordering::Relaxed);
    assert_eq!(
        (runs(&checked), runs(&made)),
        (1, 1),
        "only the first call's"
    );
}

/// A caller on a pool, which does not poll the reply of a call whose
/// logic stops shot 0's device: once the pool ran it, a call on shots 2
/// then 0 is refused with that failure, and shot 2 does not run: its
/// placement is handed no job.
#[test]
fn after_a_shots_failure_a_call_is_refused_before_any_shot_runs() {
    let (placement, jobs) = holding();
    let mut per_shot = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 0's first call hands a job"))();
    let refused = call(&mut per_shot, &[2, 0], &[Op::Read(0)]).err();
    assert_eq!(
        refused,
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
    assert!(jobs.try_recv().is_err(), "shot 2 ran");
}

/// A caller on a pool, which does not poll the reply of a call whose
/// logic stops shot 0's device: once the pool ran it, a call on shot 1
/// alone, which shot 0's failure did not hit, is refused with it too.
#[test]
fn after_a_shots_failure_a_call_on_other_shots_is_refused() {
    let (placement, jobs) = holding();
    let mut per_shot = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 0's first call hands a job"))();
    assert_eq!(
        call(&mut per_shot, &[1], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A caller on a pool, which does not poll the reply of a call whose
/// logic stops shot 0's device, calls shots 0 and 2; the pool runs shot
/// 0's job while shot 2's state is made, after the call was admitted and
/// before shot 0 is given it: shot 0, called first, refuses for its
/// failure, so the call is refused, as a call made after the failure is.
#[test]
fn a_failure_before_its_shot_gets_the_call_refuses_it_if_first() {
    let (placement, jobs) = holding();
    let jobs = Arc::new(Mutex::new(jobs));
    let pool = Arc::clone(&jobs);
    let mut bit = bits(&Threads::default());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            if context.shot() == 2 {
                let job = pool.lock().expect("no test thread panics holding it");
                (job.try_recv().expect("shot 0's first call hands a job"))();
            }
            bit(context)
        },
        run_bit as RunBit,
    )
    .run_on(placement)
    .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    assert_eq!(
        call(&mut per_shot, &[0, 2], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
    let jobs = jobs.lock().expect("no test thread panics holding it");
    assert!(jobs.try_recv().is_err(), "shot 2 ran");
}

/// A caller on a pool, which does not poll the reply of a call whose
/// logic stops shot 0's device, calls shots 2 then 0; the pool runs shot
/// 0's job while shot 2's state is made, before shot 0 is given the call:
/// shot 2, called first, accepts the call before shot 0 refuses it, too
/// late to refuse it, so the call is accepted, shot 2 runs, and the reply
/// fails with shot 0's failure.
#[test]
fn a_failure_before_its_shot_gets_the_call_fails_its_reply_if_later() {
    let (placement, jobs) = holding();
    let jobs = Arc::new(Mutex::new(jobs));
    let pool = Arc::clone(&jobs);
    let mut bit = bits(&Threads::default());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |context: ShotContext| {
            if context.shot() == 2 {
                let job = pool.lock().expect("no test thread panics holding it");
                (job.try_recv().expect("shot 0's first call hands a job"))();
            }
            bit(context)
        },
        run_bit as RunBit,
    )
    .run_on(placement)
    .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    let reply = call(&mut per_shot, &[2, 0], &[Op::Read(0)])
        .expect("shot 2 was handed the call before shot 0 refused it");
    (jobs
        .lock()
        .expect("no test thread panics holding it")
        .try_recv()
        .expect("shot 2 was handed a job"))();
    let (outcomes, end) = ended(reply);
    assert_eq!(values(&outcomes), [(2, 0, Value::Zero)]);
    assert_eq!(
        end,
        Err("shot 0: an earlier call failed: the device stopped".to_owned())
    );
}

/// A caller on a pool calls shot 0 to stop its device, then, before the
/// pool runs it, calls shots 0 and 2 (in both orders) to read: the second
/// call, accepted before the failure, is queued on shot 0 behind the
/// failing call, so it does not run there and its reply fails with `an
/// earlier call failed: …`, as shot 0's executor gives it, not named by
/// the shot; shot 2, before or after shot 0 in the batch, runs it.
#[test]
fn a_call_queued_behind_a_shots_failure_fails_while_its_other_shots_run() {
    for shots in [[0, 2], [2, 0]] {
        let (placement, jobs) = holding();
        let mut per_shot = recording(&Threads::default())
            .run_on(placement)
            .shots(OnePerShot);
        let _unpolled =
            call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
        let behind = call(&mut per_shot, &shots, &[Op::Read(0)])
            .expect("nothing failed yet when it is called");
        for job in jobs.try_iter() {
            job();
        }
        let (outcomes, end) = ended(behind);
        assert_eq!(values(&outcomes), [(2, 0, Value::Zero)], "{shots:?}");
        assert_eq!(
            end,
            Err("an earlier call failed: the device stopped".to_owned()),
            "{shots:?}"
        );
    }
}

/// A caller on a pool calls shot 0 to stop its device, then shots 1 and 2
/// to read, and never polls the first reply: the pool runs every job, so
/// shot 0's device stops, and the executor refuses its next call, yet the
/// read, accepted before the failure and on other shots, ends done with
/// both shots' readouts, since the failure is not recorded as the
/// executor's until that reply is polled: refusing the next call ends no
/// call already in flight (the open design question at `ShotReplies`).
#[test]
fn a_call_on_other_shots_ends_done_while_the_failing_reply_is_unpolled() {
    let (placement, jobs) = holding();
    let mut per_shot = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor queues a call");
    let later =
        call(&mut per_shot, &[1, 2], &[Op::Read(0)]).expect("nothing failed yet when it is called");
    for job in jobs.try_iter() {
        job();
    }
    call(&mut per_shot, &[3], &[Op::Read(0)]).expect_err("shot 0's device stopped");
    let (outcomes, end) = ended(later);
    assert_eq!(
        (values(&outcomes), end),
        (vec![(1, 0, Value::Zero), (2, 0, Value::Zero)], Ok(()))
    );
}

/// A pool that cannot make a thread for a second shot (the system ran
/// out): its first start runs each job as it is handed, inline, and its
/// next panics, as `OwnThread` does when no thread can be made.
#[derive(Clone, Default)]
struct ThreadsRunOut(Arc<AtomicUsize>);

impl ExecutorPlacement for ThreadsRunOut {
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        let started = self.0.fetch_add(1, Ordering::Relaxed);
        assert!(started == 0, "the system cannot make a thread");
        Box::new(|job: PlacementJob| job())
    }
}

/// Shot 1's placement panics as it starts, after shot 0 ran the batch on
/// shots 0 and 1: the panic reaches the caller, who never gets shot 0's
/// outcomes; as a `Mutex` is poisoned, the executor refuses every later
/// call, naming the panic, rather than run shot 0 on a state the caller's
/// program never made.
#[test]
fn after_a_panic_inside_call_it_refuses_every_later_call() {
    let mut per_shot = recording(&Threads::default())
        .run_on(ThreadsRunOut::default())
        .shots(OnePerShot);
    panic::catch_unwind(AssertUnwindSafe(|| {
        call(&mut per_shot, &[0, 1], &[Op::Flip]).map(drop)
    }))
    .expect_err("shot 1's placement's panic reaches the caller");
    assert_eq!(
        call(&mut per_shot, &[0], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// A caller with calls in flight on shots 0 and 1, on a pool's threads:
/// shot 0's logic stops its device, and once its reply is seen failed,
/// shot 1's call, which has not run, ends at its next poll with that
/// failure, rather than waiting for its job.
#[test]
fn a_call_in_flight_on_another_shot_ends_at_its_next_poll_without_waiting_for_its_job() {
    let (placement, jobs) = holding();
    let mut per_shot = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let mut call = |j, instructions: &[Op]| {
        per_shot
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions,
            })
            .expect("no failure is recorded yet")
    };
    let stopping = call(0, &[Op::Stop]);
    let mut reading = call(1, &[Op::Read(0)]);
    (jobs.try_recv().expect("shot 0's first call hands a job"))();
    ended(stopping).1.expect_err("shot 0's device stopped");
    let mut outcomes = Vec::new();
    assert_eq!(
        reading.poll_events(&mut outcomes, Waker::noop()),
        ReplyStatus::Failed("an earlier call failed: the device stopped".into())
    );
    assert!(outcomes.is_empty(), "shot 1's call never ran");
}

/// A caller that waits for a call whose logic stops shot 0's device,
/// then calls on shot 1 alone: the failure, recorded as the reply was polled,
/// fails the executor, so the call is refused with its reason, though it
/// never reaches shot 0.
#[test]
fn a_failure_recorded_by_its_reply_refuses_a_later_call_on_other_shots() {
    let mut per_shot = recording(&Threads::default())
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor runs a call");
    let reason = end.expect_err("the logic stops shot 0's device");
    assert_eq!(
        send(&mut per_shot, &[1], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure(reason))
    );
}

/// Once closed, the executor refuses every call: the conformance kit's
/// check.
#[test]
fn closed_it_refuses_every_call() {
    let mut per_shot = recording(&Threads::default())
        .run_on(OwnThread)
        .shots(OnePerShot);
    ends_soon(move || {
        let shot = ShotSet::one(0);
        conformance::rejects_calls_once_closed(
            &mut per_shot,
            InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Read(0)],
            },
        );
    });
}

/// A device whose shots each run on a thread of their own, whose log
/// probability has a bug: it panics as `close` reads it on a shot's
/// placement. `close` re-raises the panic in its caller, its payload
/// unchanged, as the same executor not placed would panic there: work
/// moved to another thread reports its panic to the caller that waits for
/// it, as `std::thread::scope` and rayon do, the least surprise for a Rust
/// user.
#[test]
fn a_panic_making_a_shots_report_at_close_reaches_the_closer() {
    let mut per_shot = executor_fn(Capabilities::default(), own_bit, run_bit as RunBit)
        .log_probability(|_: &Bit| -> f64 { panic!("the log probability has a bug") })
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let raised = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .map_err(|payload| payload.downcast_ref::<&str>().copied())
    });
    assert_eq!(raised, Err(Some("the log probability has a bug")));
}

/// A pool that runs each job as it is handed, inline, until it shuts
/// down; from then on it panics at each job it is handed, including those
/// of a shot's drop, against `ExecutorPlacement`'s rule.
fn shutting(shut: &Arc<AtomicBool>) -> impl FnMut(PlacementJob) + Send + Clone + 'static {
    let shut = Arc::clone(shut);
    move |job: PlacementJob| {
        assert!(!shut.load(Ordering::SeqCst), "the pool has shut down");
        job();
    }
}

/// A device whose three shots run on a pool, which shuts down before the
/// executor is closed: the first panic at a hand-off reaches the closer,
/// its payload unchanged, though the shots not closed yet panic again as
/// they are dropped, rather than abort the process with a panic during a
/// panic. A regression aborts the test binary rather than failing this
/// test.
#[test]
fn a_placements_panic_at_close_reaches_the_closer_with_several_shots_started() {
    let shut = Arc::new(AtomicBool::new(false));
    let mut per_shot = recording(&Threads::default())
        .run_on(shutting(&shut))
        .shots(OnePerShot);
    let (_, end) =
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    shut.store(true, Ordering::SeqCst);
    let raised = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .map_err(|payload| payload.downcast_ref::<&str>().copied())
    });
    assert_eq!(raised, Err(Some("the pool has shut down")));
}

/// A device whose shots each run on a thread of their own, whose log
/// probability panicked as the first `close` read it, and the panic
/// reached the closer, who closes again: the second `close` returns an
/// empty report, as every second `close` does, neither waiting for ever
/// nor panicking again.
#[test]
fn after_a_panic_at_close_a_second_close_reports_nothing() {
    let mut per_shot = executor_fn(Capabilities::default(), own_bit, run_bit as RunBit)
        .log_probability(|_: &Bit| -> f64 { panic!("the log probability has a bug") })
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let report = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .expect_err("the log probability's panic reaches the closer");
        per_shot.close()
    });
    assert_eq!(report, ExecutorReport::default());
}

/// An executor that ran a call reports once, its states' reports
/// included: the conformance kit's check.
#[test]
fn a_second_close_reports_nothing() {
    let mut per_shot = recording(&Threads::default())
        .state_report(|_| Count(1))
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    ends_soon(move || conformance::reports_once(&mut per_shot));
}

/// A device that holds two shots at once, each placed, refuses a call that
/// would hold a third shot, then runs one on its two: the conformance kit's
/// check that a refusal leaves the executor usable.
#[test]
fn a_refused_call_leaves_it_usable() {
    let mut per_shot = executor_fn(
        Capabilities::default().with_max_shots(2),
        bits(&Threads::default()),
        run_bit as RunBit,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    ends_soon(move || {
        let (three, two) = (
            ShotSet::range(0..=2).expect("three shots"),
            ShotSet::range(0..=1).expect("two shots"),
        );
        conformance::accepts_calls_after_a_refusal(
            &mut per_shot,
            InstructionBatchRef {
                shots: &three,
                instructions: &[Op::Flip],
            },
            InstructionBatchRef {
                shots: &two,
                instructions: &[Op::Flip, Op::Read(0)],
            },
        );
    });
}

/// The device's check refuses a tag repeated within a batch of two placed
/// shots, and only that: the conformance kit's check.
#[test]
fn a_tag_repeated_within_a_batch_is_refused() {
    let mut per_shot = executor_fn(
        Capabilities::default(),
        bits(&Threads::default()),
        run_bit as RunBit,
    )
    .check(|input: &CallInput<'_, Op>| {
        check_unique_tags(input.instructions().iter().enumerate().filter_map(
            |(at, op)| match *op {
                Op::Read(tag) | Op::ReadInverted(tag) => Some((at, tag)),
                _ => None,
            },
        ))
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    ends_soon(move || {
        let two = ShotSet::range(0..=1).expect("two shots");
        // The device has one bit, so `a_again` reads `a`'s. The first check
        // still catches a refusal of a repeated qubit, since `b` reads that bit
        // too and `[a, b]` must be accepted; the index check alone could not.
        let (a, b, a_again) = (Op::Read(0), Op::Read(1), Op::ReadInverted(0));
        conformance::refuses_a_tag_repeated_within_a_batch(&mut per_shot, &two, &a, &b, &a_again);
        conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
            &mut per_shot,
            &two,
            &a,
            &Op::Flip,
            &a_again,
        );
    });
}

/// A state, on a thread of its own, that draws its value from its seed:
/// each of shots 1, 3 and 5 reads the same 16 bits of the value in two
/// calls, whether it runs alone, among the others, or with them split
/// into calls in another order: the conformance kit's check of
/// `replayable`.
#[test]
fn each_shot_alone_gives_what_it_gives_among_others() {
    let drawing = || {
        executor_fn(
            Capabilities::default().with_replayable(true),
            draw_value,
            read_drawn_bits,
        )
        .seed(SEED)
        .run_on(OwnThread)
        .shots(OnePerShot)
    };
    ends_soon(move || {
        let ops: Vec<_> = (0..16).map(DrawingOp::Read).collect();
        conformance::gives_each_shot_alone_what_it_gives_among_others(
            drawing,
            &ShotSet::new(vec![1, 3, 5]).expect("three shots"),
            &[&ops[..8], &ops[8..]],
        );
    });
}

/// A second call on shots 0 and 1, each on its own thread, reads the flip
/// of the first: the conformance kit's check of `resumable_shots`.
#[test]
fn a_second_call_continues_each_shot() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::continues_a_shot_from_one_call_to_the_next(
            || recording(&threads).run_on(OwnThread).shots(OnePerShot),
            &ShotSet::range(0..=1).expect("two shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// A flip on shots 1 and 3, each on its own thread, leaves shots 0 and 2,
/// flipped before, as they were: the conformance kit's check that a call
/// changes no shot it does not name.
#[test]
fn a_call_changes_no_shot_it_does_not_name() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::changes_no_shot_a_call_does_not_name(
            || recording(&threads).run_on(OwnThread).shots(OnePerShot),
            &ShotSet::range(0..=3).expect("four shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// After an executor flipped shots 0 to 3, each on its own thread, and was
/// closed, or dropped unclosed, the next one reads them as made: the
/// conformance kit's check that a new executor starts its shots as made.
#[test]
fn a_new_executor_starts_its_shots_as_made() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
            || recording(&threads).run_on(OwnThread).shots(OnePerShot),
            &ShotSet::range(0..=3).expect("four shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// A caller that awaits (`run.await`) a batch whose shots run on threads
/// of their own: the reply wakes the task as the shots run, and ends
/// once all have.
#[test]
fn an_async_task_awaits_a_reply_whose_shots_run_on_threads_of_their_own() {
    let (go, gate) = mpsc::channel::<()>();
    let gate = Arc::new(Mutex::new(gate));
    let threads = Threads::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        bits(&threads),
        move |bit: &mut Bit, input: &CallInput<'_, Op>, outcomes: &mut Outcomes<'_>| {
            let gate = gate.lock().expect("no test thread panics holding it");
            gate.recv_timeout(PATIENT).expect("the test opens the gate");
            drop(gate);
            run_bit(bit, input, outcomes)
        },
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let shots = ShotSet::new(vec![0, 1]).expect("two shots");
    let reply = per_shot
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Flip, Op::Read(1)],
        })
        .expect("an open executor queues a call");
    let opener = thread::spawn(move || {
        for _ in 0..2 {
            thread::sleep(Duration::from_millis(10));
            go.send(()).expect("a shot's logic waits at the gate");
        }
    });
    let (outcomes, end) = ends_soon(move || block_on(wait_async(reply)));
    opener.join().expect("the opener does not panic");
    assert_eq!(end, Ok(()));
    assert_eq!(by_shot(&outcomes), [(0, 1, Value::One), (1, 1, Value::One)]);
}

/// A run that ends while its shots' calls are still queued: `close` waits
/// for every shot's calls, then reports the calls and each state's log
/// probability, and the replies end with every outcome.
#[test]
fn closing_waits_for_every_shots_calls_then_reports_them() {
    let threads = Threads::default();
    let mut per_shot = recording(&threads)
        .run_on(|job: PlacementJob| {
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(10));
                job();
            });
        })
        .shots(OnePerShot);
    let shots = ShotSet::new(vec![0, 1]).expect("two shots");
    let replies: Vec<_> = (0..3)
        .map(|_| {
            per_shot
                .call(InstructionBatchRef {
                    shots: &shots,
                    instructions: &[Op::Flip, Op::Read(0)],
                })
                .expect("an open executor queues a call")
        })
        .collect();
    let report = ends_soon(move || per_shot.close());
    let mut log_p = LogP::default();
    log_p.add(0, 1.0);
    log_p.add(1, 1.0);
    assert_eq!(
        report,
        ExecutorReport {
            calls: 3,
            log_p,
            states: (),
        }
    );
    for (reply, value) in replies
        .into_iter()
        .zip([Value::One, Value::Zero, Value::One])
    {
        let mut out = Vec::new();
        let mut reply = reply;
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Done,
            "ended before close returned"
        );
        assert_eq!(by_shot(&out), [(0, 0, value), (1, 0, value)]);
    }
}

/// A device that must run where its calls ran (a device's driver), a
/// state per shot, each on a thread of its own, closed by another thread:
/// each shot's report is made on that shot's thread, where its calls ran.
#[test]
fn closed_it_reports_each_shots_state_on_its_placement() {
    let mut per_shot = executor_fn(Capabilities::default(), own_bit, run_bit as RunBit)
        .state_report(reported_on)
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let report = ends_soon(move || per_shot.close());
    assert_eq!(
        report.states.0.len(),
        2,
        "each shot's state is reported once"
    );
    for (ran_on, reported) in report.states.0 {
        assert_eq!(
            reported, ran_on,
            "reported on its shot's thread, not on the closer's"
        );
    }
}

/// A device's state that takes a while to free what it holds (a device
/// that powers down): dropped, it waits, then counts itself freed.
struct SlowToFree {
    bit: Bit,
    freed: Arc<AtomicUsize>,
}

impl Drop for SlowToFree {
    fn drop(&mut self) {
        thread::sleep(Duration::from_millis(100));
        self.freed.fetch_add(1, Ordering::SeqCst);
    }
}

/// A device of [`SlowToFree`] states, each counted in `freed` once freed,
/// with a log probability that panics at close: a state per shot, each on
/// a thread of its own.
fn slow_to_free(
    freed: &Arc<AtomicUsize>,
) -> impl Executor<InstructionBatch<Op>, Reply: Send + 'static>
+ Close<Report = ExecutorReport<()>>
+ Send
+ 'static {
    let freed = Arc::clone(freed);
    executor_fn(
        Capabilities::default(),
        move |_| {
            Ok(SlowToFree {
                bit: Bit {
                    one: false,
                    threads: Threads::default(),
                },
                freed: Arc::clone(&freed),
            })
        },
        |state: &mut SlowToFree, input: &CallInput<'_, Op>, out: &mut Outcomes<'_>| {
            run_bit(&mut state.bit, input, out)
        },
    )
    .log_probability(|_: &SlowToFree| -> f64 { panic!("the log probability has a bug") })
    .run_on(OwnThread)
    .shots(OnePerShot)
}

/// A device whose shots each run on a thread of their own, and whose
/// states take a while to free, as a device that powers down: its first
/// call fails, so `close` leaves both states out of its report, and has
/// freed them when it returns, so that the device can be built again at
/// once, as in the other order.
#[test]
fn a_closed_executor_has_freed_the_states_it_leaves_out() {
    let freed = Arc::new(AtomicUsize::new(0));
    let mut per_shot = slow_to_free(&freed);
    let (_, end) =
        send(&mut per_shot, &[0, 1], &[Op::Flip, Op::Stop]).expect("an open executor runs a call");
    end.expect_err("the device stops");
    ends_soon(move || per_shot.close());
    assert_eq!(freed.load(Ordering::SeqCst), 2);
}

/// A device whose shots each run on a thread of their own, and whose log
/// probability panics as `close` reads shot 0's: the panic reaches the
/// closer once shot 1, not read, was freed too, so that the device can be
/// built again once the panic is caught.
#[test]
fn a_panic_at_close_reaches_the_closer_once_every_state_is_freed() {
    let freed = Arc::new(AtomicUsize::new(0));
    let mut per_shot = slow_to_free(&freed);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .expect_err("the log probability's panic reaches the closer");
    });
    assert_eq!(freed.load(Ordering::SeqCst), 2);
}

/// A device's state that fails to power down after its logic stopped
/// (a device driver's bug): its drop counts it in `freed`, then panics.
struct FailsToPowerDown {
    bit: Bit,
    stopped: bool,
    freed: Arc<AtomicUsize>,
}

impl Drop for FailsToPowerDown {
    fn drop(&mut self) {
        if self.stopped {
            self.freed.fetch_add(1, Ordering::SeqCst);
            panic!("the device fails to power down");
        }
    }
}

/// A device whose shots each run on a thread of their own: shot 1 flips,
/// then a call on shot 0 stops, so `close` leaves shot 0's state out and
/// frees it on its placement, where its drop panics. That panic stays on
/// the placement, reported by the panic hook: `close` returns, and reports
/// shot 1.
#[test]
fn a_panic_as_close_frees_a_state_left_out_stays_on_its_placement() {
    let freed = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&freed);
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |_| {
            Ok(FailsToPowerDown {
                bit: Bit {
                    one: false,
                    threads: Threads::default(),
                },
                stopped: false,
                freed: Arc::clone(&counted),
            })
        },
        |state: &mut FailsToPowerDown, input: &CallInput<'_, Op>, out: &mut Outcomes<'_>| {
            state.stopped = (input.instructions().iter()).any(|op| matches!(op, Op::Stop));
            run_bit(&mut state.bit, input, out)
        },
    )
    .log_probability(|_: &FailsToPowerDown| 0.0)
    .run_on(OwnThread)
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("shot 1 does not stop");
    let (_, end) = send(&mut per_shot, &[0], &[Op::Stop]).expect("an open executor runs a call");
    end.expect_err("shot 0 stops");
    assert_eq!(
        freed.load(Ordering::SeqCst),
        0,
        "shot 0's state is kept until close"
    );
    let report = ends_soon(move || per_shot.close());
    assert_eq!(
        freed.load(Ordering::SeqCst),
        1,
        "close freed shot 0's state"
    );
    assert_eq!([0, 1].map(|j| report.log_p.get(j)), [None, Some(0.0)]);
}

/// A caller that keeps two calls on shot 0 in flight on a pool's threads,
/// the first of which fails: the second fails behind it without running,
/// but the executor accepted both, so its report counts both, where the
/// other order's counts only the call that reached its executor.
#[test]
fn its_report_counts_every_call_it_accepted_those_failed_behind_a_failure_included() {
    let (placement, jobs) = holding();
    let mut per_shot = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let shot = ShotSet::one(0);
    let replies: Vec<_> = [&[Op::Stop][..], &[Op::Flip]]
        .into_iter()
        .map(|instructions| {
            let reply = per_shot.call(InstructionBatchRef {
                shots: &shot,
                instructions,
            });
            reply.expect("the failure is not recorded yet")
        })
        .collect();
    (jobs.try_recv().expect("shot 0's first call hands a job"))();
    for reply in replies {
        ended(reply).1.expect_err("the first call failed");
    }
    let closing = thread::spawn(move || per_shot.close());
    // The close's wait for shot 0's calls, then the drop of its state, left
    // out of the report, are jobs: run until the close returns.
    while let Ok(job) = jobs.recv_timeout(PATIENT) {
        job();
    }
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    assert_eq!(report.calls, 2);
}

/// An executor placed, then called on shot 4, cannot be given a state per
/// shot: its state is on its placement's thread.
#[test]
#[should_panic(expected = "a state per shot is chosen before the first call")]
fn a_placed_executor_already_called_cannot_take_a_state_per_shot() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    let (_, end) = send(&mut placed, &[4], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let _ = placed.shots(OnePerShot);
}

/// An executor called on shot 4, then placed and given a state per shot:
/// the strategy continues it, with its state and its calls.
#[test]
fn a_state_per_shot_continues_the_executor_placed() {
    let mut one_state = recording(&Threads::default());
    let shot = ShotSet::one(4);
    let reply = one_state
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip],
        })
        .expect("an open executor runs a call");
    ended(reply).1.expect("the logic does not fail");
    let mut per_shot = one_state.run_on(OwnThread).shots(OnePerShot);
    let (outcomes, end) =
        send(&mut per_shot, &[4, 5], &[Op::Read(0)]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    assert_eq!(
        by_shot(&outcomes),
        [(4, 0, Value::One), (5, 0, Value::Zero)]
    );
    let report = ends_soon(move || per_shot.close());
    assert_eq!(report.calls, 2);
    assert_eq!(report.log_p.get(4), Some(1.0));
}

/// An executor called on shot 4, then placed on a pool and given a state
/// per shot; a caller does not poll the reply of a call whose logic
/// stops shot 4's device, the state the strategy continues: once the pool
/// ran it, a call on shot 5 alone is refused with that failure, as after
/// a failure of a shot made since.
#[test]
fn a_held_shots_failure_refuses_a_call_on_other_shots() {
    let mut one_state = recording(&Threads::default());
    let (_, end) = send(&mut one_state, &[4], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let (placement, jobs) = holding();
    let mut per_shot = one_state.run_on(placement).shots(OnePerShot);
    let _unpolled = call(&mut per_shot, &[4], &[Op::Stop]).expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 4's call hands a job"))();
    assert_eq!(
        call(&mut per_shot, &[5], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A wrapper never forgets a refusal: an executor closed, then placed and
/// given a state per shot, still refuses every call as closed. Chosen
/// because a wrapper continues what it wraps, failures included, as
/// iterator adapters and Tower's `Buffer` do.
#[test]
fn placed_then_given_a_state_per_shot_after_close_it_still_refuses() {
    let mut one_state = recording(&Threads::default());
    one_state.close();
    let mut per_shot = one_state.run_on(OwnThread).shots(OnePerShot);
    assert_eq!(
        send(&mut per_shot, &[0], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// A wrapper never forgets a refusal: an executor whose call failed, then
/// placed and given a state per shot, still refuses every call with the
/// failure, whatever shots it names. Chosen because a wrapper continues
/// what it wraps, failures included, as iterator adapters and Tower's
/// `Buffer` do. A device stops in a call on shot 0; a call on shot 1
/// follows.
#[test]
fn placed_then_given_a_state_per_shot_after_a_failure_it_still_refuses() {
    let mut one_state = recording(&Threads::default());
    let (_, end) = send(&mut one_state, &[0], &[Op::Stop]).expect("an open executor runs a call");
    end.expect_err("the device stops");
    let mut per_shot = one_state.run_on(OwnThread).shots(OnePerShot);
    assert_eq!(
        send(&mut per_shot, &[1], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A wrapper never forgets a refusal, in the other direction too: a placed
/// executor that failed on its own, before its first call reached the
/// executor it places, then given a state per shot, still refuses every
/// call with that failure. Chosen because a wrapper continues what it
/// wraps, failures included, as iterator adapters and Tower's `Buffer` do.
/// A batch whose copy panics inside the placed executor's `call` (the
/// executor it places never sees it); an empty batch follows.
#[test]
fn given_a_state_per_shot_a_placed_executor_keeps_its_own_refusal() {
    let mut placed = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, CopyPanics>, _: &mut Outcomes<'_>| Ok(()),
    )
    .run_on(OwnThread);
    let shot = ShotSet::one(0);
    panic::catch_unwind(AssertUnwindSafe(|| {
        placed
            .call(InstructionBatchRef {
                shots: &shot,
                instructions: &[CopyPanics],
            })
            .map(drop)
    }))
    .expect_err("the copy's panic reaches the caller");
    let mut per_shot = placed.shots(OnePerShot);
    let refused = per_shot
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[],
        })
        .err();
    assert_eq!(
        refused,
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// A chain is composed against what its executor offers wherever its
/// shots run: a device that states every limit it can, and claims
/// the log probability, gets the promise of the other order, whether or not it
/// is told how to read a state's.
#[test]
fn it_promises_what_the_other_order_does() {
    let threads = Threads::default();
    let promise = narrow_promise().with_log_probability(true);
    let without = || executor_fn(promise.clone(), bits(&threads), run_bit as RunBit);
    let with = || without().log_probability(bit_value as BitLogP);
    assert_eq!(
        without().run_on(OwnThread).shots(OnePerShot).capabilities(),
        without().shots(OnePerShot).capabilities()
    );
    assert_eq!(
        with().run_on(OwnThread).shots(OnePerShot).capabilities(),
        with().shots(OnePerShot).capabilities()
    );
}

/// Dropped while a call runs on a shot's placement, a simulator whose
/// shots run in parallel, `.run_on(..).shots(..)`, cancels the call queued
/// behind it on that shot, as one not given a state per shot does: it
/// never runs, and its reply ends `ReplyStatus::Cancelled` as the executor
/// is dropped. Chosen so that a dropped executor frees its states after the
/// call in progress at most, as dropping tokio's `JoinSet`, Java's
/// `shutdownNow` and Python's `shutdown(cancel_futures=True)` cancel the
/// work not yet started.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn dropped_it_cancels_the_calls_not_yet_started() {
    let (gate, device) = gated_device();
    let executor = device.run_on(OwnThread).shots(OnePerShot);
    let (_, queued) = dropped_while_a_call_runs(executor, &gate);
    assert_eq!(polled_now(queued), (Vec::new(), ReplyStatus::Cancelled));
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    (gate.freed.recv_timeout(PATIENT)).expect("the state is freed, after which nothing runs");
    assert!(
        gate.started.try_recv().is_err(),
        "the call queued never started"
    );
}

/// A caller on a pool that never runs a job calls shot 0, then shot 1,
/// and drops the executor with both replies pending: each reply ends
/// `Cancelled`, whichever is polled first, since neither call started and
/// a cancellation fails nothing.
#[test]
fn dropped_its_calls_not_yet_started_end_cancelled_in_either_poll_order() {
    for last_first in [false, true] {
        let (placement, _jobs) = holding();
        let mut executor = recording(&Threads::default())
            .run_on(placement)
            .shots(OnePerShot);
        let first = call(&mut executor, &[0], &[Op::Flip]).expect("an open executor queues a call");
        let second =
            call(&mut executor, &[1], &[Op::Flip]).expect("an open executor queues a call");
        drop(executor);
        let ends = if last_first {
            let second = polled_now(second).1;
            (polled_now(first).1, second)
        } else {
            (polled_now(first).1, polled_now(second).1)
        };
        assert_eq!(
            ends,
            (ReplyStatus::Cancelled, ReplyStatus::Cancelled),
            "polled last first: {last_first}"
        );
    }
}

/// A call on shots 0 and 1 whose device stops on the shot that runs: one
/// shot's job runs and fails, then the executor is dropped, which cancels
/// the other's. The reply ends `Failed` with the device's reason, whether
/// the failed shot comes first in the batch or last: a failure outranks a
/// cancellation. Chosen because a failure is news the caller must report,
/// and it refuses later calls, where a cancellation only says that a shot
/// never ran; so the end does not depend on the order in which the parts
/// are polled, and no failure is hidden behind a cancellation.
#[test]
fn a_failed_shot_outranks_a_cancelled_one() {
    for failed in [0, 1] {
        let (placement, jobs) = holding();
        let mut executor = recording(&Threads::default())
            .run_on(placement)
            .shots(OnePerShot);
        let reply =
            call(&mut executor, &[0, 1], &[Op::Stop]).expect("an open executor queues a call");
        let mut shots_jobs: Vec<PlacementJob> = jobs.try_iter().collect();
        assert_eq!(shots_jobs.len(), 2, "each shot's first call hands a job");
        (shots_jobs.remove(failed))();
        drop(executor);
        assert_eq!(
            polled_now(reply).1,
            ReplyStatus::Failed("the device stopped".to_owned()),
            "shot {failed} failed"
        );
        for job in shots_jobs {
            job();
        }
    }
}

/// A cancelled call fails no other call: dropped with two calls queued on
/// a shot, neither started, a simulator whose shots run in parallel ends
/// each one's reply `ReplyStatus::Cancelled`, the second's too, though the
/// first's was seen first, not failed by "an earlier call failed". Chosen
/// so that a caller tells every call it abandoned from one that failed,
/// since dropping cancels the work not yet started (tokio's `JoinSet`,
/// Java's `shutdownNow`, Python's `cancel_futures`), which did not fail.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn a_cancelled_call_fails_no_later_call() {
    let (placement, jobs) = holding();
    let mut executor = recording(&Threads::default())
        .run_on(placement)
        .shots(OnePerShot);
    let shot = ShotSet::one(0);
    let mut replies = [Op::Read(0), Op::Read(1)].map(|op| {
        executor
            .call(InstructionBatchRef {
                shots: &shot,
                instructions: &[op],
            })
            .expect("an open executor queues a call")
    });
    drop(executor);
    let mut out = Vec::new();
    for reply in &mut replies {
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Cancelled
        );
    }
    for job in jobs.try_iter() {
        job();
    }
    assert!(out.is_empty(), "neither call ran");
}

/// Dropped with a call queued on each of two shots, awaited by a task
/// whose waker panics, a simulator whose shots run in parallel,
/// `.run_on(..).shots(..)`, cancels the call on both shots, and `drop`
/// returns: each shot's executor absorbs the waker's panic, the caller's
/// bug, so none leaves `drop`; one that left a shot's `drop` would unwind
/// through the next shot's, which aborts the process when it panics too
/// (C-DTOR-FAIL). A regression aborts the test binary rather than failing
/// this test.
#[test]
fn dropped_with_a_callers_waker_that_panics_it_cancels_every_shots_call() {
    let (gate, device) = gated_device();
    let mut executor = device.run_on(OwnThread).shots(OnePerShot);
    let running = call(&mut executor, &[0, 1], &[Op::Flip, Op::Read(0)])
        .expect("an open executor queues a call");
    for _ in 0..2 {
        (gate.started.recv_timeout(PATIENT)).expect("each shot's first call starts");
    }
    let mut queued = call(&mut executor, &[0, 1], &[Op::Flip, Op::Read(1)])
        .expect("an open executor queues a call");
    let panicking = Waker::from(Arc::new(PanicsWhenWoken));
    assert_eq!(
        queued.poll_events(&mut Vec::new(), &panicking),
        ReplyStatus::Pending,
        "queued behind the calls running"
    );
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("`drop` raises no caller's panic");
    assert_eq!(polled_now(queued), (Vec::new(), ReplyStatus::Cancelled));
    for _ in 0..2 {
        gate.open
            .send(())
            .expect("each shot's call running waits at the gate");
    }
    ended(running).1.expect("the calls running finish");
}

/// A device whose states fail to power down, and whose logic panics
/// (two bugs), placed whole on a thread of its own, runs a call on two new
/// shots: the logic's panic stays in the call's job, which fails the
/// reply, and the states, freed as it unwinds, leave theirs to the panic
/// hook, rather than abort the process from the placement's thread.
#[test]
fn placed_whole_a_logics_panic_fails_the_reply_though_its_new_states_drops_panic_too() {
    let freed = Freed::default();
    let mut placed = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        panics,
    )
    .shots(OnePerShot)
    .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose states fail to power down, placed whole on a thread of
/// its own, stops in a call on two new shots: the call frees both states
/// on the placement, where the first drop's panic stays in the call's job,
/// which fails the reply, rather than abort the process.
#[test]
fn placed_whole_a_failed_call_frees_every_state_made_for_it_and_fails_its_reply() {
    let freed = Freed::default();
    let mut placed = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        stops,
    )
    .shots(OnePerShot)
    .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the device stops");
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose states fail to power down, placed whole on a thread of
/// its own, ran a call on shots 0 and 1: `close` frees both on the
/// placement, then the first one's panic reaches the closer, its payload
/// unchanged.
#[test]
fn placed_whole_close_frees_every_state_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut placed = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        runs,
    )
    .shots(OnePerShot)
    .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| placed.close()))
            .map(drop)
            .expect_err("the states' drops panic")
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose shots each run on a thread of their own, whose states
/// fail to power down, has no room for shot 2: a call on shots 0, 1 and 2
/// frees the states it made for shots 0 and 1, and the first one's panic
/// reaches the caller once both are freed, rather than abort the process.
#[test]
fn a_refused_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, Some(2)),
        runs,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).map(drop)
    }))
    .expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// What a closure of a device whose copy has a bug captures (its logic,
/// say): its `Clone` panics.
struct CopyHasABug;

impl Clone for CopyHasABug {
    fn clone(&self) -> Self {
        panic!("the copy has a bug")
    }
}

/// A device whose shots each run on a thread of their own, whose state
/// fails to power down, and whose logic's copy panics (two bugs): each
/// shot's copy of the logic is made before its state, so the copy's
/// panic reaches the caller with no state made, rather than drop a state
/// as it unwinds, whose drop's panic would abort the process.
#[test]
fn a_shots_copy_of_the_logic_that_panics_reaches_the_caller_before_its_state_is_made() {
    let freed = Freed::default();
    let bug = CopyHasABug;
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        move |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let CopyHasABug = &bug;
            Ok(())
        },
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0], &[Op::Flip]).map(drop)
    }))
    .expect_err("the logic's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(freed_so_far(&freed), [], "no state was made");
}

/// A device whose shots each run on a thread of their own, whose states
/// fail to power down, and whose logic panics (two bugs), runs a call on
/// shots 0, 1 and 2: each panic stops its shot's placement, so `close`
/// frees the states, left out of the report, on the closer's thread, then
/// raises the first one's panic, once all three are freed. The logics
/// meet at a barrier before they panic, so that every shot gets the call
/// (the reply ends at the first shot's failure), and the test waits for
/// every job to end before `close`, so that every placement has stopped.
#[test]
fn close_frees_the_states_of_stopped_placements_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let barrier = Arc::new(Barrier::new(3));
    let jobs = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::clone(&jobs);
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        move |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            barrier.wait();
            panic!("the device's driver panics")
        },
    )
    .run_on(move |job: PlacementJob| {
        (spawned.lock().expect("nothing panics holding the lock")).push(thread::spawn(job));
    })
    .shots(OnePerShot);
    let (_, end) =
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let jobs = mem::take(&mut *jobs.lock().expect("nothing panics holding the lock"));
    assert_eq!(jobs.len(), 3, "each shot's placement got the call's job");
    for job in jobs {
        job.join().expect("the job keeps the logic's panic");
    }
    let panicked = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect_err("the states' drops panic")
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1, 2]);
}

/// The same device and call, its three placements stopped, is dropped
/// unclosed: `drop` frees the three states and raises no panic of their
/// drops, as a placed executor's `drop` never panics; only `close` raises
/// it.
#[test]
fn once_its_placements_stopped_its_drop_frees_the_states_raising_no_panic_of_their_drops() {
    let freed = Freed::default();
    let barrier = Arc::new(Barrier::new(3));
    let jobs = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::clone(&jobs);
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        move |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            barrier.wait();
            panic!("the device's driver panics")
        },
    )
    .run_on(move |job: PlacementJob| {
        (spawned.lock().expect("nothing panics holding the lock")).push(thread::spawn(job));
    })
    .shots(OnePerShot);
    let (_, end) =
        send(&mut per_shot, &[0, 1, 2], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let jobs = mem::take(&mut *jobs.lock().expect("nothing panics holding the lock"));
    assert_eq!(jobs.len(), 3, "each shot's placement got the call's job");
    for job in jobs {
        job.join().expect("the job keeps the logic's panic");
    }
    panic::catch_unwind(AssertUnwindSafe(|| drop(per_shot)))
        .expect("`drop` raises no panic of a state's drop");
    let mut freed = freed_so_far(&freed);
    freed.sort_unstable();
    assert_eq!(freed, [0, 1, 2]);
}

/// A device whose shot runs on a thread of its own, whose state fails to
/// power down, and whose logic panics (two bugs), runs a call: the panic
/// stops the shot's placement, whose job may still be ending, holding the
/// queue, when `close` is called. `close` frees the state on the closer's
/// thread, not with the last of the job, and raises its drop's panic
/// before it returns. Whether the job ended is a race the test leaves
/// open; a pause at the end of the job makes it fail every time without
/// the rule.
#[test]
fn close_frees_a_stopped_placements_state_before_it_returns_though_its_job_is_still_ending() {
    let freed = Freed::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            panic!("the device's driver panics")
        },
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let panicked = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect_err("the state's drop panics")
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose shots each run on a thread of their own, whose states
/// fail to power down, ran a call on four shots: `close` frees each
/// state it reports on its placement, and raises the first one's panic,
/// shot 0's, once the other shots are freed too.
#[test]
fn close_raises_the_first_reported_states_drop_panic_once_every_shot_is_freed() {
    let freed = Freed::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        runs,
    )
    .run_on(OwnThread)
    .shots(OnePerShot);
    let (_, end) =
        send(&mut per_shot, &[0, 1, 2, 3], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect_err("the states' drops panic")
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1, 2, 3]);
}

/// A device whose state fails to power down ran a call on shot 0 before
/// its shots were each placed on a thread of their own, so it holds that
/// state where its calls ran; its logic then panics (two bugs) in a
/// call on shot 0, which stops that shot's placement: `close` frees the
/// state on the closer's thread and raises its drop's panic, as for a
/// state made once placed.
#[test]
fn once_stopped_close_frees_a_state_held_from_before_it_was_placed_then_raises_its_drops_panic() {
    let freed = Freed::default();
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        |_: &mut PowersDownBadly, input: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            assert!(
                !input.instructions().contains(&Op::Panic),
                "the device's driver panics"
            );
            Ok(())
        },
    );
    let (_, end) = send(&mut device, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let mut per_shot = device.run_on(OwnThread).shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Panic]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let panicked = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect_err("the state's drop panics")
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose states fail to power down, its shots each placed by a
/// placement that runs each job as it is handed (inline), and whose
/// logic stops, runs a call on new shots 0 and 1: shot 0 fails as it is
/// handed the call, which ends there, so the state made for shot 1 never
/// gets it, and `call` frees that state there, raising its drop's panic,
/// as for a refused call.
#[test]
fn a_call_whose_shot_fails_frees_the_states_of_its_unstarted_shots_then_raises_the_first_drops_panic()
 {
    let freed = Freed::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        stops,
    )
    .run_on(|job: PlacementJob| job())
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0, 1], &[Op::Flip]).map(drop)
    }))
    .expect_err("the unstarted state's drop panics");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(1)));
    assert_eq!(freed_so_far(&freed), [1]);
}

/// The same device and call: a later call is refused with the reason of
/// the logic that failed first, not as after a panic of `call`, since
/// the failure came before the drop's panic.
#[test]
fn after_a_shots_failure_and_an_unstarted_states_drop_panic_later_calls_get_the_logics_reason() {
    let freed = Freed::default();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        stops,
    )
    .run_on(|job: PlacementJob| job())
    .shots(OnePerShot);
    panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0, 1], &[Op::Flip]).map(drop)
    }))
    .expect_err("the unstarted state's drop panics");
    assert_eq!(
        send(&mut per_shot, &[0], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A device whose shots each run through a copy of a placement that runs
/// each job as it is handed (inline), whose logic and placement each
/// hold a handle that fails to release (a device driver's bug), shared by their
/// copies, ran a call on shots 0 and 1, and is dropped unclosed: its drop
/// releases both handles, each in a catch of its own, and raises neither
/// panic.
#[test]
fn dropped_it_frees_what_its_closures_and_placement_capture_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let placing = Arc::new(handle.another());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        own_bit,
        holding_a_handle(Arc::new(handle)),
    )
    .run_on(move |job: PlacementJob| {
        let _held = &placing;
        job();
    })
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(per_shot)))
        .expect("`drop` raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 2);
}

/// A device whose every closure holds a handle on its connection ran a
/// call on shot 0 before its shots were each placed on a thread of their
/// own, so it holds that session where its calls ran; dropped unclosed
/// before any call there, it frees that session first, before any closure
/// closes the connection, as `close` does.
#[test]
fn dropped_it_frees_a_state_held_from_before_it_was_placed_before_its_closures() {
    let connection = Connection::default();
    let mut device = connected_device(&connection);
    let (_, end) = send(&mut device, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let per_shot = device.run_on(OwnThread).shots(OnePerShot);
    drop(per_shot);
    assert_eq!(connection.logged(), ["released shot 0"]);
}

/// A device whose every closure holds a handle on its connection, reached
/// through a context per thread, runs its shots each on a thread of its
/// own, with copies of the logic, log probability and report maker; it ran
/// a call on shots 0 and 1, and is dropped unclosed: on each shot's
/// thread, the session is dropped first, before the shot's copies of the
/// closures close that thread's context.
#[test]
fn dropped_each_shot_frees_its_state_before_its_copies_of_the_closures() {
    let connection = Connection::default();
    let mut per_shot = connected_device(&connection)
        .run_on(OwnThread)
        .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    drop(per_shot);
    // `drop` does not wait for the shots' threads, which drop their states.
    let deadline = Instant::now() + PATIENT;
    while connection.logged().len() < 2 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    let mut ended = connection.logged();
    ended.sort();
    assert_eq!(ended, ["released shot 0", "released shot 1"]);
}

/// A device whose every closure holds a handle on its connection, reached
/// through a context per thread, runs its shots on a placement that holds
/// each job until the test runs it; it ran a call on shot 0, and is
/// dropped unclosed: `drop` returns without waiting for the shot's state,
/// whose drop is a job still held, so the executor's own closures are
/// freed first, on the dropping thread; the shot's session, freed there
/// afterwards, finds that thread's context closed and leaks (`close`,
/// which waits, would have freed it first).
#[test]
fn dropped_it_frees_its_own_closures_without_waiting_for_its_shots_states() {
    let connection = Connection::default();
    let (placement, jobs) = holding();
    let mut per_shot = connected_device(&connection)
        .run_on(placement)
        .shots(OnePerShot);
    let reply = per_shot
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip],
        })
        .expect("an open executor queues a call");
    (jobs.try_recv().expect("shot 0's call hands a job"))();
    ended(reply).1.expect("the logic runs");
    let logged = ends_soon(move || {
        drop(per_shot);
        for job in jobs.try_iter() {
            job();
        }
        connection.logged()
    });
    assert_eq!(logged, ["shot 0 leaked"]);
}

/// What a closure of a device, or its placement, captures: a handle on
/// the device's connection that fails to release (a device driver's bug), each
/// copy a handle of its own, so that each copy's drop panics.
struct HandlePerCopy(ReleasesBadly);

impl Clone for HandlePerCopy {
    fn clone(&self) -> Self {
        Self(self.0.another())
    }
}

/// A device whose logic, log probability and report maker each hold a
/// handle that fails to release, a handle per copy, runs shot 0 on a
/// thread of its own; its logic panics (two bugs), which stops the
/// shot's placement: `close` frees the shot's copies of the three
/// closures, each in a catch of its own, and raises none of their panics,
/// as `drop` does.
#[test]
fn once_stopped_close_frees_the_shots_copies_of_the_closures_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let logics_handle = HandlePerCopy(handle.another());
    let (reading, reporting) = (HandlePerCopy(handle.another()), HandlePerCopy(handle));
    let mut per_shot = executor_fn(
        Capabilities::default(),
        own_bit,
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            panic!("the device's driver panics")
        },
    )
    .log_probability(move |_: &Bit| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Bit| {
        let _held = &reporting;
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let per_shot = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect("`close` raises no panic of a capture's drop");
        per_shot
    });
    assert_eq!(
        released.load(Ordering::SeqCst),
        3,
        "the shot's three copies"
    );
    drop(per_shot);
}

/// A device whose logic and log probability each hold a handle that
/// fails to release, a handle per copy, and whose report maker's copy
/// panics (two bugs), runs shot 0 on a thread of its own: the report
/// maker's copy is made after the others, as `call` makes the shot's
/// executor, so its panic reaches the caller, and the copies made before
/// it are freed as it unwinds, each in a catch of its own, rather than
/// abort the process.
#[test]
fn a_shots_copy_that_panics_frees_the_copies_made_before_it_raising_its_own_panic() {
    let (handle, released) = ReleasesBadly::new();
    let (logics_handle, reading) = (HandlePerCopy(handle.another()), HandlePerCopy(handle));
    let bug = CopyHasABug;
    let mut per_shot = executor_fn(
        Capabilities::default(),
        own_bit,
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            Ok(())
        },
    )
    .log_probability(move |_: &Bit| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Bit| {
        let CopyHasABug = &bug;
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0], &[Op::Flip]).map(drop)
    }))
    .expect_err("the report maker's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(
        released.load(Ordering::SeqCst),
        2,
        "the logic's and the log probability's copies"
    );
    drop(per_shot);
}

/// A device whose logic, log probability and report maker each hold a
/// handle that fails to release, a handle per copy, and whose state
/// closure panics (two bugs), runs shot 0 on a thread of its own: the
/// shot's copies of the three closures are made before its state, so the
/// state closure's panic reaches the caller, and the copies are freed as
/// it unwinds, each in a catch of its own, rather than abort the process.
#[test]
fn a_state_closure_that_panics_frees_the_shots_copies_raising_its_own_panic() {
    let (handle, released) = ReleasesBadly::new();
    let logics_handle = HandlePerCopy(handle.another());
    let (reading, reporting) = (HandlePerCopy(handle.another()), HandlePerCopy(handle));
    let mut per_shot = executor_fn(
        Capabilities::default(),
        |_: ShotContext| -> Result<Bit, Rejected> { panic!("the device fails to power up") },
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            Ok(())
        },
    )
    .log_probability(move |_: &Bit| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Bit| {
        let _held = &reporting;
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0], &[Op::Flip]).map(drop)
    }))
    .expect_err("the state closure panics");
    assert_eq!(panic_text(&*panicked), Some("the device fails to power up"));
    assert_eq!(
        released.load(Ordering::SeqCst),
        3,
        "the shot's three copies"
    );
    drop(per_shot);
}

/// A device whose logic, log probability and report maker each hold a
/// handle that fails to release, a handle per copy, and whose state
/// closure refuses (the device is busy), runs shot 0: the shot's copies of
/// the three closures, made before its state, are freed as the call is
/// refused, each in a catch of its own, raising none of their panics: the
/// caller gets the refusal.
#[test]
fn a_state_closures_refusal_frees_the_shots_copies_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let logics_handle = HandlePerCopy(handle.another());
    let (reading, reporting) = (HandlePerCopy(handle.another()), HandlePerCopy(handle));
    let busy = || Rejected::Unavailable("the device is busy".to_owned());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        move |_: ShotContext| -> Result<Bit, Rejected> { Err(busy()) },
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            Ok(())
        },
    )
    .log_probability(move |_: &Bit| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Bit| {
        let _held = &reporting;
    })
    .run_on(OwnThread)
    .shots(OnePerShot);
    let refused = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0], &[Op::Flip]).map(drop)
    }))
    .expect("a refusal raises no panic of a copy's drop");
    assert_eq!(refused, Err(busy()));
    assert_eq!(
        released.load(Ordering::SeqCst),
        3,
        "the shot's three copies"
    );
    drop(per_shot);
}

/// A device whose state fails to power down, and whose placement's copy
/// panics (two bugs), runs shot 0: the shot's state is made, then its
/// copy of the placement, so the copy's panic reaches the caller, and the
/// state, held by the shot's executor, is freed as it unwinds, its drop's
/// panic absorbed, rather than abort the process.
#[test]
fn a_shots_copy_of_the_placement_that_panics_frees_its_state_raising_its_own_panic() {
    let freed = Freed::default();
    let bug = CopyHasABug;
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| Ok(()),
    )
    .run_on(move |job: PlacementJob| {
        let CopyHasABug = &bug;
        job();
    })
    .shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut per_shot, &[0], &[Op::Flip]).map(drop)
    }))
    .expect_err("the placement's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down, and whose logic's copy
/// panics (two bugs), ran a call on shot 0 before its shots were each
/// placed on a thread of their own, so it holds that state: placing its
/// shots copies the logic for shot 0 once the state is held by the
/// executor being placed, so the copy's panic reaches the caller, and the
/// state is freed as it unwinds, its drop's panic absorbed, rather than
/// abort the process.
#[test]
fn a_logic_copy_that_panics_as_its_shots_are_placed_frees_the_state_held_from_before_raising_its_own_panic()
 {
    let freed = Freed::default();
    let bug = CopyHasABug;
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        move |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let CopyHasABug = &bug;
            Ok(())
        },
    );
    let (_, end) = send(&mut device, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let placing = device.run_on(OwnThread);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        drop(placing.shots(OnePerShot));
    }))
    .expect_err("the logic's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down, and whose placement's copy
/// panics (two bugs), ran a call on shot 0 before its shots were each
/// placed through a copy of it, so it holds that state: placing its shots
/// copies the placement for shot 0 once the state is held by the shot's
/// executor, so the copy's panic reaches the caller, and the state is
/// freed as it unwinds, its drop's panic absorbed, rather than abort the
/// process.
#[test]
fn a_placement_copy_that_panics_as_its_shots_are_placed_frees_the_state_held_from_before_raising_its_own_panic()
 {
    let freed = Freed::default();
    let bug = CopyHasABug;
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| Ok(()),
    );
    let (_, end) = send(&mut device, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let placing = device.run_on(move |job: PlacementJob| {
        let CopyHasABug = &bug;
        job();
    });
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        drop(placing.shots(OnePerShot));
    }))
    .expect_err("the placement's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down, whose placement holds a
/// handle that fails to release, a handle per copy, and whose logic's
/// copy panics (three bugs), ran a call on shot 0 before its shots were
/// each placed through a copy of that placement, so it holds that state:
/// placing its shots copies the logic for shot 0, so the copy's panic
/// reaches the caller, and the placement is freed as it unwinds, with the
/// state, each drop's panic absorbed, rather than abort the process. The
/// placement is freed here only as a panic unwinds (otherwise the executor
/// placed per shot holds it), so a placement held bare can fail this test
/// only by aborting the process.
#[test]
fn a_logic_copy_that_panics_as_its_shots_are_placed_frees_the_placement_raising_its_own_panic() {
    let freed = Freed::default();
    let bug = CopyHasABug;
    let (handle, released) = ReleasesBadly::new();
    let placing = HandlePerCopy(handle);
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        move |_: &mut PowersDownBadly, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let CopyHasABug = &bug;
            Ok(())
        },
    );
    let (_, end) = send(&mut device, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let placing = device.run_on(move |job: PlacementJob| {
        let _held = &placing;
        job();
    });
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        drop(placing.shots(OnePerShot));
    }))
    .expect_err("the logic's copy panics");
    assert_eq!(panic_text(&*panicked), Some("the copy has a bug"));
    assert_eq!(freed_so_far(&freed), [0]);
    assert_eq!(released.load(Ordering::SeqCst), 1, "the placement");
}

/// A device whose logic holds a handle that fails to release, a handle
/// per copy, and whose shots each run through a copy of a placement that
/// runs each job as it is handed (inline) and holds such a handle too
/// (a device driver's bug), runs shot 0, then closes: `close` reports the shot,
/// whose placement frees its copy of the logic (here inline, before
/// `close` returns), and `close` frees that placement, each in a catch of
/// its own, raising none of their panics.
#[test]
fn a_reported_shots_copies_and_placement_are_freed_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let (logics_handle, placing) = (HandlePerCopy(handle.another()), HandlePerCopy(handle));
    let mut per_shot = executor_fn(
        Capabilities::default(),
        own_bit,
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            Ok(())
        },
    )
    .run_on(move |job: PlacementJob| {
        let _held = &placing;
        job();
    })
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let report = panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
        .expect("`close` raises no panic of a capture's drop");
    assert_eq!(report.calls, 1, "the shot is reported");
    assert_eq!(
        released.load(Ordering::SeqCst),
        2,
        "the shot's copies of the logic and of the placement"
    );
    drop(per_shot);
}

/// A placement that hands each job to a thread of its own, holding a
/// handle that fails to release, a handle per copy: each shot's placement
/// holds a handle of its own.
fn thread_per_job_holding(
    handle: HandlePerCopy,
) -> impl FnMut(PlacementJob) + Send + Clone + 'static {
    move |job| {
        let _held = &handle;
        thread::spawn(job);
    }
}

/// A device whose logic and shot's placement each hold a handle that
/// fails to release, a handle per copy, and whose logic stops, runs
/// shot 0: the call fails, and `close` frees the state left out of the
/// report, with the shot's copy of the logic, waiting for its placement
/// to drop them, then frees that placement, raising no panic of their
/// drops.
#[test]
fn close_frees_the_copies_and_placement_of_a_shot_left_out_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let logics_handle = HandlePerCopy(handle.another());
    let mut per_shot = executor_fn(
        Capabilities::default(),
        own_bit,
        move |_: &mut Bit, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &logics_handle;
            Err("the device stopped".to_owned())
        },
    )
    .run_on(thread_per_job_holding(HandlePerCopy(handle)))
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic stops");
    let per_shot = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect("`close` raises no panic of a capture's drop");
        per_shot
    });
    assert_eq!(
        released.load(Ordering::SeqCst),
        2,
        "the shot's copies of the logic and of the placement"
    );
    drop(per_shot);
}

/// A device whose state fails to power down, whose shot's placement holds
/// a handle that fails to release, a handle per copy, and whose logic
/// panics (three bugs), runs shot 0: the panic stops the shot's placement,
/// and `close` frees the state on the closer's thread, then the placement
/// as that state's panic unwinds, its drop's panic absorbed: the state's
/// panic reaches the closer, rather than abort the process.
#[test]
fn once_stopped_close_raises_the_states_drop_panic_though_its_placements_drop_panics_too() {
    let freed = Freed::default();
    let (handle, released) = ReleasesBadly::new();
    let mut per_shot = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        panics,
    )
    .run_on(thread_per_job_holding(HandlePerCopy(handle)))
    .shots(OnePerShot);
    let (_, end) = send(&mut per_shot, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect_err("the logic panics");
    let (panicked, per_shot) = ends_soon(move || {
        let panicked = panic::catch_unwind(AssertUnwindSafe(|| per_shot.close()))
            .map(drop)
            .expect_err("the state's drop panics");
        (panicked, per_shot)
    });
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
    assert_eq!(released.load(Ordering::SeqCst), 1, "the shot's placement");
    drop(per_shot);
}
