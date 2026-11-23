// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Driver`'s default states: the run, as it arrives, and nothing of
//! its set.

use super::every_field_stated;
use crate::tests::example_sets::QubitLimited;
use crate::tests::fake_parts::{PassesTheRun, QubitProgram};
use crate::{Capabilities, DriverReadings, DriverRequirements, InstructionSet, RunRequirement};

/// A driver whose shots need nothing of their own, either keeping the
/// default `requires` of its limits or stating none at all, requires of
/// the parts below what the run requires, every field as the runner
/// stated it, and its set's capabilities that need nothing. A default
/// dropping a field, adding one, or changing a value fails.
#[test]
fn by_default_requires_passes_the_run_and_states_nothing_of_its_set() {
    let stated = every_field_stated();
    let run = RunRequirement {
        resources: *stated.resources(),
        features: *stated.features(),
    };
    let passed = Capabilities::new(run.resources, run.features, QubitLimited::needs_nothing());
    let driver = PassesTheRun(QubitProgram::new(&[0], 1));
    assert_eq!(driver.stating_limits().required(&run), passed);
    let driver = PassesTheRun(QubitProgram::new(&[0], 1));
    assert_eq!(driver.required(&run), passed);
}
