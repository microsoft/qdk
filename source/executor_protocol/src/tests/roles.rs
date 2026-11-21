// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What each role's trait does for a part that keeps its defaults: a part
//! states only what it needs or limits, so a default states nothing of its
//! own.

mod adapter;
mod driver;
mod executor;

use crate::{BlockType, Capabilities, Features, Resources};

/// A requirement stating every kind of field, a feature `false` among
/// them, so that a default dropping or changing any field differs from
/// it.
fn every_field_stated() -> Capabilities {
    Capabilities::new(
        Resources::unconstrained()
            .with_qubits(4)
            .with_blocks(BlockType::new("carbon"), 2)
            .with_shots(100)
            .with_instructions_per_batch(50),
        Features::unconstrained()
            .with_seeded_runs(false)
            .with_readout_parity(true),
    )
}
