// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Driver`'s default states: the run, as it arrives, and nothing of
//! its set.

use super::every_field_stated;
use crate::tests::example_sets::QubitLimited;
use crate::tests::fake_parts::{PassesTheRun, QubitProgram};
use crate::{Capabilities, Driver, InstructionSet, RunRequirement};

/// A driver whose shots need nothing of their own keeps the default
/// `requires`: it requires of the parts below what the run requires, every
/// field as the runner stated it, and its set's capabilities that state
/// nothing. A default dropping a field, adding one, or changing a value
/// fails.
#[test]
fn by_default_requires_passes_the_run_and_states_nothing_of_its_set() {
    let driver = PassesTheRun(QubitProgram::new(&[0], 1));
    let stated = every_field_stated();
    let run = RunRequirement {
        resources: *stated.resources(),
        features: *stated.features(),
    };
    let nothing_of_the_set = QubitLimited::unconstrained().set_capabilities();
    assert_eq!(
        Driver::requires(&driver, &run),
        Capabilities::new(run.resources, run.features, *nothing_of_the_set)
    );
}
