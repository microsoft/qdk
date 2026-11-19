// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A report's contents: what an implementer's failing test prints.

use executor_protocol::{ReadoutSlot, ShotId};

use super::fake_executor::{Breaks, FakeExecutor};
use super::{needs, script, trials};
use crate::executor::writes_every_readout;
use crate::{Nonconformance, Place, Rule};

/// A report names the part, each rule it broke, each place where it broke
/// it, and the trial that found it, so that the implementer's failing test
/// says what to fix. This one is the README's example report: an executor
/// leaving a readout slot of shot 1 unwritten in each `execute`. The test
/// checks what the report names, not its wording.
#[test]
fn a_report_names_the_part_the_rule_and_each_place() {
    let make = || FakeExecutor::breaking(Breaks::LeavesASlotUnwritten);
    let found = writes_every_readout(make, needs(), &script(), trials())
        .expect_err("the executor breaks the rule");
    let report = found.to_string();
    let Nonconformance::Broken { trial, rules, .. } = &found else {
        panic!("expected broken rules, got {found:?}");
    };
    assert!(
        report.contains("tests::fake_executor::FakeExecutor"),
        "the report names the part: {report}"
    );
    assert!(
        report.contains(&trial.to_string()),
        "the report names its trial: {report}"
    );
    assert_eq!(
        rules.iter().map(|broken| broken.rule).collect::<Vec<_>>(),
        [Rule::WritesEveryReadout],
        "{report}"
    );
    assert!(
        report.contains(&Rule::WritesEveryReadout.to_string()),
        "the report states the rule: {report}"
    );
    let slot = |execute, slot| Place::Slot {
        execute,
        batch: 0,
        row: 1,
        shot: ShotId::new(1),
        slot: ReadoutSlot::new(slot),
    };
    for place in [slot(0, 1), slot(1, 0)] {
        assert!(
            report.contains(&format!("{place}: ")),
            "the report names {place:?}: {report}"
        );
    }
}
