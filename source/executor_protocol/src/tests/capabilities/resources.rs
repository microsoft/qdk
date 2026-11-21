// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The resources a part requires against those the part below provides,
//! at one boundary: which amounts fit.

use crate::{BlockType, BoundaryCompatible, ResourceMismatch, Resources};

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

fn short_blocks(block_type: BlockType, required: u32, provided: u32) -> ResourceMismatch {
    ResourceMismatch::Blocks {
        block_type,
        required,
        provided,
    }
}

/// What a part's shots, runs and batches reach against what the part
/// below serves: an amount fits exactly when the provided one covers it,
/// or when either side does not state it.
#[test]
fn a_required_amount_fits_exactly_when_the_provided_one_covers_it() {
    let eight = Resources::new().with_qubits(8);
    for (row, required, provided, fit) in [
        // A program on 9 qubits, on an executor of 8: the qubits are
        // short, with both counts.
        (
            "more qubits than provided",
            Resources::new().with_qubits(9),
            &eight,
            Err(vec![short_blocks(BlockType::qubit(), 9, 8)]),
        ),
        // The most is inclusive.
        (
            "as many qubits as provided",
            Resources::new().with_qubits(8),
            &eight,
            Ok(()),
        ),
        // A provider states 0 for a block type it does not serve.
        (
            "a block type provided 0 times",
            Resources::new().with_blocks(carbon(), 2),
            &Resources::new().with_qubits(8).with_blocks(carbon(), 0),
            Err(vec![short_blocks(carbon(), 2, 0)]),
        ),
        // A requirement of 0 blocks of a type is no requirement.
        (
            "no block of a type provided 0 times",
            Resources::new().with_qubits(1).with_blocks(carbon(), 0),
            &Resources::new().with_qubits(8).with_blocks(carbon(), 0),
            Ok(()),
        ),
        // A provider that does not state a type serves any count of it.
        (
            "a block type the provider does not state",
            Resources::new()
                .with_qubits(u32::MAX)
                .with_blocks(carbon(), 2),
            &Resources::new(),
            Ok(()),
        ),
        // A requirement that does not state an amount fits any provision.
        (
            "nothing required",
            Resources::new(),
            &Resources::new().with_qubits(0).with_shots(0),
            Ok(()),
        ),
        (
            "more shots than provided",
            Resources::new().with_shots(5),
            &Resources::new().with_shots(4),
            Err(vec![ResourceMismatch::Shots {
                required: 5,
                provided: 4,
            }]),
        ),
        (
            "as many shots as provided",
            Resources::new().with_shots(4),
            &Resources::new().with_shots(4),
            Ok(()),
        ),
        (
            "more instructions per batch than provided",
            Resources::new().with_instructions_per_batch(3),
            &Resources::new().with_instructions_per_batch(2),
            Err(vec![ResourceMismatch::InstructionsPerBatch {
                required: 3,
                provided: 2,
            }]),
        ),
    ] {
        assert_eq!(required.fits(provided), fit, "{row}");
    }
}

/// Every short amount is listed at once, the blocks by type, then the
/// shots, then the instructions per batch, so that whoever builds the
/// chain fixes them all at once.
#[test]
fn every_short_amount_is_listed_in_order() {
    let required = Resources::new()
        .with_qubits(9)
        .with_blocks(carbon(), 3)
        .with_shots(10)
        .with_instructions_per_batch(4);
    let provided = Resources::new()
        .with_qubits(8)
        .with_blocks(carbon(), 2)
        .with_shots(5)
        .with_instructions_per_batch(1);
    assert_eq!(
        required.fits(&provided),
        Err(vec![
            short_blocks(carbon(), 3, 2),
            short_blocks(BlockType::qubit(), 9, 8),
            ResourceMismatch::Shots {
                required: 10,
                provided: 5
            },
            ResourceMismatch::InstructionsPerBatch {
                required: 4,
                provided: 1
            },
        ])
    );
}

/// A short amount's message, for a user who reads which amount to lower
/// or which part to change: it names what is short, with both counts.
#[test]
fn a_short_amount_says_what_is_required_and_what_is_provided() {
    for (short, named) in [
        (short_blocks(carbon(), 3, 2), &["carbon", "3", "2"][..]),
        (
            ResourceMismatch::Shots {
                required: 10,
                provided: 5,
            },
            &["shots", "10", "5"],
        ),
        (
            ResourceMismatch::InstructionsPerBatch {
                required: 4,
                provided: 1,
            },
            &["instructions per batch", "4", "1"],
        ),
    ] {
        let message = short.to_string();
        for word in named {
            assert!(message.contains(word), "{short:?} names {word}: {message}");
        }
    }
}
