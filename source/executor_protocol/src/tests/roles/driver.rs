// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Driver`'s default states: the run, as it arrives.

use super::every_field_stated;
use crate::Driver;
use crate::tests::fake_parts::{CoinCopy, PassesTheRun};

/// A driver whose shots need nothing of their own keeps the default
/// `requires`: it requires of the parts below what the run requires, every
/// field as the runner stated it. A default dropping a field, adding one,
/// or changing a value fails.
#[test]
fn by_default_requires_passes_the_run_unchanged() {
    let driver = PassesTheRun(CoinCopy::new());
    let run = every_field_stated();
    assert_eq!(Driver::requires(&driver, &run), run);
}
