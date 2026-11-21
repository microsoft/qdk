// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The resources a part requires against those the part below provides,
//! at one boundary: which amounts fit.

use super::carbon;
use crate::{
    BlockType, BoundaryCompatible, Capabilities, Features, Resource, ResourceMismatch, Resources,
};

/// Each amount `required` states above what `provided` states, as
/// [`Capabilities`](crate::Capabilities)' fit lists them.
fn fit(required: Resources, provided: &Resources) -> Result<(), Vec<ResourceMismatch>> {
    let fit = Capabilities::new(required, Features::unconstrained()).fits(&Capabilities::new(
        provided.clone(),
        Features::unconstrained(),
    ));
    fit.map(drop).map_err(|mismatch| mismatch.resources)
}

fn short_blocks(block_type: BlockType, required: u32, provided: u32) -> ResourceMismatch {
    ResourceMismatch {
        key: Resource::Blocks(block_type),
        required,
        provided,
    }
}

/// What a part's shots, runs and batches reach against what the part
/// below provides: an amount fits exactly when the provided one covers it,
/// or when either side does not state it.
#[test]
fn a_required_amount_fits_exactly_when_the_provided_one_covers_it() {
    let eight = Resources::unconstrained().with_qubits(8);
    for (row, required, provided, expected) in [
        // A program on 9 qubits, on an executor of 8: the qubits are
        // short, with both counts.
        (
            "more qubits than provided",
            Resources::unconstrained().with_qubits(9),
            &eight,
            Err(vec![short_blocks(BlockType::qubit(), 9, 8)]),
        ),
        // The most is inclusive.
        (
            "as many qubits as provided",
            Resources::unconstrained().with_qubits(8),
            &eight,
            Ok(()),
        ),
        // A provider states 0 for a block type it does not serve.
        (
            "a block type provided 0 times",
            Resources::unconstrained().with_blocks(carbon(), 2),
            &Resources::unconstrained()
                .with_qubits(8)
                .with_blocks(carbon(), 0),
            Err(vec![short_blocks(carbon(), 2, 0)]),
        ),
        // A requirement of 0 blocks of a type is no requirement.
        (
            "no block of a type provided 0 times",
            Resources::unconstrained()
                .with_qubits(1)
                .with_blocks(carbon(), 0),
            &Resources::unconstrained()
                .with_qubits(8)
                .with_blocks(carbon(), 0),
            Ok(()),
        ),
        // A provider that does not state a type takes any count of it.
        (
            "a block type the provider does not state",
            Resources::unconstrained()
                .with_qubits(u32::MAX)
                .with_blocks(carbon(), 2),
            &Resources::unconstrained(),
            Ok(()),
        ),
        // A requirement that does not state an amount fits any provision.
        (
            "nothing required",
            Resources::unconstrained(),
            &Resources::unconstrained().with_qubits(0).with_shots(0),
            Ok(()),
        ),
        (
            "more shots than provided",
            Resources::unconstrained().with_shots(5),
            &Resources::unconstrained().with_shots(4),
            Err(vec![ResourceMismatch {
                key: Resource::Shots,
                required: 5,
                provided: 4,
            }]),
        ),
        (
            "as many shots as provided",
            Resources::unconstrained().with_shots(4),
            &Resources::unconstrained().with_shots(4),
            Ok(()),
        ),
        (
            "more instructions per batch than provided",
            Resources::unconstrained().with_instructions_per_batch(3),
            &Resources::unconstrained().with_instructions_per_batch(2),
            Err(vec![ResourceMismatch {
                key: Resource::InstructionsPerBatch,
                required: 3,
                provided: 2,
            }]),
        ),
    ] {
        assert_eq!(fit(required, provided), expected, "{row}");
    }
}
