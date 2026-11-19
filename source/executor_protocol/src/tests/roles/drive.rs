// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver's provided `drive_each`: by default, every output once `drive`
//! answers `Ok`.

use crate::tests::composition::runner::{SEED, runner};
use crate::tests::fake_parts::{Bits, CoinCopy, Log, Values};
use crate::{Driver, Executor, ShotId, ShotSet};

/// A run by `run_each` of 6 shots on a driver that keeps the default
/// `drive_each`: `each` is given every shot's output once `drive` has
/// answered, after the bits' last `execute`, once each, in the order of the
/// shots, each the record `run` gives.
#[test]
fn drive_each_by_default_gives_every_output_once_drive_answers() {
    let shots = ShotSet::ids(0..6);
    let log = Log::default();
    let coin_copy = || runner(Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(&log)));
    let records = coin_copy().run(&shots, SEED).expect("the run succeeds");
    let mut given: Vec<(ShotId, Values)> = Vec::new();
    coin_copy()
        .run_each(&shots, SEED, &mut |id, values| {
            log.push("output");
            given.push((id, values));
        })
        .expect("the run succeeds");
    let entries = log.entries();
    let last_execute = entries
        .iter()
        .rposition(|entry| entry == "Bits execute")
        .expect("the bits ran");
    let first_output = entries
        .iter()
        .position(|entry| entry == "output")
        .expect("an output was given");
    assert!(
        last_execute < first_output,
        "outputs at the end, {entries:?}"
    );
    let (ids, values): (Vec<ShotId>, Vec<Values>) = given.into_iter().unzip();
    assert_eq!(ids, shots.iter().collect::<Vec<_>>(), "once each, in order");
    assert_eq!(values, records, "the records `run` gives");
}
