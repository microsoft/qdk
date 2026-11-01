// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a reply over shots run on placements of their own costs its caller
//! when the shots end one at a time: each poll of a [`ShotReplies`] polls
//! every part still pending, so a caller woken once per shot that ends
//! polls O(n²) parts for a batch of n shots.
//!
//! The executor is a bit per shot, read once: its own work is a few
//! nanoseconds, so the reply's cost is most of the time. It is called with
//! one batch of 1, 16, 256 and 1024 shots, on a placement that keeps each
//! job so that the bench runs them itself, on its own thread, in order,
//! with no thread switch in the time. It reports nanoseconds per shot, the
//! fastest of 7 runs, for the call, the jobs and the polls (the executor is
//! made, and dropped, outside the time), and each column's ratio to the
//! first.
//!
//! The columns:
//!
//! - **polled once**: `.run_on(place).shots(OnePerShot)`, each shot
//!   on a placement of its own; the bench runs every shot's job, then polls
//!   the reply once: the floor, one poll of each part.
//! - **polled per shot**: the same executor; the bench polls the reply
//!   after each shot's job, as a caller woken by each shot that ends: the
//!   cost of the wakes.
//! - **one placement**: `.shots(OnePerShot).run_on(place)`, every
//!   shot in the one job; the bench runs it, then polls the reply once.
//!
//! Before any timing, it asserts that the three columns give the same
//! outcomes and end their replies. It asserts no timing: the numbers
//! depend on the machine and its load, so compare the columns of one run,
//! not runs on different machines.
//!
//! Run it pinned to one core where the platform allows (on Linux,
//! `taskset -c <core>` before the command):
//! `cargo bench -p executor_protocol --bench shot_replies_wake`.

use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;
use std::time::{Duration, Instant};

use executor_protocol::{
    Capabilities, Executor, InstructionBatchRef, OnePerShot, Outcome, Outcomes, PlacementJob,
    ProgramInput, Rejected, Reply, ReplyStatus, ShotContext, ShotId, ShotOutcome, ShotReplies,
    ShotSet, TypedInstruction, executor_fn,
};

/// The runs of each measurement; the fastest is reported.
const RUNS: usize = 7;

/// The instruction of the bench's executor: read the shot's bit into the
/// readout with this tag.
#[derive(Clone, Copy, Debug)]
struct Read(u32);

impl TypedInstruction for Read {}

/// A shot's state: a bit, the parity of the shot.
struct Bit {
    one: bool,
}

/// The bit of the shot of `context`: one on the odd shots.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the state closure of executor_fn returns a Result"
)]
fn bit(context: ShotContext) -> Result<Bit, Rejected> {
    Ok(Bit {
        one: context.shot() % 2 == 1,
    })
}

/// Runs a batch on a shot's bit: each `Read` gives the bit.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the program of executor_fn returns a Result"
)]
fn read(
    bit: &mut Bit,
    input: &ProgramInput<'_, Read>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for &Read(tag) in input.instructions() {
        outcomes.push(Outcome::new(tag, bit.one.into()));
    }
    Ok(())
}

/// The jobs a placement was handed, for the bench to run.
type Jobs = Arc<Mutex<Vec<PlacementJob>>>;

/// A placement that keeps each job in `jobs`.
fn keep(jobs: &Jobs) -> impl FnMut(PlacementJob) + Clone + Send + 'static {
    let jobs = Arc::clone(jobs);
    move |job| {
        jobs.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(job);
    }
}

/// The jobs kept so far, in the order they were handed over.
fn take(jobs: &Jobs) -> Vec<PlacementJob> {
    std::mem::take(&mut *jobs.lock().unwrap_or_else(PoisonError::into_inner))
}

/// How a column runs its jobs and polls its reply.
#[derive(Clone, Copy)]
enum Column {
    PolledOnce,
    PolledPerShot,
    OnePlacement,
}

const COLUMNS: [(Column, &str); 3] = [
    (Column::PolledOnce, "polled once"),
    (Column::PolledPerShot, "polled per shot"),
    (Column::OnePlacement, "one placement"),
];

/// One run of `column` on a batch of `shots`: its outcomes and the time of
/// the call, the jobs and the polls.
fn run(column: Column, shots: &ShotSet) -> (Vec<ShotOutcome>, Duration) {
    let jobs = Jobs::default();
    let executor = executor_fn(Capabilities::default(), bit, read);
    let batch = InstructionBatchRef {
        shots,
        instructions: &[Read(0)],
    };
    let mut out = Vec::new();
    let elapsed = match column {
        Column::PolledOnce | Column::PolledPerShot => {
            let mut per_shot = executor.run_on(keep(&jobs)).shots(OnePerShot);
            let start = Instant::now();
            let mut reply = per_shot.call(batch).expect("an open executor runs a call");
            let polled_per_shot = matches!(column, Column::PolledPerShot);
            run_jobs(&jobs, &mut reply, &mut out, polled_per_shot);
            let end = reply.poll_events(&mut out, Waker::noop());
            let elapsed = start.elapsed();
            assert_eq!(end, ReplyStatus::Done, "every job ran");
            elapsed
        }
        Column::OnePlacement => {
            let mut placed = executor.shots(OnePerShot).run_on(keep(&jobs));
            let start = Instant::now();
            let mut reply = placed.call(batch).expect("an open executor runs a call");
            for job in take(&jobs) {
                job();
            }
            let end = reply.poll_events(&mut out, Waker::noop());
            let elapsed = start.elapsed();
            assert_eq!(end, ReplyStatus::Done, "every job ran");
            elapsed
        }
    };
    (out, elapsed)
}

/// Runs the jobs kept in `jobs`; when `polled_per_shot`, polls `reply`
/// after each, as a caller woken by each shot that ends.
fn run_jobs<R: Reply<Item = ShotOutcome>>(
    jobs: &Jobs,
    reply: &mut ShotReplies<R>,
    out: &mut Vec<ShotOutcome>,
    polled_per_shot: bool,
) {
    for job in take(jobs) {
        job();
        if polled_per_shot {
            let status = reply.poll_events(out, Waker::noop());
            assert!(!matches!(status, ReplyStatus::Failed(_)), "no shot fails");
        }
    }
}

/// `outcomes` by shot, to compare columns whose shots interleave.
fn by_shot(mut outcomes: Vec<ShotOutcome>) -> Vec<ShotOutcome> {
    outcomes.sort_by_key(|outcome| (outcome.shot, outcome.tag));
    outcomes
}

fn main() {
    let sizes: [ShotId; 4] = [1, 16, 256, 1024];
    for &n in &sizes {
        let shots = ShotSet::new((0..n).collect()).expect("distinct shots");
        let expected: Vec<ShotOutcome> = (0..n)
            .map(|shot| ShotOutcome::new(shot, 0, (shot % 2 == 1).into()))
            .collect();
        for (column, name) in COLUMNS {
            let (outcomes, _) = run(column, &shots);
            assert_eq!(by_shot(outcomes), expected, "{name} on {n} shots");
        }
    }

    println!("ns per shot, the fastest of {RUNS} runs; ratio to polled once");
    print!("{:>6}", "shots");
    for (_, name) in COLUMNS {
        print!(" {name:>16} {:>8}", "ratio");
    }
    println!();
    for n in sizes {
        let shots = ShotSet::new((0..n).collect()).expect("distinct shots");
        print!("{n:>6}");
        let mut first = None;
        for (column, _) in COLUMNS {
            let fastest = (0..RUNS)
                .map(|_| run(column, &shots).1)
                .min()
                .expect("at least one run");
            let per_shot = fastest.as_secs_f64() * 1e9 / f64::from(n);
            let base = *first.get_or_insert(per_shot);
            print!(" {per_shot:>16.1} {:>7.2}x", per_shot / base);
        }
        println!();
    }
}
