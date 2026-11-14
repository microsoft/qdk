// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The failures that end a run: a part's error at the shot it names,
//! readouts misshapen for their batch, and a part's panic, from its call,
//! its future's poll or drop, or its end of the shots. Each is named after
//! the right part wherever the brackets go, and the run's shots are ended.

use std::any::type_name;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Context, Waker};

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, EndsBelow, FailsOnce, Impatient, Later, Log, Misshape, Misshapen,
    Panics, PanicsEnding, PanicsInDrop, PanicsWhen, Profiler, Program, Renamed, Trap, Values,
};
use crate::{
    Adapter, ChainPart, Drive, Driver, Execute, Executor, Failure, Runner, ShapeFault, ShotId,
    ShotSet,
};

use super::{SEED, failed, runner};

/// A program trapping on shot 5, run on 8 shots: the run fails naming shot
/// 5 and the trap, and gives no records.
#[test]
fn a_failing_shot_ends_the_run_naming_it() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new()) >> Adapter::new(Trap::on(5)) >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids(0..8), SEED));
    assert_eq!(error.shot, ShotId::new(5));
    assert!(
        matches!(&error.failure, Failure::Part(why) if why.to_string() == "the program trapped"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "shot 5 failed: the program trapped");
}

/// Coin copies under a noise flipping 30% of the `x`s, above an adapter
/// failing the run's second `execute`, in both bracketings: the failed run
/// leaves no shot's state in any part, so the rerun with the same seed gives
/// the records of a new chain.
#[test]
fn a_rerun_after_a_failed_run_gives_a_new_chains_records() {
    /// The records of `runner`'s second run of `shots`, its first failing.
    fn rerun<D: Drive<Output = Values>, E: Execute>(
        mut runner: Runner<D, E>,
        shots: &ShotSet,
    ) -> Vec<Values> {
        failed(runner.run(shots, SEED));
        runner.run(shots, SEED).expect("the rerun succeeds")
    }
    let shots = ShotSet::ids(0..16);
    let log = Log::default();
    let new_chain = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &log))
            >> Executor::new(Bits::new(&log)),
    )
    .run(&shots, SEED)
    .expect("the run succeeds");
    let left = runner(
        ((Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, &log)))
            >> Adapter::new(FailsOnce::new()))
            >> Executor::new(Bits::new(&log)),
    );
    let right = runner(
        Driver::new(CoinCopy::new())
            >> (Adapter::new(BitFlip::new(300, &log))
                >> (Adapter::new(FailsOnce::new()) >> Executor::new(Bits::new(&log)))),
    );
    assert_eq!(rerun(left, &shots), new_chain, "the noise with the driver");
    assert_eq!(rerun(right, &shots), new_chain, "the noise with the bits");
}

/// An executor answering a readouts more than the batches: a failure of
/// the first batch's first shot, with the shape fault.
#[test]
fn an_answer_of_more_readouts_than_batches_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::ExtraReadouts),
        ShapeFault::Batches {
            expected: 1,
            got: 2
        }
    );
}

/// An executor answering a row fewer than the batch's shots: a failure.
#[test]
fn an_answer_missing_a_shots_row_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::MissingRow),
        ShapeFault::Rows {
            batch: 0,
            expected: 3,
            got: 2
        }
    );
}

/// An executor answering a value more than the batch's readout slots: a
/// failure.
#[test]
fn an_answer_wider_than_the_batchs_readouts_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::ExtraValue),
        ShapeFault::Slots {
            batch: 0,
            expected: 1,
            got: 2
        }
    );
}

/// Two batches in one `execute`, shots 4 and 2 then 9 and 7, each measuring
/// qubit 0, on an executor misshaping the second batch's readouts: the run
/// fails naming that batch and its first shot, 9.
#[test]
fn a_later_batchs_misshapen_readouts_name_it() {
    for (misshape, expected) in [
        (
            Misshape::MissingRow,
            ShapeFault::Rows {
                batch: 1,
                expected: 2,
                got: 1,
            },
        ),
        (
            Misshape::ExtraValue,
            ShapeFault::Slots {
                batch: 1,
                expected: 1,
                got: 2,
            },
        ),
    ] {
        let mut runner = runner(
            Driver::new(Program::halves(&[("mz", 0)], &[("mz", 0)]))
                >> Executor::new(Misshapen(misshape)),
        );
        let error = failed(runner.run(&ShotSet::ids([4, 2, 9, 7]), SEED));
        assert_eq!(error.shot, ShotId::new(9), "{expected}");
        let Failure::Part(fault) = error.failure else {
            panic!("the part's error, not {:?}", error.failure);
        };
        assert_eq!(fault.downcast_ref::<ShapeFault>(), Some(&expected));
    }
}

/// The shape fault of a run of 3 shots, first shot 6, of one `mz` on a
/// misshapen executor, checking the failure names shot 6.
fn shape_fault(misshape: Misshape) -> ShapeFault {
    let mut runner =
        runner(Driver::new(Program::new(&[("mz", 0)])) >> Executor::new(Misshapen(misshape)));
    let error = failed(runner.run(&ShotSet::ids([6, 1, 2]), SEED));
    assert_eq!(error.shot, ShotId::new(6));
    let Failure::Part(fault) = error.failure else {
        panic!("the part's error, not {:?}", error.failure);
    };
    *fault.downcast_ref::<ShapeFault>().expect("a shape fault")
}

/// Coin copies under a noise, above a trap on shot 5, over bits, in each
/// bracketing: a run's shots are ended on the noise, then the trap, then
/// the bits, both when the run succeeds, by the driver and then by the
/// runner, and when it fails, by the runner.
#[test]
fn a_runs_shots_are_ended_top_to_bottom_even_when_it_fails() {
    /// The ends logged by a run that succeeds, then by one that fails.
    type Ends = [Vec<String>; 2];
    /// The shots ended by `runner`'s run of shots 0 to 3, which succeeds,
    /// then by its run of shots 0 to 7, which fails on shot 5, as logged
    /// to `log`.
    fn ends<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) -> Ends {
        let ended = |log: &Log| -> Vec<String> {
            log.entries()
                .into_iter()
                .filter(|entry| entry.contains("end_shots"))
                .collect()
        };
        runner
            .run(&ShotSet::ids(0..4), SEED)
            .expect("a run without shot 5 succeeds");
        let succeeded = ended(log);
        failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        let failed_run = ended(log)[succeeded.len()..].to_vec();
        [succeeded, failed_run]
    }
    let bracketings: [fn(&Log) -> Ends; 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(BitFlip::new(300, log))
                >> Adapter::new(Trap::on(5).logging(log))
                >> Executor::new(Bits::new(log));
            ends(runner(chain), log)
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(BitFlip::new(300, log))
                    >> Adapter::new(Trap::on(5).logging(log))))
                >> Executor::new(Bits::new(log));
            ends(runner(chain), log)
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, log)))
                >> (Adapter::new(Trap::on(5).logging(log)) >> Executor::new(Bits::new(log)));
            ends(runner(chain), log)
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(BitFlip::new(300, log))
                    >> Adapter::new(Trap::on(5).logging(log)))
                    >> Executor::new(Bits::new(log)));
            ends(runner(chain), log)
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(BitFlip::new(300, log))
                    >> (Adapter::new(Trap::on(5).logging(log)) >> Executor::new(Bits::new(log))));
            ends(runner(chain), log)
        },
    ];
    let top_to_bottom = [
        "BitFlip end_shots, holding 0",
        "Trap end_shots",
        "Bits end_shots, holding 0",
    ];
    for (bracketing, chain) in bracketings.into_iter().enumerate() {
        let [succeeded, failed_run] = chain(&Log::default());
        assert_eq!(
            succeeded,
            [top_to_bottom, top_to_bottom].concat(),
            "bracketing {bracketing}, ended by the driver, then the runner"
        );
        assert_eq!(failed_run, top_to_bottom, "bracketing {bracketing}, failed");
    }
}

/// Coin copies under an adapter that panics in `end_shots`, over a noise,
/// over bits, in a run with no other fault, in each bracketing: the
/// driver's end of its shots fails the run, the runner ends them again on
/// every part, each part beside the one that panics is ended both times,
/// and `run` then raises the panic.
#[test]
fn a_panic_ending_shots_ends_every_other_part_then_leaves_the_run() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn ends_every_other_part<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises the panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 2);
        assert_eq!(log.count("Bits end_shots, holding 0"), 2);
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::new(log))
                >> Adapter::new(BitFlip::new(300, log))
                >> Executor::new(Bits::new(log));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::new(log)) >> Adapter::new(BitFlip::new(300, log))))
                >> Executor::new(Bits::new(log));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new()) >> Adapter::new(PanicsEnding::new(log)))
                >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log)));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(PanicsEnding::new(log)) >> Adapter::new(BitFlip::new(300, log)))
                    >> Executor::new(Bits::new(log)));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::new(log))
                    >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log))));
            ends_every_other_part(runner(chain), log);
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// Coin copies over an adapter whose first `end_shots` panics, over a
/// noise, over bits, in each bracketing: the driver ends each shot on the
/// chain below, so the panic unwinds through the driver's `drive`, and the
/// run fails naming the driver, part 0, not the adapter.
#[test]
fn a_panic_ending_shots_the_driver_ends_is_named_as_the_driver() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn names_the_driver<D: Drive, E: Execute>(mut runner: Runner<D, E>) {
        let error = failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == ChainPart::new(0, type_name::<CoinCopy>())
                        && message == "end_shots broke"
            ),
            "{error:?}"
        );
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> Adapter::new(PanicsEnding::once(log))
                    >> Adapter::new(BitFlip::new(300, log))
                    >> Executor::new(Bits::new(log)),
            ));
        },
        |log| {
            names_the_driver(runner(
                (Driver::new(CoinCopy::new())
                    >> (Adapter::new(PanicsEnding::once(log))
                        >> Adapter::new(BitFlip::new(300, log))))
                    >> Executor::new(Bits::new(log)),
            ));
        },
        |log| {
            names_the_driver(runner(
                (Driver::new(CoinCopy::new()) >> Adapter::new(PanicsEnding::once(log)))
                    >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log))),
            ));
        },
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> ((Adapter::new(PanicsEnding::once(log))
                        >> Adapter::new(BitFlip::new(300, log)))
                        >> Executor::new(Bits::new(log))),
            ));
        },
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> (Adapter::new(PanicsEnding::once(log))
                        >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log)))),
            ));
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// A program that never ends its shots, over an adapter that ends its
/// shots below inside `adapt`, over an adapter whose first `end_shots`
/// panics, over bits, in each bracketing: the panic unwinds through the
/// ending adapter's `adapt`, so the run fails naming that adapter, part 1,
/// not the driver, and not the part that panicked.
#[test]
fn a_panic_ending_shots_an_adapter_ends_is_named_as_that_adapter() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn names_the_adapter<D: Drive, E: Execute>(mut runner: Runner<D, E>) {
        let error = failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == ChainPart::new(1, type_name::<EndsBelow>())
                        && message == "end_shots broke"
            ),
            "{error:?}"
        );
    }
    let program = || Driver::new(Program::new(&[("mz", 0)]).not_ending());
    let ends = |log: &Log| Adapter::new(EndsBelow::new(log));
    let panics = |log: &Log| Adapter::new(PanicsEnding::once(log));
    let bits = |log: &Log| Executor::new(Bits::new(log));
    let logs: [Log; 5] = Default::default();
    names_the_adapter(runner(
        ((program() >> ends(&logs[0])) >> panics(&logs[0])) >> bits(&logs[0]),
    ));
    names_the_adapter(runner(
        (program() >> ends(&logs[1])) >> (panics(&logs[1]) >> bits(&logs[1])),
    ));
    names_the_adapter(runner(
        program() >> (ends(&logs[2]) >> (panics(&logs[2]) >> bits(&logs[2]))),
    ));
    names_the_adapter(runner(
        program() >> ((ends(&logs[3]) >> panics(&logs[3])) >> bits(&logs[3])),
    ));
    names_the_adapter(runner(
        (program() >> (ends(&logs[4]) >> panics(&logs[4]))) >> bits(&logs[4]),
    ));
}

/// A program that never ends its shots, over an adapter whose first
/// `end_shots` panics, over a noise, over bits, in each bracketing: the
/// runner's end is the only one, so the panic is raised again from `run`,
/// once the noise and the bits are ended, and the outputs are lost, where
/// the same panic in the driver's end fails the run naming the driver.
#[test]
fn a_panic_ending_shots_the_runner_ends_is_raised_from_the_run() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn raised<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises the panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
        assert_eq!(log.count("Bits end_shots, holding 0"), 1);
    }
    let program = || Driver::new(Program::new(&[("mz", 0)]).not_ending());
    let breaks = |log: &Log| Adapter::new(PanicsEnding::once(log));
    let noise = |log: &Log| Adapter::new(BitFlip::new(300, log));
    let bits = |log: &Log| Executor::new(Bits::new(log));
    let logs: [Log; 5] = Default::default();
    raised(
        runner(((program() >> breaks(&logs[0])) >> noise(&logs[0])) >> bits(&logs[0])),
        &logs[0],
    );
    raised(
        runner((program() >> breaks(&logs[1])) >> (noise(&logs[1]) >> bits(&logs[1]))),
        &logs[1],
    );
    raised(
        runner(program() >> (breaks(&logs[2]) >> (noise(&logs[2]) >> bits(&logs[2])))),
        &logs[2],
    );
    raised(
        runner(program() >> ((breaks(&logs[3]) >> noise(&logs[3])) >> bits(&logs[3]))),
        &logs[3],
    );
    raised(
        runner((program() >> (breaks(&logs[4]) >> noise(&logs[4]))) >> bits(&logs[4])),
        &logs[4],
    );
}

/// Coin copies under a noise, over an adapter that panics in `end_shots`,
/// over bits that answer later: a run whose future is dropped by the
/// caller's own panic still ends its shots on every other part, and drops
/// the end's panic rather than raising a second one, which would abort.
#[test]
fn a_run_dropped_by_a_panic_ends_its_shots_and_keeps_that_panic() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &log))
            >> Adapter::new(PanicsEnding::new(&log))
            >> Executor::new(Later(Bits::new(&log))),
    );
    let shots = ShotSet::ids(0..8);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the run is unfinished");
        panic!("the caller broke");
    }));
    let panic = caught.expect_err("the caller's panic leaves the closure");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the caller broke"));
    assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}

/// Coin copies over an adapter that panics in `end_shots`, over bits that
/// answer later: a run whose future is dropped unfinished, outside any
/// panic, ends its shots on the bits, then raises the end's panic from
/// the drop.
#[test]
fn a_run_dropped_unfinished_raises_an_ends_panic_from_the_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(PanicsEnding::new(&log))
            >> Executor::new(Later(Bits::new(&log))),
    );
    let shots = ShotSet::ids(0..8);
    let mut run = Box::pin(runner.run_async(&shots, SEED));
    let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    assert!(poll.is_pending(), "the run is unfinished");
    let dropped = catch_unwind(AssertUnwindSafe(|| drop(run)));
    let panic = dropped.expect_err("the drop raises the end's panic");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}

/// Coin copies over two adapters that panic in `end_shots`, "first" above
/// "second", over bits, in each bracketing: `run` raises the first panic,
/// top to bottom, and the bits are still ended.
#[test]
fn of_two_panics_ending_shots_the_first_is_raised() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn raises_the_first<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises a panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"first"));
        assert_eq!(log.count("Bits end_shots, holding 0"), 2);
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::saying("first", log))
                >> Adapter::new(PanicsEnding::saying("second", log))
                >> Executor::new(Bits::new(log));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::saying("first", log))
                    >> Adapter::new(PanicsEnding::saying("second", log))))
                >> Executor::new(Bits::new(log));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::saying("first", log)))
                >> (Adapter::new(PanicsEnding::saying("second", log))
                    >> Executor::new(Bits::new(log)));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(PanicsEnding::saying("first", log))
                    >> Adapter::new(PanicsEnding::saying("second", log)))
                    >> Executor::new(Bits::new(log)));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::saying("first", log))
                    >> (Adapter::new(PanicsEnding::saying("second", log))
                        >> Executor::new(Bits::new(log))));
            raises_the_first(runner(chain), log);
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// An executor that panics, while making its future or while it is
/// polled: the run fails naming the batch's first shot, the executor at
/// its position and the message.
#[test]
fn an_executors_panic_is_a_failure_of_its_batch() {
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            "the part broke while making its future",
        ),
        (PanicsWhen::Polled, "the part broke while it was polled"),
    ] {
        let mut runner =
            runner(Driver::new(Program::new(&[("mz", 0)])) >> Executor::new(Panics(when)));
        let error = failed(runner.run(&ShotSet::ids([3, 1]), SEED));
        assert_eq!(error.shot, ShotId::new(3));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(m) }
                    if *part == ChainPart::new(1, type_name::<Panics>()) && m == message
            ),
            "{error:?}"
        );
    }
}

/// A driver that panics: the run fails naming its first shot and the
/// driver, part 0.
#[test]
fn a_drivers_panic_is_a_failure_of_the_run() {
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            "the part broke while making its future",
        ),
        (PanicsWhen::Polled, "the part broke while it was polled"),
    ] {
        let log = Log::default();
        let mut runner = runner(Driver::new(Panics(when)) >> Executor::new(Bits::new(&log)));
        let error = failed(runner.run(&ShotSet::ids([9, 1]), SEED));
        assert_eq!(error.shot, ShotId::new(9));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(m) }
                    if *part == ChainPart::new(0, type_name::<Panics>()) && m == message
            ),
            "{error:?}"
        );
    }
}

/// A driver panicking while polled, composed with a profiler: the run
/// fails naming the driver, not the profiler.
#[test]
fn a_composed_drivers_panic_names_the_driver() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let mut runner = runner(
        (Driver::new(Panics(PanicsWhen::Polled)) >> Adapter::new(Profiler::new(&counted, &log)))
            >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids([9, 1]), SEED));
    assert!(
        matches!(
            &error.failure,
            Failure::Panicked { part, .. }
                if *part == ChainPart::new(0, type_name::<Panics>())
        ),
        "{error:?}"
    );
}

/// A driver and an executor that panic, each named as a part built at
/// run time is, by what it was built from: the failure names the part by
/// that name, at its position.
#[test]
fn a_panic_names_the_part_by_its_name() {
    let log = Log::default();
    let shots = ShotSet::ids([9, 1]);
    let mut executor = runner(
        Driver::new(Program::new(&[("mz", 0)]))
            >> Executor::new(Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")),
    );
    let mut driver = runner(
        Driver::new(Renamed::new(Panics(PanicsWhen::Polled), "h2 program"))
            >> Executor::new(Bits::new(&log)),
    );
    for (runner_name, error, position, name) in [
        (
            "executor",
            failed(executor.run(&shots, SEED)),
            1,
            "qodec runtime",
        ),
        ("driver", failed(driver.run(&shots, SEED)), 0, "h2 program"),
    ] {
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, .. } if *part == ChainPart::new(position, name)
            ),
            "{runner_name}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(&format!("`{name}` (part {position}) panicked")),
            "{runner_name}: {error}"
        );
    }
}

/// A driver that panics, over a noise, over bits, bracketed both ways:
/// the failed run ends its shots on the noise and the bits.
#[test]
fn a_run_failed_by_a_drivers_panic_ends_its_shots() {
    /// Runs `runner` on 2 shots, which fails, and checks it as above.
    fn ends_its_shots<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        failed(
            runner
                .run(&ShotSet::ids([9, 1]), SEED)
                .map(|records| records.len()),
        );
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
        assert_eq!(log.count("Bits end_shots, holding 0"), 1);
    }
    for when in [PanicsWhen::MakingItsFuture, PanicsWhen::Polled] {
        let log = Log::default();
        ends_its_shots(
            runner(
                (Driver::new(Panics(when)) >> Adapter::new(BitFlip::new(0, &log)))
                    >> Executor::new(Bits::new(&log)),
            ),
            &log,
        );
        let log = Log::default();
        ends_its_shots(
            runner(
                Driver::new(Panics(when))
                    >> (Adapter::new(BitFlip::new(0, &log)) >> Executor::new(Bits::new(&log))),
            ),
            &log,
        );
    }
}

/// A program, over a profiler, over a noise, over bits, each a part whose
/// futures may panic once dropped.
type DropChain = (
    Driver<PanicsInDrop<Program>>,
    Adapter<PanicsInDrop<Profiler>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Executor<PanicsInDrop<Bits>>,
);

/// The parts of a [`DropChain`], whose part at position `armed` panics in
/// its futures' destructor.
fn drop_chain(armed: usize, log: &Log) -> DropChain {
    let counted = Rc::new(Cell::new(0));
    (
        Driver::new(PanicsInDrop::new(Program::new(&[("mz", 0)]), armed == 0)),
        Adapter::new(PanicsInDrop::new(Profiler::new(&counted, log), armed == 1)),
        Adapter::new(PanicsInDrop::new(BitFlip::new(0, log), armed == 2)),
        Executor::new(PanicsInDrop::new(Bits::new(log), armed == 3)),
    )
}

/// A program, over a profiler, over a noise, over bits, in each
/// bracketing, one of them panicking in the destructor of each future it
/// returns, once its answer is given: the run fails naming that part, at
/// its position, with its message, as for a panic while it is polled, and
/// its shots are ended on the bits.
#[test]
fn a_panic_dropping_a_parts_future_is_a_failure_naming_that_part() {
    /// Runs `runner` on 2 shots, and checks it as above.
    fn names<D: Drive, E: Execute>(mut runner: Runner<D, E>, named: &ChainPart, log: &Log) {
        let error = failed(
            runner
                .run(&ShotSet::ids([9, 1]), SEED)
                .map(|records| records.len()),
        );
        assert_eq!(error.shot, ShotId::new(9));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if part == named && message == "the part's future broke when dropped"
            ),
            "{named}: {error:?}"
        );
        assert!(log.count("Bits end_shots, holding 0") >= 1, "{named}");
    }
    let bracketings: [fn(usize, &ChainPart); 5] = [
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(((d >> a) >> b) >> e), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner((d >> a) >> (b >> e)), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(d >> (a >> (b >> e))), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(d >> ((a >> b) >> e)), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner((d >> (a >> b)) >> e), named, &log);
        },
    ];
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        for chain in bracketings {
            chain(armed, &ChainPart::new(armed, name));
        }
    }
}

/// A program over an adapter that stops waiting for its first answer from
/// below, over bits whose futures panic when dropped, bracketed both ways:
/// the bits' future is dropped unfinished by the adapter, not by the
/// protocol, so the panic unwinds through the adapter's call and the run
/// fails naming the adapter, with the bits' message.
#[test]
fn a_panic_dropping_a_future_a_part_above_stopped_waiting_for_names_that_part() {
    for brackets in ["left", "right"] {
        let log = Log::default();
        let d = Driver::new(Program::new(&[("mz", 0)]));
        let a = Adapter::new(Impatient::new(&log));
        let e = Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true));
        let shots = ShotSet::ids([3, 1]);
        let error = if brackets == "left" {
            failed(runner((d >> a) >> e).run(&shots, SEED))
        } else {
            failed(runner(d >> (a >> e)).run(&shots, SEED))
        };
        assert_eq!(error.shot, ShotId::new(3), "{brackets}");
        let adapter = ChainPart::new(1, type_name::<Impatient>());
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == adapter && message == "the part's future broke when dropped"
            ),
            "{brackets}: {error:?}"
        );
    }
}

/// A program, over a profiler, over two noises, over bits, each a part
/// whose futures may panic when polled, and again when dropped.
type CompoundChain = (
    Driver<PanicsInDrop<Program>>,
    Adapter<PanicsInDrop<Profiler>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Executor<PanicsInDrop<Bits>>,
);

/// The parts of a [`CompoundChain`], whose part at position `armed`
/// panics when its futures are polled, and again when they are dropped.
fn compound_chain(armed: usize, log: &Log) -> CompoundChain {
    let counted = Rc::new(Cell::new(0));
    (
        Driver::new(PanicsInDrop::polled_too(
            Program::new(&[("mz", 0)]),
            armed == 0,
        )),
        Adapter::new(PanicsInDrop::polled_too(
            Profiler::new(&counted, log),
            armed == 1,
        )),
        Adapter::new(PanicsInDrop::polled_too(BitFlip::new(0, log), armed == 2)),
        Adapter::new(PanicsInDrop::polled_too(BitFlip::new(0, log), armed == 3)),
        Executor::new(PanicsInDrop::polled_too(Bits::new(log), armed == 4)),
    )
}

/// A program, over a profiler, over two noises, over bits, in each of
/// the 14 bracketings, one of them panicking while its future is polled,
/// and again when that future is dropped: the run fails naming that
/// part, at its position, with the message of its first panic, and the
/// second is dropped; the process does not abort, as it would if a
/// composed part's future dropped the part's while the first unwinds.
#[test]
fn a_part_panicking_polled_then_dropped_is_named_with_its_first_panic() {
    /// Runs `runner` on 2 shots, and checks it as above.
    fn names<D: Drive, E: Execute>(mut runner: Runner<D, E>, named: &ChainPart) {
        let error = failed(
            runner
                .run(&ShotSet::ids([9, 1]), SEED)
                .map(|records| records.len()),
        );
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if part == named && message == "the part's future broke when polled"
            ),
            "{named}: {error:?}"
        );
    }
    /// Runs the chain armed at `$armed` in each bracketing `$chain` of its
    /// parts, named as in `[$d, $a, $b, $c, $e]`.
    macro_rules! in_each {
        (
            $armed:expr, $named:expr, [$d:ident, $a:ident, $b:ident, $c:ident, $e:ident],
            $($chain:expr),+ $(,)?
        ) => {$({
            let log = Log::default();
            let ($d, $a, $b, $c, $e) = compound_chain($armed, &log);
            names(runner($chain), $named);
        })+};
    }
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        let named = &ChainPart::new(armed, name);
        in_each!(
            armed,
            named,
            [d, a, b, c, e],
            d >> (((a >> b) >> c) >> e),
            d >> ((a >> (b >> c)) >> e),
            d >> ((a >> b) >> (c >> e)),
            d >> (a >> ((b >> c) >> e)),
            d >> (a >> (b >> (c >> e))),
            (d >> a) >> ((b >> c) >> e),
            (d >> a) >> (b >> (c >> e)),
            ((d >> a) >> b) >> (c >> e),
            (d >> (a >> b)) >> (c >> e),
            (((d >> a) >> b) >> c) >> e,
            ((d >> (a >> b)) >> c) >> e,
            ((d >> a) >> (b >> c)) >> e,
            (d >> ((a >> b) >> c)) >> e,
            (d >> (a >> (b >> c))) >> e,
        );
    }
}

/// A program, over two noises, over bits, in each bracketing, one of them
/// panicking while the future of its `ready` is polled, and again when
/// it is dropped: the run raises the first panic, as it raises any panic
/// in `ready`, and the second is dropped, so the process does not abort.
#[test]
fn a_panic_in_ready_polled_then_dropped_is_raised_with_its_first_message() {
    /// Runs `runner` on 2 shots, and checks it as above.
    fn raises<D: Drive, E: Execute>(mut runner: Runner<D, E>, armed: usize) {
        let raised = catch_unwind(AssertUnwindSafe(|| {
            runner.run(&ShotSet::ids([3, 1]), SEED).map(|_| ())
        }));
        let payload = raised.expect_err("a panic in `ready` is raised from the run");
        assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("the part's future broke when polled"),
            "part {armed}"
        );
    }
    /// Runs the chain armed at `$armed` in each bracketing `$chain` of its
    /// parts, named as in `[$d, $a, $b, $e]`.
    macro_rules! in_each {
        (
            $armed:expr, [$d:ident, $a:ident, $b:ident, $e:ident],
            $($chain:expr),+ $(,)?
        ) => {$({
            let log = Log::default();
            let $d = Driver::new(PanicsInDrop::in_ready(
                Program::new(&[("mz", 0)]),
                $armed == 0,
            ));
            let $a = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0, &log), $armed == 1));
            let $b = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0, &log), $armed == 2));
            let $e = Executor::new(PanicsInDrop::in_ready(Bits::new(&log), $armed == 3));
            raises(runner($chain), $armed);
        })+};
    }
    for armed in 0..4 {
        in_each!(
            armed,
            [d, a, b, e],
            ((d >> a) >> b) >> e,
            (d >> a) >> (b >> e),
            d >> (a >> (b >> e)),
            d >> ((a >> b) >> e),
            (d >> (a >> b)) >> e,
        );
    }
}

/// Coin copies over bits that answer later, and whose futures panic once
/// dropped: a run whose future is dropped while the bits' future is
/// pending raises that panic again from the drop, and its shots are ended
/// on the bits.
#[test]
fn a_panic_dropping_a_parts_future_with_the_run_is_raised_from_the_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true)),
    );
    let shots = ShotSet::ids(0..4);
    let dropped = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the bits answer later");
    }));
    let panic = dropped.expect_err("the drop raises the panic");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"the part's future broke when dropped")
    );
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}

/// The same chain, whose run's future a caller's panic drops while the
/// bits' future is pending: the bits' panic in its destructor, a second
/// panic while the caller's unwinds, is dropped rather than aborting the
/// process, so the caller's panic leaves, and the shots are ended on the
/// bits.
#[test]
fn a_run_dropped_by_a_panic_keeps_that_panic_when_a_parts_future_panics_in_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true)),
    );
    let shots = ShotSet::ids(0..4);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the bits answer later");
        panic!("the caller broke");
    }));
    let panic = caught.expect_err("the caller's panic leaves");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the caller broke"));
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}
