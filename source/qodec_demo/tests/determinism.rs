//! The same seed gives the same run, as the chain is put together and its
//! run driven. c4c6's profiled chain,
//! `static_driver() >> top >> middle >> profiler() >> coins`, gives the same
//! outcomes, statuses, profiler totals and device report with its layers,
//! its layers and profiler, or its profiler and coins composed first
//! (QD6), and with its run awaited rather than waited. Several runs
//! awaited together on one thread each give the outcomes and statuses
//! they give alone. The same run over a backend that answers later, or on
//! a thread of its own, is in `tests/answers_later.rs`. Each test is
//! bounded in time, so that a hang fails.
//!
//! A Qodec layer serves each upper call within it (QD1, mode (c)), so a
//! run yields only while a reply of a call that reads nothing (a prepare,
//! sent below) is pending. Repetition3's two runs are over a simulator on
//! a thread of its own, held at a gate until every run was polled once, so
//! that their prepares are pending when first polled: the test checks that
//! a run yielded, so that the runs interleave.

mod common;

use std::future::{Future, IntoFuture, poll_fn};
use std::pin::Pin;

use executor_protocol::{
    ExecutorPlacement, ExecutorReport, Needs, OwnThread, PlacementJob, ShotId, Value, static_driver,
};
use qodec_demo::coin::coin_device;
use qodec_demo::noisy_bits::simulator;
use qodec_demo::profiler::{Profile, profiler};
use qodec_demo::qodec_layer::{QodecLayer, Statuses};

use common::devices::Gate;
use common::waiting::{bounded, join_all};
use common::{
    CODE_SHOTS, DRAWS_FLIP, SEED, SHOTS, bottom, c4c6, c4c6_program, c422, c422_program,
    repetition3, repetition3_program, shot_set, sorted, stim_rz,
};

/// What a run of c4c6's profiled chain gave: its outcomes, by shot and
/// tag; each layer's statuses, the top's then the middle's; the
/// profiler's totals; and what the coins ran.
#[derive(Debug, PartialEq)]
struct Ran {
    outcomes: Vec<(ShotId, u32, Value)>,
    top: Statuses,
    middle: Statuses,
    profile: Profile,
    device: ExecutorReport<Profile>,
}

/// The report of c4c6's profiled chain, layers first.
type Report = (Statuses, (Statuses, (Profile, ExecutorReport<Profile>)));

impl Ran {
    fn new(
        outcomes: Result<
            Vec<executor_protocol::ShotOutcome>,
            executor_protocol::StaticProgramError,
        >,
        (top, (middle, (profile, device))): Report,
    ) -> Self {
        let outcomes = sorted(outcomes.expect("the run succeeds"));
        assert!(!outcomes.is_empty(), "the program reads");
        Self {
            outcomes,
            top,
            middle,
            profile,
            device,
        }
    }
}

/// c4c6's program on shots 0 to 3 through `$chain`, built of the layers
/// `$top` and `$middle` and the bottom set `$set`, the run ended by `$end`
/// (`wait` or awaited): what it gave.
macro_rules! c4c6_ran {
    (|$top:ident, $middle:ident, $set:ident| $chain:expr, $end:expr) => {{
        let qodec = c4c6();
        let $top = QodecLayer::new(&qodec, 0).expect("c4c6 plans").adapter();
        let $middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans").adapter();
        let $set = bottom(&qodec).set.clone();
        let chain = $chain.expect("every boundary matches");
        let shots: Vec<_> = CODE_SHOTS.collect();
        let run = chain
            .run(
                c4c6_program(&qodec).to_static(Needs::default()),
                shot_set(&shots),
            )
            .expect("the job fits the chain");
        let (outcomes, report): (_, Report) = $end(run);
        Ran::new(outcomes, report)
    }};
    (|$top:ident, $middle:ident, $set:ident| $chain:expr) => {
        c4c6_ran!(
            |$top, $middle, $set| $chain,
            |run: executor_protocol::Run<_, _, _>| run.wait()
        )
    };
}

/// The reference run: the chain built one component at a time, from the
/// static driver down, the coins answering within their call, the run
/// waited on the caller's thread.
fn reference() -> Ran {
    c4c6_ran!(|top, middle, set| static_driver()
        >> top
        >> middle
        >> profiler()
        >> coin_device(set, SEED))
}

/// `>>` is associative (QD6): the two layers composed first, as one
/// adapter, then put between the static driver and the profiler: the same
/// run.
#[test]
fn c4c6_gives_the_same_run_with_its_layers_composed_first() {
    let (grouped, expected) = bounded(|| {
        let grouped = c4c6_ran!(|top, middle, set| {
            let layers = top >> middle;
            static_driver() >> layers >> profiler() >> coin_device(set, SEED)
        });
        (grouped, reference())
    });
    assert_eq!(grouped, expected);
}

/// The two layers and the profiler composed first, as one adapter, then
/// put between the static driver and the coins: the same run.
#[test]
fn c4c6_gives_the_same_run_with_its_layers_and_profiler_composed_first() {
    let (grouped, expected) = bounded(|| {
        let grouped = c4c6_ran!(|top, middle, set| {
            let adapters = top >> middle >> profiler();
            static_driver() >> adapters >> coin_device(set, SEED)
        });
        (grouped, reference())
    });
    assert_eq!(grouped, expected);
}

/// The profiler composed with the coins first, as one executor, and the
/// middle layer over that executor, then the top layer: the same run.
#[test]
fn c4c6_gives_the_same_run_with_its_executor_composed_first() {
    let (grouped, expected) = bounded(|| {
        let grouped = c4c6_ran!(|top, middle, set| {
            let below = (profiler() >> coin_device(set, SEED)).expect("the profiler fits");
            let middle_down = (middle >> below).expect("the middle layer fits");
            static_driver() >> top >> middle_down
        });
        (grouped, reference())
    });
    assert_eq!(grouped, expected);
}

/// The run awaited, as an async caller does, rather than waited: the same
/// run.
#[test]
fn c4c6_gives_the_same_run_awaited_as_waited() {
    let (awaited, expected) = bounded(|| {
        let awaited = c4c6_ran!(
            |top, middle, set| static_driver()
                >> top
                >> middle
                >> profiler()
                >> coin_device(set, SEED),
            |run: executor_protocol::Run<_, _, _>| {
                let (mut ended, _) = join_all(vec![run.into_future()]);
                ended.pop().expect("the run ends")
            }
        );
        (awaited, reference())
    });
    assert_eq!(awaited, expected);
}

/// What a run gave: its outcomes, by shot and tag, and its top layer's
/// statuses, which differ between the simulator's draws at `DRAWS_FLIP`.
type OutcomesAndStatuses = (Vec<(ShotId, u32, Value)>, Statuses);

/// A run's future, its output mapped to [`OutcomesAndStatuses`].
type Running = Pin<Box<dyn Future<Output = OutcomesAndStatuses>>>;

/// A thread of its own, on which each job waits at `gate` first: a placed
/// executor answers no call until the test opens it.
fn own_thread_held_at(gate: Gate) -> impl ExecutorPlacement {
    let mut thread = OwnThread.start();
    move |job: PlacementJob| {
        let gate = gate.clone();
        thread(Box::new(move || {
            gate.pass();
            job();
        }));
    }
}

/// `run`, opening `gate` once it was polled: last in a [`join_all`], once
/// every run was polled.
fn opening_once_polled(gate: Gate, mut run: Running) -> Running {
    Box::pin(poll_fn(move |context| {
        let polled = run.as_mut().poll(context);
        gate.open();
        polled
    }))
}

/// The runs: each one's future, built with its own chain, repetition3's
/// two over simulators answering only once `gate` is open.
fn runs(gate: &Gate) -> Vec<Running> {
    let code_shots: Vec<_> = CODE_SHOTS.collect();
    macro_rules! running {
        ($qodec:expr, $program:ident, $shots:expr, |$($layer:ident),+| $chain:expr) => {{
            let qodec = $qodec;
            let mut layers = (0..).map(|k| QodecLayer::new(&qodec, k).expect("the code plans").adapter());
            $(let $layer = layers.next().expect("a layer");)+
            let chain = $chain(&qodec).expect("every boundary matches");
            let run = chain
                .run($program(&qodec).to_static(Needs::default()), shot_set($shots))
                .expect("the job fits the chain");
            Box::pin(async move {
                let (outcomes, (statuses, _)) = run.into_future().await;
                (sorted(outcomes.expect("the run succeeds")), statuses)
            }) as Running
        }};
    }
    vec![
        running!(c4c6(), c4c6_program, &code_shots, |top, middle| {
            |q: &qodec::Qodec| {
                static_driver()
                    >> top
                    >> middle
                    >> profiler()
                    >> coin_device(bottom(q).set.clone(), SEED)
            }
        }),
        running!(c422(), c422_program, &code_shots, |layer| {
            |q: &qodec::Qodec| {
                static_driver() >> layer >> profiler() >> coin_device(bottom(q).set.clone(), SEED)
            }
        }),
        running!(repetition3(), repetition3_program, &SHOTS, |layer| {
            |_: &qodec::Qodec| {
                static_driver()
                    >> layer
                    >> profiler()
                    >> simulator(stim_rz().set, SEED, DRAWS_FLIP)
                        .run_on(own_thread_held_at(gate.clone()))
            }
        }),
        running!(repetition3(), repetition3_program, &SHOTS, |layer| {
            |_: &qodec::Qodec| {
                static_driver()
                    >> layer
                    >> profiler()
                    >> simulator(stim_rz().set, SEED + 1, DRAWS_FLIP)
                        .run_on(own_thread_held_at(gate.clone()))
            }
        }),
    ]
}

/// Four runs, of three codes and two seeds, awaited together on one
/// thread, the two over a placed simulator interleaving: each gives the
/// outcomes and statuses it gives alone, awaited by itself. The simulators
/// answer once every run was polled, so that the runs interleave.
#[test]
fn runs_awaited_together_on_one_thread_each_give_what_they_give_alone() {
    let (together, alone) = bounded(|| {
        let held = Gate::default();
        let mut together = runs(&held);
        let last = together.pop().expect("there are runs");
        together.push(opening_once_polled(held, last));
        let (together, pending) = join_all(together);
        assert!(pending > 0, "a run yielded, so that the runs interleave");
        let open = Gate::default();
        open.open();
        let alone: Vec<_> = (runs(&open).into_iter())
            .map(|run| join_all(vec![run]).0.pop().expect("the run ends"))
            .collect();
        (together, alone)
    });
    assert!(
        alone.iter().all(|(o, _)| !o.is_empty()),
        "every program reads"
    );
    assert_ne!(alone[2].1, alone[3].1, "the two seeds draw differently");
    assert_eq!(together, alone);
}
