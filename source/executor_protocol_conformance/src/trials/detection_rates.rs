// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The detection rates of the drawn trials, for the kit's own tests: how
//! often the drawn trials alone catch each fake part. Run when
//! [`SEEDS_VARIABLE`] is set, it reruns each check the tests make, after
//! its own run, on the drawn trials of many seeds without trial 0, and
//! prints one line per check: the fake part it ran on, what the check gave
//! under the default trials, and how many seeds' drawn trials report the
//! part broken, or say they cannot judge it. A broken fake part caught by
//! some seeds only is fitted to some patterns; a correct one reported by
//! any seed is a false alarm. Its rerun repeats the check's calls, so a
//! test that counts them fails while it measures.
//!
//! Run it with
//! `EXECUTOR_PROTOCOL_CONFORMANCE_DETECTION_SEEDS=200 cargo test -p
//! executor_protocol_conformance -- --nocapture`, and gather the lines
//! that start with `detection`.

use std::cell::{Cell, RefCell};

use crate::splitmix64::mix;
use crate::{Nonconformance, Trials};

/// The environment variable that sets how many seeds the measure runs.
pub(crate) const SEEDS_VARIABLE: &str = "EXECUTOR_PROTOCOL_CONFORMANCE_DETECTION_SEEDS";

/// The environment variable that sets how many drawn trials each seed of
/// the measure runs, [`Trials::DEFAULT_COUNT`] if unset: 1 measures what
/// one trial catches.
pub(crate) const TRIALS_VARIABLE: &str = "EXECUTOR_PROTOCOL_CONFORMANCE_DETECTION_TRIALS";

thread_local! {
    /// The fake part made last on this thread: the one a check runs on.
    static PART: RefCell<String> = const { RefCell::new(String::new()) };
    /// How deep in [`Trials::run`] this thread is: only the outermost run
    /// is a check's own.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Names the fake part just made, for the measure's lines.
pub(crate) fn name_the_part(part: impl Into<String>) {
    PART.with(|name| *name.borrow_mut() = part.into());
}

/// Runs `trials` through `run`, and, if this is a check's own run of trial
/// 0 on and the measure is on, measures what the drawn trials catch.
pub(crate) fn measuring(
    trials: Trials,
    first: u32,
    mut run: impl FnMut(Trials) -> Result<(), Nonconformance>,
) -> Result<(), Nonconformance> {
    let depth = DEPTH.with(|depth| {
        depth.set(depth.get() + 1);
        depth.get()
    });
    let result = run(trials);
    let seeds = std::env::var(SEEDS_VARIABLE)
        .ok()
        .and_then(|seeds| seeds.parse::<u64>().ok());
    if let (1, 0, Some(seeds)) = (depth, first, seeds) {
        let count = std::env::var(TRIALS_VARIABLE)
            .ok()
            .and_then(|count| count.parse().ok())
            .unwrap_or(Trials::DEFAULT_COUNT);
        let (mut broken, mut not_judged) = (0, 0);
        for seed in 0..seeds {
            // An arbitrary salt, chosen, not measured: any fixed one gives
            // a fixed family of seeds, which `mix` spreads over 64 bits.
            match run(Trials::drawn_only(mix(seed ^ 0xD37E_C7ED), count)) {
                Ok(()) => {}
                Err(Nonconformance::Broken { .. }) => broken += 1,
                Err(Nonconformance::CannotJudge { .. }) => not_judged += 1,
            }
        }
        let outcome = match &result {
            Ok(()) => "passes".to_owned(),
            Err(Nonconformance::Broken { rules, .. }) => rules
                .iter()
                .map(|broken| format!("{:?}", broken.rule))
                .collect::<Vec<_>>()
                .join("+"),
            Err(Nonconformance::CannotJudge { .. }) => "cannot judge".to_owned(),
        };
        let part = PART.with(|name| std::mem::take(&mut *name.borrow_mut()));
        let test = std::thread::current().name().unwrap_or("?").to_owned();
        println!("detection\t{test}\t{part}\t{outcome}\t{broken}\t{not_judged}\t{seeds}");
    }
    DEPTH.with(|depth| depth.set(depth.get() - 1));
    result
}
