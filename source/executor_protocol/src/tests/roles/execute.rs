// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor's provided methods: `execute_each`, by default every row
//! once `execute` answers `Ok` and none when it fails; a shot's readouts
//! however its instructions are split over calls, by `execute` or
//! `execute_stream`; a stream run in pieces of at most the receiver's
//! most, each pulled only once the one before it answered; an executor's
//! own `execute_stream` reached through the parts that forward it; and
//! the row `execute_each` gives a shot whose instructions span calls.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::tests::composition::runner::{SEED, failed, runner};
use crate::tests::fake_parts::{
    Bits, Later, Log, OutputsAsRowsCome, Profiler, Program, RowsGiven, SeesPieces, Sending, Spread,
    Stream, Values, in_each_bracketing, sorted_values,
};
use crate::{Adapter, Driver, Executor, Offer, ShotConditions, ShotSet};

/// A program of 12 instructions on 3 qubits that tosses, flips and reads
/// them in turn, its reads spread through it, so that each cut falls
/// between a shot's draws and reads: what the tests of split programs
/// send.
const PROGRAM: [(&str, u32); 12] = [
    ("h", 0),
    ("mz", 0),
    ("x", 1),
    ("h", 2),
    ("mz", 1),
    ("mz", 2),
    ("x", 0),
    ("mz", 0),
    ("h", 1),
    ("x", 2),
    ("mz", 1),
    ("mz", 2),
];

/// The seed the cuts of [`PROGRAM`] are drawn from, named in each
/// failure, so that a failing split is replayed.
const CUTS_SEED: u64 = 1119;

/// Bits taking at most `most` instructions per batch, if some.
fn bits(log: &Log, most: Option<u32>) -> Bits {
    let offer = Offer::unlimited().max_qubits(64).without_readout_parity();
    Bits::new(log).offering(match most {
        Some(most) => offer.max_instructions(most),
        None => offer,
    })
}

/// `pieces - 1` distinct cuts of [`PROGRAM`], each before a position in
/// `1..12`, drawn from `seed`, in order.
fn seeded_cuts(seed: u64, pieces: usize) -> Vec<usize> {
    let mut random = Stream(seed);
    let mut cuts = BTreeSet::new();
    let positions = u64::try_from(PROGRAM.len() - 1).expect("a short program");
    while cuts.len() + 1 < pieces {
        let drawn = usize::try_from(random.next() % positions).expect("below the program's length");
        cuts.insert(1 + drawn);
    }
    cuts.into_iter().collect()
}

/// The records of [`PROGRAM`] sent whole, in one `execute`, on `shots`.
fn sent_whole(shots: &ShotSet) -> Vec<(Values, ShotConditions)> {
    let log = Log::default();
    runner(Driver::new(Program::new(&PROGRAM)) >> Executor::new(bits(&log, None)))
        .run(shots, SEED)
        .expect("the run succeeds")
}

/// A driver sending [`PROGRAM`] whole, then in 2 to 5 `execute`s cut at
/// seeded points, then as one stream, to receivers taking at most 1, 2,
/// 5, 12 instructions, or any count, and executors that run the stream
/// themselves or cut it by the default, directly, through an adapter that
/// forwards streams or one that cuts them by its default, in each
/// bracketing, and behind `&mut`: each shot's readouts are those of the
/// program sent whole, since its state is kept between calls.
#[test]
fn a_shots_readouts_are_the_same_however_its_instructions_are_split_over_calls() {
    let shots = ShotSet::ids(0..5);
    let whole = sent_whole(&shots);
    let mut sendings: Vec<(String, Sending, Option<u32>)> = (2..=5)
        .map(|pieces| {
            let seed = CUTS_SEED + pieces as u64;
            let cuts = seeded_cuts(seed, pieces);
            let sent = format!("{pieces} `execute`s cut at {cuts:?} (seed {seed})");
            (sent, Sending::Pieces(cuts), None)
        })
        .collect();
    for most in [Some(1), Some(2), Some(5), Some(12), None] {
        let sent = format!("a stream, to a receiver taking at most {most:?}");
        sendings.push((sent, Sending::stream(), most));
    }
    for (sent, sending, most) in sendings {
        let program = || Driver::new(Program::new(&PROGRAM).sending_as(sending.clone()));
        let same = |run: Result<Vec<(Values, ShotConditions)>, _>, chain: &str| {
            let records: Vec<_> = run.expect("the run succeeds");
            assert_eq!(records, whole, "{sent}, {chain}");
        };
        let (log, counted) = (Log::default(), Rc::new(Cell::new(0)));
        same(
            runner(program() >> Executor::new(bits(&log, most))).run(&shots, SEED),
            "an executor running streams itself",
        );
        same(
            runner(program() >> Executor::new(Later(bits(&log, most)))).run(&shots, SEED),
            "an executor cutting streams by the default",
        );
        in_each_bracketing!([d, a, e] from {
            let d = program();
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(Later(bits(&log, most)));
        }, |chain, brackets| {
            let chain_named = format!("an adapter forwarding streams, {brackets}");
            same(runner(chain).run(&shots, SEED), &chain_named);
        });
        in_each_bracketing!([d, a, e] from {
            let d = program();
            let a = Adapter::new(Spread::new(1, &log));
            let e = Executor::new(bits(&log, most));
        }, |chain, brackets| {
            let chain_named = format!("an adapter cutting streams by the default, {brackets}");
            same(runner(chain).run(&shots, SEED), &chain_named);
        });
        let mut profiler = Profiler::new(&counted, &log);
        let mut later = Later(bits(&log, most));
        same(
            runner(program() >> Adapter::new(&mut profiler) >> Executor::new(&mut later))
                .run(&shots, SEED),
            "parts behind `&mut`",
        );
    }
}

/// A driver sending [`PROGRAM`] as one stream, each instruction made only
/// when pulled, to an executor that cuts it by the default and takes at
/// most 5, 4, 12, 13 instructions, or any count, directly or through an
/// adapter that forwards streams, or one that cuts them by its default,
/// in each bracketing: each piece holds at most the receiver's most, a
/// short piece only at the end, and when each runs, the driver has made
/// only the instructions of the pieces so far, so no part holds more than
/// a piece of the stream.
#[test]
fn a_stream_is_run_in_pieces_of_the_receivers_most_each_pulled_as_it_runs() {
    let shots = ShotSet::ids(0..3);
    for (most, pieces) in [
        (Some(5), &[5, 5, 2][..]),
        (Some(4), &[4, 4, 4]),
        (Some(12), &[12]),
        (Some(13), &[12]),
        (None, &[12]),
    ] {
        let expected: Vec<String> = pieces
            .iter()
            .scan(0, |produced, &piece| {
                *produced += piece;
                Some(format!("execute of {piece}, {produced} produced"))
            })
            .collect();
        let seen = |log: &Log, chain: &str| {
            let seen: Vec<String> = log
                .entries()
                .into_iter()
                .filter(|entry| entry.starts_with("execute of"))
                .collect();
            assert_eq!(seen, expected, "at most {most:?}, {chain}");
        };
        let stream = |produced: &Rc<Cell<usize>>| {
            Driver::new(Program::new(&PROGRAM).sending_as(Sending::stream_counted(produced)))
        };
        let (log, produced) = (Log::default(), Rc::new(Cell::new(0)));
        runner(
            stream(&produced) >> Executor::new(SeesPieces::new(bits(&log, most), &produced, &log)),
        )
        .run(&shots, SEED)
        .expect("the run succeeds");
        seen(&log, "no adapter");
        let counted = Rc::new(Cell::new(0));
        in_each_bracketing!([d, a, e] from {
            let (log, produced) = (Log::default(), Rc::new(Cell::new(0)));
            let d = stream(&produced);
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(SeesPieces::new(bits(&log, most), &produced, &log));
        }, |chain, brackets| {
            runner(chain).run(&shots, SEED).expect("the run succeeds");
            seen(&log, &format!("an adapter forwarding streams, {brackets}"));
        });
        in_each_bracketing!([d, a, e] from {
            let (log, produced) = (Log::default(), Rc::new(Cell::new(0)));
            let d = stream(&produced);
            let a = Adapter::new(Spread::new(1, &log));
            let e = Executor::new(SeesPieces::new(bits(&log, most), &produced, &log));
        }, |chain, brackets| {
            runner(chain).run(&shots, SEED).expect("the run succeeds");
            seen(&log, &format!("an adapter cutting streams by the default, {brackets}"));
        });
    }
}

/// A driver sending one stream to bits that run streams themselves,
/// directly, through one or two adapters that forward streams, in each
/// bracketing, and behind `&mut`: the bits' own `execute_stream` gets the
/// stream, once, never cut into `execute`s, and so does each adapter's own
/// `adapt_stream`, so a part that takes streams whole is never bypassed.
#[test]
fn an_executors_own_execute_stream_is_reached_through_every_part_that_forwards_it() {
    let shots = ShotSet::ids(0..3);
    let stream = || Driver::new(Program::new(&PROGRAM).sending_as(Sending::stream()));
    let reached = |log: &Log, adapters: usize, chain: &str| {
        let calls = (
            log.count("Profiler adapt_stream"),
            log.count("Bits execute_stream"),
            log.count("Bits execute"),
        );
        assert_eq!(calls, (adapters, 1, 0), "{chain}: {:?}", log.entries());
    };
    let log = Log::default();
    runner(stream() >> Executor::new(Bits::new(&log)))
        .run(&shots, SEED)
        .expect("the run succeeds");
    reached(&log, 0, "no adapter");
    let counted = Rc::new(Cell::new(0));
    in_each_bracketing!([d, a, e] from {
        let log = Log::default();
        let d = stream();
        let a = Adapter::new(Profiler::new(&counted, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        runner(chain).run(&shots, SEED).expect("the run succeeds");
        reached(&log, 1, brackets);
    });
    in_each_bracketing!([d, a, b, e] from {
        let log = Log::default();
        let d = stream();
        let a = Adapter::new(Profiler::new(&counted, &log));
        let b = Adapter::new(Profiler::new(&counted, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        runner(chain).run(&shots, SEED).expect("the run succeeds");
        reached(&log, 2, brackets);
    });
    let log = Log::default();
    let (mut profiler, mut bits) = (Profiler::new(&counted, &log), Bits::new(&log));
    runner(stream() >> Adapter::new(&mut profiler) >> Executor::new(&mut bits))
        .run(&shots, SEED)
        .expect("the run succeeds");
    reached(&log, 1, "parts behind `&mut`");
}

/// A driver sending [`PROGRAM`] over 2 to 5 `execute_each`s cut at
/// seeded points, to bits that give rows as they are final, and to bits
/// that answer later, by the default: each call gives each shot the row of
/// that call's instructions alone, so a shot's rows, in the order of the
/// calls, are its row of the program sent whole.
#[test]
fn execute_each_gives_a_shot_whose_instructions_span_calls_each_calls_row() {
    let shots = ShotSet::ids(0..4);
    let whole = sent_whole(&shots);
    for pieces in 2..=5 {
        let seed = CUTS_SEED + pieces as u64;
        let cuts = seeded_cuts(seed, pieces);
        let mut expected = Vec::new();
        let ends = cuts.iter().copied().chain([PROGRAM.len()]);
        let (mut start, mut read) = (0, 0);
        for (piece, end) in ends.enumerate() {
            let reads = PROGRAM[start..end]
                .iter()
                .filter(|&&(mnemonic, _)| mnemonic == "mz")
                .count();
            for (position, (values, _)) in whole.iter().enumerate() {
                expected.push((piece, position, values[read..read + reads].to_vec()));
            }
            (start, read) = (end, read + reads);
        }
        let given_by = |given: &RowsGiven| {
            let mut given: Vec<(usize, usize, Values)> = given
                .borrow()
                .iter()
                .map(|(piece, position, values, _)| (*piece, *position, values.clone()))
                .collect();
            given.sort_by_key(|&(piece, position, _)| (piece, position));
            given
        };
        let program = |given: &RowsGiven| {
            Driver::new(
                Program::new(&PROGRAM).sending_as(Sending::PiecesEach(cuts.clone(), given.clone())),
            )
        };
        let (log, given) = (Log::default(), RowsGiven::default());
        runner(program(&given) >> Executor::new(bits(&log, None)))
            .run(&shots, SEED)
            .expect("the run succeeds");
        let case = format!("{pieces} calls cut at {cuts:?} (seed {seed})");
        assert_eq!(given_by(&given), expected, "{case}, rows as they are final");
        let (log, given) = (Log::default(), RowsGiven::default());
        runner(program(&given) >> Executor::new(Later(bits(&log, None))))
            .run(&shots, SEED)
            .expect("the run succeeds");
        assert_eq!(given_by(&given), expected, "{case}, by the default");
    }
}

/// A driver calling `execute_each` for 6 shots on an executor that answers
/// later and keeps the default `execute_each`, which finds no shot suspect,
/// every shot, or the odd shots: the driver is given every row once, batch
/// by batch, once the executor has answered, each with the values and
/// conditions the `execute` answers for its shot.
#[test]
fn execute_each_by_default_gives_every_row_once_execute_answers() {
    let shots = ShotSet::ids(0..6);
    let (none, rejected) = (ShotConditions::NONE, ShotConditions::REJECTED);
    let as_built: fn(Bits) -> Bits = |bits| bits;
    for (case, variant, conditions) in [
        ("an executor answering later", as_built, [none; 6]),
        (
            "one finding every shot suspect",
            Bits::losing,
            [rejected; 6],
        ),
        (
            "one finding the odd shots suspect",
            Bits::losing_odd_shots,
            [none, rejected, none, rejected, none, rejected],
        ),
    ] {
        let (given, log) = (RowsGiven::default(), Log::default());
        let bits = variant(Bits::new(&log));
        let records =
            runner(Driver::new(OutputsAsRowsCome::new(&given, &log)) >> Executor::new(Later(bits)))
                .run(&shots, SEED)
                .expect("the run succeeds");
        given_once_it_answered(case, (&records, &conditions), &given, &log);
    }
}

/// A driver calling `execute_each` for 6 shots on an executor that keeps
/// the default `execute_each` and fails its `execute` once it has run the
/// shots, as a device whose link drops before it answers: the run fails,
/// and the driver is given no row.
#[test]
fn execute_each_by_default_gives_no_row_when_execute_fails() {
    let (given, log) = (RowsGiven::default(), Log::default());
    let mut executor_fails = runner(
        Driver::new(OutputsAsRowsCome::new(&given, &log))
            >> Executor::new(Later(Bits::new(&log).failing())),
    );
    failed(executor_fails.run(&ShotSet::ids(0..6), SEED));
    assert_eq!(*given.borrow(), [], "no row");
}

/// Checks that the driver was given every row of its 6 shots once, batch
/// by batch, after the bits' last `execute`, with the values of `records`
/// and the conditions of `conditions`, each shot's in the order of the
/// shots.
pub(super) fn given_once_it_answered(
    case: &str,
    (records, conditions): (&[Values], &[ShotConditions]),
    given: &RowsGiven,
    log: &Log,
) {
    let entries = log.entries();
    let last_execute = entries
        .iter()
        .rposition(|entry| entry == "Bits execute")
        .expect("the bits ran");
    let first_row = entries
        .iter()
        .position(|entry| entry.starts_with("row "))
        .expect("a row was given");
    assert!(
        last_execute < first_row,
        "{case}: rows at the end, {entries:?}"
    );
    let order: Vec<(usize, usize)> = given.borrow().iter().map(|&(b, p, ..)| (b, p)).collect();
    assert_eq!(
        order,
        [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)],
        "{case}: once each, batch by batch"
    );
    assert_eq!(sorted_values(given), records, "{case}: the final values");
    let given_conditions: Vec<ShotConditions> = given.borrow().iter().map(|row| row.3).collect();
    assert_eq!(given_conditions, conditions, "{case}: the final conditions");
}
