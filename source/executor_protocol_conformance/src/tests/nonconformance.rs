// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A report's wording: what an implementer's failing test prints.

use super::fake_executor::{Breaks, FakeExecutor};
use super::{needs, script, trials};
use crate::executor::writes_every_readout;

/// A report names the part, each rule it broke, and each place, so that
/// the implementer's failing test says what to fix. This one is the
/// README's example report, word for word.
#[test]
fn a_report_names_the_part_the_rule_and_each_place() {
    let make = || FakeExecutor::breaking(Breaks::LeavesASlotUnwritten);
    let report = writes_every_readout(make, needs(), &script(), trials())
        .expect_err("the executor breaks the rule")
        .to_string();
    let expected = "\
`executor_protocol_conformance::tests::fake_executor::FakeExecutor` breaks 1 rule, in trial 0, on the fixed patterns:
- it writes every readout value of each batch it answers (`Execute::execute`):
  - execute 0, batch 0, row 1 (shot 1), slot 1: holds the value the kit wrote before the part ran, in each of 22 runs
  - execute 1, batch 0, row 1 (shot 1), slot 0: holds the value the kit wrote before the part ran, in each of 22 runs";
    assert_eq!(report.trim_end(), expected);
}
