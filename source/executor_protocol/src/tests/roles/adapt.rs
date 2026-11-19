// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter's provided methods: `adapt_each`, by default every row once
//! `adapt` answers `Ok`, and none when it fails; and `adapt_stream`, by
//! default a stream run in pieces of the most the adapter offers above.

use std::cell::Cell;
use std::rc::Rc;

use super::execute::{PROGRAM, bits, given_once_it_answered, pieces_made, pieces_run};
use crate::tests::composition::runner::{SEED, failed, runner};
use crate::tests::fake_parts::{
    BitFlip, Bits, Log, Offering, OutputsAsRowsCome, Program, RowsGiven, SeesPieces, Sending,
    Spread, Trap, in_each_bracketing,
};
use crate::{Adapter, Driver, Executor, ShotConditions, ShotSet};

/// A driver calling `execute_each` for 6 shots through an adapter that
/// draws noise and keeps the default `adapt_each`, over bits that give rows
/// as they are final, wherever the brackets go: the driver is given every
/// row once, batch by batch, once the adapter has answered, with the values
/// the `execute` answers.
#[test]
fn adapt_each_by_default_gives_every_row_once_adapt_answers() {
    let shots = ShotSet::ids(0..6);
    in_each_bracketing!([d, a, e] from {
        let (given, log) = (RowsGiven::default(), Log::default());
        let d = Driver::new(OutputsAsRowsCome::new(&given, &log));
        let a = Adapter::new(BitFlip::new(0.3, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let records = runner(chain).run(&shots, SEED).expect("the run succeeds");
        let answered = (&records[..], &[ShotConditions::NONE; 6][..]);
        given_once_it_answered(&format!("noise, {brackets}"), answered, &given, &log);
    });
}

/// A driver calling `execute_each` for 6 shots through an adapter that
/// keeps the default `adapt_each` and fails its `adapt`, as a trap on shot
/// 0: the run fails, and the driver is given no row.
#[test]
fn adapt_each_by_default_gives_no_row_when_adapt_fails() {
    let (given, log) = (RowsGiven::default(), Log::default());
    let mut adapter_fails = runner(
        Driver::new(OutputsAsRowsCome::new(&given, &log))
            >> Adapter::new(Trap::on(0))
            >> Executor::new(Bits::new(&log)),
    );
    failed(adapter_fails.run(&ShotSet::ids(0..6), SEED));
    assert_eq!(*given.borrow(), [], "no row");
}

/// A driver sending a program as one stream, each instruction made only
/// when pulled, through an adapter that keeps the default `adapt_stream`,
/// wherever the brackets go: the adapter is the stream's receiver, so its
/// pieces hold at most the most it offers above, a short piece only at
/// the end: 3 for one offering at most 3 over bits taking 5, not the
/// bits' 5; the bits' most, 5, 4, 12, 13 or none set, for one offering
/// what the bits offer. An empty stream runs no piece.
#[test]
fn adapt_stream_by_default_runs_pieces_of_the_most_it_offers_above() {
    for (most_above, bits_most, program, pieces) in [
        (Some(3), Some(5), &PROGRAM[..], &[3, 3, 3, 3][..]),
        (None, Some(5), &PROGRAM, &[5, 5, 2]),
        (None, Some(4), &PROGRAM, &[4, 4, 4]),
        (None, Some(12), &PROGRAM, &[12]),
        (None, Some(13), &PROGRAM, &[12]),
        (None, None, &PROGRAM, &[12]),
        (None, Some(5), &[], &[]),
    ] {
        let seen = |log: &Log, brackets: &str| {
            assert_eq!(
                pieces_run(log),
                pieces_made(pieces),
                "an adapter offering at most {most_above:?} over bits taking \
                 {bits_most:?}, {} instructions, {brackets}",
                program.len()
            );
        };
        let stream = |produced: &Rc<Cell<usize>>| {
            Driver::new(Program::new(program).sending_as(Sending::stream_counted(produced)))
        };
        let shots = ShotSet::ids(0..3);
        if let Some(most_above) = most_above {
            in_each_bracketing!([d, a, e] from {
                let (log, produced) = (Log::default(), Rc::new(Cell::new(0)));
                let d = stream(&produced);
                let a = Adapter::new(Offering::buffering(most_above, &log));
                let e = Executor::new(SeesPieces::new(bits(&log, bits_most), &produced, &log));
            }, |chain, brackets| {
                runner(chain).run(&shots, SEED).expect("the run succeeds");
                seen(&log, brackets);
            });
        } else {
            in_each_bracketing!([d, a, e] from {
                let (log, produced) = (Log::default(), Rc::new(Cell::new(0)));
                let d = stream(&produced);
                let a = Adapter::new(Spread::new(1, &log));
                let e = Executor::new(SeesPieces::new(bits(&log, bits_most), &produced, &log));
            }, |chain, brackets| {
                runner(chain).run(&shots, SEED).expect("the run succeeds");
                seen(&log, brackets);
            });
        }
    }
}
