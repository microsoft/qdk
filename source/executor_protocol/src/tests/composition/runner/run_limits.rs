// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The runner's own refusals, before any part is asked or runs: a run of
//! more shots than its chain takes, and a seeded run on a chain that is not
//! seedable.

use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::fake_parts::{Bits, CoinCopy, Log, Offering, Profiler, in_each_bracketing};
use crate::{Offer, RunRefusal, Runner, ShotSet};

use super::{SEED, bits_asked, refused, runner};

/// Runs one shot more than `most` on `runner`, refused before any part is
/// asked or runs, then `most` shots, a record each: a row of
/// [`a_run_of_more_shots_than_its_chain_takes_is_refused`], named `row`.
fn takes_at_most<D: Driver<Output: std::fmt::Debug>, E: Executor>(
    row: &str,
    mut runner: Runner<D, E>,
    most: u32,
    log: &Log,
) {
    let asked = bits_asked(log);
    let ran = log.count("Bits execute");
    let refusal = refused(runner.run(&ShotSet::ids(0..=most), SEED));
    assert!(
        matches!(refusal, RunRefusal::TooManyShots { asked, most: limit } if asked == most + 1 && limit == most),
        "{row}: {refusal:?}"
    );
    assert_eq!(bits_asked(log), asked, "{row}: no part asked");
    assert_eq!(log.count("Bits execute"), ran, "{row}: nothing ran");
    let records = runner
        .run(&ShotSet::ids(0..most), SEED)
        .unwrap_or_else(|error| panic!("{row}: a run of {most}: {error:?}"));
    assert_eq!(records.len(), most as usize, "{row}: a record per shot");
}

/// A chain runs at most the fewest shots any of its parts offers, in every
/// bracketing: a run of one more is refused before any part is asked or
/// runs, and a run of that many runs. So an adapter may lower the most
/// shots a run may have, but not raise it.
#[test]
fn a_run_of_more_shots_than_its_chain_takes_is_refused() {
    let log = Log::default();
    let bits = |most| {
        let offer = Offer::unlimited().max_qubits(64).max_shots(most);
        Bits::new(&log).offering(offer)
    };
    // An executor running 4 shots at most.
    let alone = runner(CoinCopy::new().on(bits(4)));
    takes_at_most("the executor alone", alone, 4, &log);
    // Bits under a job queue of 4 shots at most: the queue's limit holds
    // wherever the brackets go.
    in_each_bracketing!([d, queue, e] from {
        let d = CoinCopy::new();
        let queue = Offering::queue(4, &log);
        let e = Bits::new(&log);
    }, |chain, brackets| {
        takes_at_most(&format!("under a queue, {brackets}"), runner(chain), 4, &log);
    });
    // Bits running 1 shot at most, under a decoder that restates its offer
    // with no shot limit, or with a limit of 4: the bits' limit still holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating(64, &log);
        let e = bits(1);
    }, |chain, brackets| {
        let row = format!("a decoder with no limit, {brackets}");
        takes_at_most(&row, runner(chain), 1, &log);
    });
    in_each_bracketing!([d, decoder, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating_most_shots(64, 4, &log);
        let e = bits(1);
    }, |chain, brackets| {
        let row = format!("a decoder raising the limit, {brackets}");
        takes_at_most(&row, runner(chain), 1, &log);
    });
    // Bits running 8 shots at most, under a decoder that restates its offer
    // with a limit of 4: the decoder's lower limit holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating_most_shots(64, 4, &log);
        let e = bits(8);
    }, |chain, brackets| {
        let row = format!("a decoder lowering the limit, {brackets}");
        takes_at_most(&row, runner(chain), 4, &log);
    });
    // Bits running 8 shots at most, under a job queue of 4, under a decoder
    // that restates its offer with no shot limit: the queue's limit, though
    // neither the executor's nor the one reaching the driver, holds.
    in_each_bracketing!([d, decoder, queue, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating(64, &log);
        let queue = Offering::queue(4, &log);
        let e = bits(8);
    }, |chain, brackets| {
        let row = format!("a queue under a decoder, {brackets}");
        takes_at_most(&row, runner(chain), 4, &log);
    });
}

/// Runs `shots` shots with a seed on `runner`, refused before any part is
/// asked or runs, then with no seed: a row of
/// [`a_seeded_run_is_refused_unless_every_part_is_seedable`], named `row`.
fn refuses_a_seed<D: Driver<Output: std::fmt::Debug>, E: Executor>(
    row: &str,
    mut runner: Runner<D, E>,
    shots: u32,
    log: &Log,
) {
    let shots = ShotSet::ids(0..shots);
    let asked = bits_asked(log);
    let ran = log.count("Bits execute");
    let refusal = refused(runner.run(&shots, SEED));
    assert!(
        matches!(refusal, RunRefusal::NotSeedable),
        "{row}: {refusal:?}"
    );
    assert_eq!(bits_asked(log), asked, "{row}: no part asked");
    assert_eq!(log.count("Bits execute"), ran, "{row}: nothing ran");
    runner
        .run(&shots, None)
        .unwrap_or_else(|error| panic!("{row}: a run with no seed: {error:?}"));
}

/// A chain is seedable only if every part is, in every bracketing: a seeded
/// run on one that is not is refused before any part is asked or runs, and
/// a run with no seed runs. So an adapter may make a chain not seedable,
/// but cannot make a chain that is not seedable seedable.
#[test]
fn a_seeded_run_is_refused_unless_every_part_is_seedable() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let unseedable_bits = || {
        let offer = Offer::unlimited().max_qubits(64).not_seedable();
        Bits::new(&log).offering(offer)
    };
    // An executor that cannot be seeded.
    let alone = runner(CoinCopy::new().on(unseedable_bits()));
    refuses_a_seed("the executor alone", alone, 4, &log);
    // Bits that cannot be seeded, under a decoder that restates its offer
    // as seedable: the bits' seedability still holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating(64, &log);
        let e = unseedable_bits();
    }, |chain, brackets| {
        let row = format!("a decoder over bits, {brackets}");
        refuses_a_seed(&row, runner(chain), 1, &log);
    });
    // Seedable bits under a link that cannot be seeded, under a profiler.
    in_each_bracketing!([d, profiler, link, e] from {
        let d = CoinCopy::new();
        let profiler = Profiler::new(&counted, &log);
        let link = Offering::unseedable(&log);
        let e = Bits::new(&log);
    }, |chain, brackets| {
        let row = format!("a link under a profiler, {brackets}");
        refuses_a_seed(&row, runner(chain), 2, &log);
    });
    // Seedable bits under a link that cannot be seeded, under a decoder
    // that restates its offer as seedable: the link's seedability, though
    // neither the executor's nor the one reaching the driver, holds.
    in_each_bracketing!([d, decoder, link, e] from {
        let d = CoinCopy::new();
        let decoder = Offering::restating(64, &log);
        let link = Offering::unseedable(&log);
        let e = Bits::new(&log);
    }, |chain, brackets| {
        let row = format!("a link under a decoder, {brackets}");
        refuses_a_seed(&row, runner(chain), 2, &log);
    });
}
