// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Executor`'s default states: no limit.

use crate::tests::example_sets::QubitLimited;
use crate::tests::fake_parts::{Log, QubitRegister, StatesNoLimits};
use crate::{ExecutorLimits, ExecutorReadings, InstructionSet};

/// An executor that does not state its limits, either keeping the default
/// `provides` of its limits or stating none at all, limits nothing, so a
/// chain is never refused at its boundary. A default stating any limit,
/// even one as wide as a type's largest value, fails.
#[test]
fn by_default_provides_limits_nothing() {
    let executor = StatesNoLimits(QubitRegister::new(8, &Log::default()));
    assert_eq!(executor.limited().provided(), QubitLimited::unconstrained());
    let executor = StatesNoLimits(QubitRegister::new(8, &Log::default()));
    assert_eq!(executor.provided(), QubitLimited::unconstrained());
}
