// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run returns when it gives no outputs: the failures that end it,
//! a part's error or refusal at the shot it names, and what the messages
//! of a run's errors say. A part's panics are in `part_panics.rs`; that a
//! failed run's shots are ended top to bottom is in `runner.rs`, beside
//! the successful run's; the runner's own refusals are in
//! `run_limits.rs`.

use std::cell::Cell;
use std::rc::Rc;

use crate::tests::fake_parts::{
    Bits, CoinCopy, Log, Panics, PanicsWhen, Profiler, Program, Renamed, Trap, coin_gates,
    in_each_bracketing,
};
use crate::{
    Adapter, Batch, BatchReadouts, DriveError, Driver, Execute, ExecuteError, ExecuteRefusal,
    Executor, ExecutorSetup, Failure, InstructionSet, Offer, Seed, ShotId, ShotSet,
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
}

/// An executor of [`CoinGates`](crate::tests::fake_parts::CoinGates)
/// refusing every `execute` with `refusal`, counting them in `calls`.
struct Refuses {
    refusal: ExecuteRefusal,
    calls: Rc<Cell<u32>>,
}

impl Refuses {
    fn new(refusal: ExecuteRefusal, calls: &Rc<Cell<u32>>) -> Self {
        Self {
            refusal,
            calls: Rc::clone(calls),
        }
    }
}

impl ExecutorSetup for Refuses {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }
}

impl Execute for Refuses {
    async fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.calls.set(self.calls.get() + 1);
        Err(ExecuteError::Refused(self.refusal.clone()))
    }
}

/// An executor refusing a run's `execute`, on a batch of shots 0 to 3,
/// because shot 2 failed earlier or is not held there, or, on a batch of
/// shots 2 to 5, because it cannot take the `execute` now: the run fails
/// naming shot 2, the shot the refusal names, whether or not it is the
/// batch's first, with the refusal. So too through an adapter that passes
/// the refusal up, wherever the brackets go: each boundary passes a
/// refusal from below up as it was made, so that the part above reads its
/// shot and its [`retry_class`](ExecuteRefusal::retry_class) unchanged.
/// The protocol sends a refused `execute` once, whatever its class: the
/// executor is asked once per run.
#[test]
fn a_refusal_ends_the_run_naming_its_shot() {
    let shot = ShotId::new(2);
    for (refusal, shots) in [
        (ExecuteRefusal::EarlierFailure(shot), ShotSet::ids(0..4)),
        (ExecuteRefusal::ShotNotHeld(shot), ShotSet::ids(0..4)),
        (
            ExecuteRefusal::NotReady {
                shot,
                why: "the device is calibrating".into(),
            },
            ShotSet::ids(2..6),
        ),
    ] {
        let calls = Rc::new(Cell::new(0));
        let refuses = || Executor::new(Refuses::new(refusal.clone(), &calls));
        let ends_the_run = |error: DriveError, brackets: &str| {
            assert_eq!(error.shot, shot, "{refusal:?}, {brackets}");
            assert!(
                matches!(&error.failure, Failure::Refused(why) if *why == refusal),
                "{brackets}: {error:?}"
            );
            assert_eq!(
                calls.replace(0),
                1,
                "the refused execute is sent once, {refusal:?}, {brackets}"
            );
        };
        ends_the_run(
            failed(runner(measure() >> refuses()).run(&shots, SEED)),
            "no adapter",
        );
        let log = Log::default();
        let counted = Rc::new(Cell::new(0));
        in_each_bracketing!(
            [d, a, e] from {
                let d = measure();
                let a = Adapter::new(Profiler::new(&counted, &log));
                let e = refuses();
            },
            |chain, brackets| ends_the_run(failed(runner(chain).run(&shots, SEED)), brackets),
        );
    }
}

/// The program measuring qubit 0 in one batch.
fn measure() -> Driver<Program> {
    Driver::new(Program::new(&[("mz", 0)]))
}

/// The errors of runs refused or failed, as a user reads them: each
/// message says the run was refused or failed, and names what is at fault
/// with what is wrong: the shots asked and the most a run may have, the
/// fixed seed, the part's reason, the shot and the part's error, that an
/// `execute` could not be taken now and why, the part that panicked and
/// its message.
#[test]
fn a_runs_error_says_what_ended_it_and_names_what_is_at_fault() {
    let log = Log::default();
    let shots = ShotSet::ids(0..8);
    let bits = |offer: Offer| Executor::new(Bits::new(&log).offering(offer));
    let coin = || Driver::new(CoinCopy::new());
    let limited = Offer::unlimited().max_qubits(64).max_shots(4);
    let unseedable = Offer::unlimited().max_qubits(64).not_seedable();
    for (row, error, said) in [
        (
            "too many shots",
            runner(coin() >> bits(limited)).run(&shots, SEED).map(drop),
            &["run refused", "8 shots", "at most 4"][..],
        ),
        (
            "a seed the chain cannot take",
            runner(coin() >> bits(unseedable))
                .run(&shots, SEED)
                .map(drop),
            &["run refused", "seed", "not seedable"],
        ),
        (
            "a part not ready",
            runner(coin() >> Executor::new(Bits::new(&log).busy("calibrating")))
                .run(&shots, SEED)
                .map(drop),
            &["run refused", "(part 1) is not ready: calibrating"],
        ),
        (
            "a shot failing",
            runner(coin() >> Adapter::new(Trap::on(5)) >> Executor::new(Bits::new(&log)))
                .run(&shots, SEED)
                .map(drop),
            &["run failed", "shot 5", "the program trapped"],
        ),
        (
            "an execute the executor cannot take now",
            runner(
                measure()
                    >> Executor::new(Refuses::new(
                        ExecuteRefusal::NotReady {
                            shot: ShotId::new(0),
                            why: "the device is calibrating".into(),
                        },
                        &Rc::default(),
                    )),
            )
            .run(&shots, SEED)
            .map(drop),
            &["run failed", "not ready", "the device is calibrating"],
        ),
        (
            "a part panicking",
            runner(
                measure()
                    >> Executor::new(Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")),
            )
            .run(&shots, SEED)
            .map(drop),
            &[
                "run failed",
                "`qodec runtime` (part 1) panicked",
                "the part broke while it was polled",
            ],
        ),
    ] {
        let message = error.expect_err("the run gives no outputs").to_string();
        for part in said {
            assert!(message.contains(part), "{row}: {message:?} says {part:?}");
        }
    }
}
