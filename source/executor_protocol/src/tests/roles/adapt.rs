// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter's provided `adapt_each`: by default, every row once `adapt`
//! answers `Ok`, and none when it fails.

use super::execute::given_once_it_answered;
use crate::tests::composition::runner::{SEED, failed, runner};
use crate::tests::fake_parts::{
    BitFlip, Bits, Log, OutputsAsRowsCome, RowsGiven, Trap, in_each_bracketing,
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
