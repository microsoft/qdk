// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part's shots require of the block set against what the part
//! below provides: block counts by type and readout parities, every
//! mismatch at once.

use crate::{BlockCapabilities, BlockCapabilitiesMismatch, BlockCountMismatch, BlockType};
use executor_protocol_core::BoundaryCompatible;

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

fn magic() -> BlockType {
    BlockType::new("magic")
}

/// The capabilities stating `blocks`, in the order given.
fn stating(blocks: &[(BlockType, u32)]) -> BlockCapabilities {
    (blocks.iter()).fold(BlockCapabilities::NONE, |stated, (block_type, n)| {
        stated.with_blocks(block_type.clone(), *n)
    })
}

/// The mismatch of `block_type`'s count, `required` above `provided`.
fn short(block_type: BlockType, required: u32, provided: u32) -> BlockCountMismatch {
    BlockCountMismatch {
        block_type,
        required,
        provided,
    }
}

/// The block counts `required` states above those `provided` states.
fn short_counts(
    required: &BlockCapabilities,
    provided: &BlockCapabilities,
) -> Vec<BlockCountMismatch> {
    required
        .fits(provided)
        .map_or_else(|mismatch| mismatch.blocks, |()| vec![])
}

/// A required count fits a provided one unless the provider states a
/// smaller one, so that a part states only what it needs or limits: a
/// block type a part does not state, it neither needs nor limits, on
/// either side. Each count required above the one provided is listed, with
/// both counts, every one at once and in the types' order, so that the
/// faults read alike whatever order the parts stated them in. A fit
/// refusing an equal count fails "an equal count"; one reading a type the
/// provider does not state as a limit fails "a type the provider does not
/// state"; one listing in another order fails "every larger count".
#[test]
fn a_required_count_fits_unless_a_stated_provided_count_is_smaller() {
    let qubit = BlockType::qubit;
    for (row, required, provided, expected) in [
        (
            "a type the requirer does not state",
            vec![],
            vec![(carbon(), 0)],
            vec![],
        ),
        (
            "a type the provider does not state",
            vec![(carbon(), u32::MAX)],
            vec![],
            vec![],
        ),
        (
            "an equal count",
            vec![(carbon(), 4)],
            vec![(carbon(), 4)],
            vec![],
        ),
        (
            "a smaller count",
            vec![(carbon(), 3)],
            vec![(carbon(), 4)],
            vec![],
        ),
        (
            "a larger count",
            vec![(carbon(), 5)],
            vec![(carbon(), 4)],
            vec![short(carbon(), 5, 4)],
        ),
        (
            "every larger count, a fitting one between them",
            vec![(qubit(), 9), (magic(), 1), (carbon(), 5)],
            vec![(magic(), 2), (qubit(), 8), (carbon(), 4)],
            vec![short(carbon(), 5, 4), short(qubit(), 9, 8)],
        ),
    ] {
        assert_eq!(
            short_counts(&stating(&required), &stating(&provided)),
            expected,
            "{row}"
        );
    }
}

/// Readout parities required fit unless the provider states it takes none:
/// `true` required over `false` provided is the one mismatch, and a side
/// that does not state them neither needs nor limits them.
#[test]
fn readout_parities_required_fit_unless_the_provider_takes_none() {
    let parity = |yes| BlockCapabilities::NONE.with_readout_parity(yes);
    let refused = Err(BlockCapabilitiesMismatch {
        blocks: vec![],
        readout_parity: true,
    });
    for (row, required, provided, expected) in [
        ("`true` over `false`", parity(true), parity(false), refused),
        ("`false` over `true`", parity(false), parity(true), Ok(())),
        ("`true` over `true`", parity(true), parity(true), Ok(())),
        (
            "`true` over a provider not stating them",
            parity(true),
            BlockCapabilities::NONE,
            Ok(()),
        ),
        (
            "a requirer not stating them over `false`",
            BlockCapabilities::NONE,
            parity(false),
            Ok(()),
        ),
    ] {
        assert_eq!(required.fits(&provided), expected, "{row}");
    }
}

/// Each block type is a count of its own, as qubits are one type among
/// them: a requirement of one type is checked against the provider's count
/// of that type alone. So a type provided 0 times is short of any blocks
/// of it, beside qubits that fit. A fit reading one type's count as
/// another's, or keying every block type alike, fails it.
#[test]
fn each_block_type_is_a_count_of_its_own() {
    let required = BlockCapabilities::NONE
        .with_qubits(1)
        .with_blocks(carbon(), 2);
    let provided = BlockCapabilities::NONE
        .with_qubits(8)
        .with_blocks(carbon(), 0);
    assert_eq!(
        short_counts(&required, &provided),
        vec![short(carbon(), 2, 0)]
    );
}

/// A part restating a count states the last one: a builder keeping the
/// first fails it.
#[test]
fn the_last_count_stated_of_a_type_is_the_one_that_holds() {
    let provided = BlockCapabilities::NONE.with_qubits(2).with_qubits(8);
    assert_eq!(provided.blocks(&BlockType::qubit()), Some(8));
    assert_eq!(
        BlockCapabilities::NONE.with_qubits(8).fits(&provided),
        Ok(())
    );
}

/// Shots requiring 9 qubits, 3 carbon blocks and readout parities, over a
/// part taking 8 qubits, 2 carbon blocks and no readout parity.
fn every_kind_short() -> (BlockCapabilities, BlockCapabilities) {
    let required = BlockCapabilities::NONE
        .with_qubits(9)
        .with_blocks(carbon(), 3)
        .with_readout_parity(true);
    let provided = BlockCapabilities::NONE
        .with_readout_parity(false)
        .with_blocks(carbon(), 2)
        .with_qubits(8);
    (required, provided)
}

/// Whoever builds the chain fixes every mismatch at once: each short
/// count, by type, then readout parities.
#[test]
fn every_mismatch_is_listed_at_once_in_order() {
    let (required, provided) = every_kind_short();
    assert_eq!(
        required.fits(&provided),
        Err(BlockCapabilitiesMismatch {
            blocks: vec![short(carbon(), 3, 2), short(BlockType::qubit(), 9, 8)],
            readout_parity: true,
        })
    );
}

/// A mismatch's message names what is at fault, for a user who reads which
/// requirement to lower, or which part to change: each of its mismatches,
/// a short count with its type and both counts.
#[test]
fn a_mismatch_names_what_is_at_fault() {
    let (required, provided) = every_kind_short();
    let message = required
        .fits(&provided)
        .expect_err("nothing fits")
        .to_string();
    for named in [
        &["carbon", "3", "2"][..],
        &["qubit", "9", "8"],
        &["readout parit"],
    ] {
        let piece = message
            .split("; ")
            .find(|piece| named.iter().all(|word| piece.contains(word)));
        assert!(piece.is_some(), "one mismatch names {named:?}: {message}");
    }
}
