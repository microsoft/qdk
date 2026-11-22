// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What each role's trait does for a part that keeps its defaults: a part
//! states only what it needs or limits, so a default states nothing of its
//! own.

mod adapter;
mod driver;
mod executor;

use crate::tests::example_sets::{QubitLimited, Qubits};
use crate::{Capabilities, Features, Resources};

/// A requirement stating every field, a feature `false` and one `true`
/// among them, and the set's qubits, so that a default dropping or
/// changing any field differs from it.
fn every_field_stated() -> Capabilities<QubitLimited> {
    Capabilities::new(
        Resources::unconstrained()
            .with_shots(100)
            .with_instructions_per_batch(50),
        Features {
            replay: Some(true),
            seeded_runs: Some(false),
        },
        Qubits(4),
    )
}
