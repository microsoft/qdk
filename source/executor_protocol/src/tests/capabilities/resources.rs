// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Resources` adds to the one rule of every kind: blocks keyed by
//! their type.

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

/// Each block type is a resource of its own, as qubits are one type among
/// them: a requirement of one type is checked against the provider's count
/// of that type alone. So a type provided 0 times is short of any blocks
/// of it, beside qubits that fit; and a type the provider does not state
/// takes any count of it, beside qubits that are short. A fit reading one
/// type's count as another's fails the first row; one reading a type the
/// provider does not state as 0, the second.
#[test]
fn each_block_type_is_a_resource_of_its_own() {
    for (row, required, provided, expected) in [
        (
            "a block type provided 0 times",
            Resources::unconstrained()
                .with_qubits(1)
                .with_blocks(carbon(), 2),
            Resources::unconstrained()
                .with_qubits(8)
                .with_blocks(carbon(), 0),
            vec![short_blocks(carbon(), 2, 0)],
        ),
        (
            "a block type the provider does not state",
            Resources::unconstrained()
                .with_qubits(9)
                .with_blocks(carbon(), 2),
            Resources::unconstrained().with_qubits(8),
            vec![short_blocks(BlockType::qubit(), 9, 8)],
        ),
    ] {
        assert_eq!(fit(required, &provided), Err(expected), "{row}");
    }
}
