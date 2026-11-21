// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run returns when it gives no outputs: the failures that end it,
//! a part's error or refusal at the shot it names, and what the messages
//! of a run's errors say. A part's panics are in `part_panics.rs`; that a
//! failed run's shots are ended top to bottom is in `runner.rs`, beside
//! the successful run's and the runner's own refusals.

use crate::prelude::*;
use crate::tests::fake_parts::{
    Bits, CoinCopy, Log, Panics, PanicsWhen, Program, Renamed, Stating, Trap, coin_gates, qubits,
};
use crate::{
    Batch, BatchReadouts, ExecuteError, ExecuteRefusal, Failure, Features, InstructionSet, Seed,
    ShotId, ShotSet,
};

use super::{SEED, failed, runner};

/// A program trapping on shot 5, run on 8 shots: the run fails naming shot
/// 5 and the trap, and gives no records.
#[test]
fn a_failing_shot_ends_the_run_naming_it() {
    let log = Log::default();
    let mut runner = runner(CoinCopy::new().through(Trap::on(5)).on(Bits::new(&log)));
    let error = failed(runner.run(&ShotSet::ids(0..8), SEED));
    assert_eq!(error.shot, ShotId::new(5));
    assert!(
        matches!(&error.failure, Failure::Part(why) if why.to_string() == "the program trapped"),
        "{error:?}"
    );
}

/// An executor of [`CoinGates`](crate::tests::fake_parts::CoinGates)
/// refusing every `execute` with `refusal`.
struct Refuses {
    refusal: ExecuteRefusal,
}

impl Executor for Refuses {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        Err(ExecuteError::Refused(self.refusal.clone()))
    }
}

/// An executor refusing a run's `execute` because one of its shots, shot
/// 2, failed earlier or is not held there: the run fails naming that
/// shot, not the batch's first, with the refusal.
#[test]
fn a_refused_shot_ends_the_run_naming_it() {
    let shot = ShotId::new(2);
    for refusal in [
        ExecuteRefusal::EarlierFailure(shot),
        ExecuteRefusal::ShotNotHeld(shot),
    ] {
        let mut runner = runner(measure().on(Refuses {
            refusal: refusal.clone(),
        }));
        let error = failed(runner.run(&ShotSet::ids(0..4), SEED));
        assert_eq!(error.shot, shot, "{refusal:?}");
        assert!(
            matches!(&error.failure, Failure::Refused(why) if *why == refusal),
            "{error:?}"
        );
    }
}

/// The program measuring qubit 0 in one batch.
fn measure() -> Program {
    Program::new(&[("mz", 0)])
}

/// The errors of runs refused or failed, as a user reads them: each
/// message says the run was refused or failed, and names what is at fault
/// with what is wrong: the shots required and the most provided, the
/// fixed seed, the part's reason, the shot and the part's error, the part
/// that panicked and its message.
#[test]
fn a_runs_error_says_what_ended_it_and_names_what_is_at_fault() {
    let log = Log::default();
    let shots = ShotSet::ids(0..8);
    let coin = || CoinCopy::new();
    let limited = Bits::new(&log).with_resources(qubits(64).with_shots(4));
    let without_seeded_runs =
        Bits::new(&log).with_features(Features::unconstrained().with_seeded_runs(false));
    for (row, error, said) in [
        (
            "too many shots",
            runner(coin().on(limited)).run(&shots, SEED).map(drop),
            &["run refused", "shots", "8 required", "4 provided"][..],
        ),
        (
            "a seed the chain cannot take",
            runner(coin().on(without_seeded_runs))
                .run(&shots, SEED)
                .map(drop),
            &["run refused", "seeded runs", "not provided"],
        ),
        (
            "a part not ready",
            runner(coin().on(Bits::new(&log).busy("calibrating")))
                .run(&shots, SEED)
                .map(drop),
            &["run refused", "(part 1) is not ready: calibrating"],
        ),
        (
            "a shot failing",
            runner(coin().through(Trap::on(5)).on(Bits::new(&log)))
                .run(&shots, SEED)
                .map(drop),
            &["run failed", "shot 5", "the program trapped"],
        ),
        (
            "too many shots for a wrapped joined driver's adapter",
            runner(
                Renamed::new(coin().through(Stating::queue(4, &log)), "coin").on(Bits::new(&log)),
            )
            .run(&shots, SEED)
            .map(drop),
            &["run refused", "not ready", "8 required", "4 provided"],
        ),
        (
            "a part panicking",
            runner(measure().on(Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")))
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
