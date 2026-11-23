// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Executor`'s default states: no limit.

use crate::tests::fake_parts::{Log, QubitRegister, StatesNoLimits};
use crate::{ExecutorLimits, ExecutorReadings};

/// An executor that does not state its limits, either keeping the default
/// `provides` of its limits or stating none at all, states no limit
/// (`None`), so a chain is never refused at its boundary. A default
/// stating any limit, even one as wide as a type's largest value, fails.
#[test]
fn by_default_provides_limits_nothing() {
    let executor = StatesNoLimits(QubitRegister::new(8, &Log::default()));
    assert_eq!(executor.stating_limits().provided(), None);
    let executor = StatesNoLimits(QubitRegister::new(8, &Log::default()));
    assert_eq!(executor.provided(), None);
}
