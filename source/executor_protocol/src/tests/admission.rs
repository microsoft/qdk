// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The admission's rule: what a driver's shots need against what an
//! executor offers.

use crate::{BlockType, Capabilities, Needs, UnmetNeed};

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

fn unmet_blocks(block_type: BlockType, needed: u32, offered: u32) -> UnmetNeed {
    UnmetNeed::Blocks {
        block_type,
        needed,
        offered,
    }
}

/// A program on 9 qubits, on an executor of 8: the qubits are unmet, with
/// both counts.
#[test]
fn more_blocks_than_offered_are_unmet() {
    let offer = Capabilities::unlimited().max_qubits(8);
    assert_eq!(
        Needs::qubits(9).fit(&offer),
        Err(vec![unmet_blocks(BlockType::qubit(), 9, 8)])
    );
}

/// As many qubits as offered fit: the most is inclusive.
#[test]
fn as_many_blocks_as_offered_fit() {
    let offer = Capabilities::unlimited().max_qubits(8);
    assert_eq!(Needs::qubits(8).fit(&offer), Ok(()));
}

/// A block type the executor does not list is offered 0 times.
#[test]
fn a_block_type_not_offered_is_offered_none() {
    let needs = Needs::default().blocks(carbon(), 2);
    assert_eq!(
        needs.fit(&Capabilities::unlimited()),
        Err(vec![unmet_blocks(carbon(), 2, 0)])
    );
}

/// An unlimited block type serves any count.
#[test]
fn an_unlimited_block_type_serves_any_count() {
    assert_eq!(
        Needs::qubits(u32::MAX).fit(&Capabilities::unlimited()),
        Ok(())
    );
}

/// A need of 0 blocks of a type is no need: an executor without that type
/// serves it.
#[test]
fn needing_no_block_of_a_type_needs_nothing_of_it() {
    let needs = Needs::qubits(1).blocks(carbon(), 0);
    assert_eq!(needs.fit(&Capabilities::unlimited()), Ok(()));
}

/// Readout parities need an executor that resolves them; one that does not
/// serves needs without them.
#[test]
fn readout_parities_need_an_executor_resolving_them() {
    let offer = Capabilities::unlimited().without_readout_parity();
    assert_eq!(
        Needs::qubits(1).with_readout_parity().fit(&offer),
        Err(vec![UnmetNeed::ReadoutParity])
    );
    assert_eq!(Needs::qubits(1).fit(&offer), Ok(()));
}

/// Every unmet need is listed at once, the blocks by type, then the
/// parities.
#[test]
fn every_unmet_need_is_listed() {
    let needs = Needs::qubits(9).blocks(carbon(), 3).with_readout_parity();
    let offer = Capabilities::unlimited()
        .max_qubits(8)
        .max_blocks(carbon(), 2)
        .without_readout_parity();
    assert_eq!(
        needs.fit(&offer),
        Err(vec![
            unmet_blocks(carbon(), 3, 2),
            unmet_blocks(BlockType::qubit(), 9, 8),
            UnmetNeed::ReadoutParity,
        ])
    );
}
