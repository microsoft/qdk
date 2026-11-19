// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A [`Script`]'s refusals: a script the kit cannot send leaves the check
//! unable to judge the part, saying why, before any run.

use super::fake_executor::{CoinGatesAndReset, FakeExecutor};
use super::{gates, needs, shots, trials, why_cannot_judge};
use crate::Script;
use crate::executor::writes_every_readout;

/// A script that names no shot, puts a shot in two batches of an
/// `execute`, or holds an instruction outside the set the part accepts,
/// cannot be sent, so the check names why rather than run it.
#[test]
fn a_script_the_kit_cannot_send_cannot_judge_the_part() {
    let reset = CoinGatesAndReset::set()
        .instruction("reset", [0], &[])
        .expect("an instruction of CoinGatesAndReset");
    let read = || gates(&[("h", 0), ("mz", 0)]);
    let cases = [
        (Script::new(), "names no shot"),
        (Script::new().execute([]), "names no shot"),
        (
            Script::new().execute([(shots([0, 1]), read()), (shots([1, 2]), read())]),
            "execute 0 puts shot 1 in two batches",
        ),
        (
            Script::new()
                .execute([(shots([0, 1]), read())])
                .execute([(shots([0]), read()), (shots([2]), vec![reset])]),
            "execute 1, batch 1 holds an instruction outside the set",
        ),
    ];
    for (script, reason) in cases {
        let why = why_cannot_judge(writes_every_readout(
            FakeExecutor::new,
            needs(),
            &script,
            trials(),
        ));
        assert!(why.contains(reason), "{reason}: {why}");
    }
}
