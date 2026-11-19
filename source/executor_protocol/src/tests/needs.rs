// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The needs against an offer, at one boundary: which needs it meets.

use crate::{BlockType, Needs, Offer, UnmetNeed};

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

/// What the shots need against what the part below offers: a need is met
/// exactly when the offer covers it.
#[test]
fn a_need_is_met_exactly_when_its_offer_covers_it() {
    let eight = Offer::unlimited().max_qubits(8);
    let no_parity = Offer::unlimited().without_readout_parity();
    let not_replayable = Offer::unlimited().not_replayable();
    let not_seedable = Offer::unlimited().not_seedable();
    for (row, needs, offer, met) in [
        // A program on 9 qubits, on an executor of 8: the qubits are
        // unmet, with both counts.
        (
            "more qubits than offered",
            Needs::qubits(9),
            &eight,
            Err(vec![unmet_blocks(BlockType::qubit(), 9, 8)]),
        ),
        // The most is inclusive.
        (
            "as many qubits as offered",
            Needs::qubits(8),
            &eight,
            Ok(()),
        ),
        // A block type the executor does not list is offered 0 times.
        (
            "a block type not offered",
            Needs::default().blocks(carbon(), 2),
            &Offer::unlimited(),
            Err(vec![unmet_blocks(carbon(), 2, 0)]),
        ),
        (
            "an unlimited block type, for any count",
            Needs::qubits(u32::MAX),
            &Offer::unlimited(),
            Ok(()),
        ),
        // A need of 0 blocks of a type is no need: an executor without
        // that type serves it.
        (
            "no block of a type not offered",
            Needs::qubits(1).blocks(carbon(), 0),
            &Offer::unlimited(),
            Ok(()),
        ),
        // Readout parities need an executor that resolves them; one that
        // does not serves needs without them.
        (
            "readout parities, not resolved",
            Needs::qubits(1).with_readout_parity(),
            &no_parity,
            Err(vec![UnmetNeed::ReadoutParity]),
        ),
        (
            "no readout parities, not resolved",
            Needs::qubits(1),
            &no_parity,
            Ok(()),
        ),
        // A shot run again alone, with the run's seed, needs an executor
        // whose shots are replayable and that a run may seed, as
        // `unlimited`'s are; an offer that lacks either still meets needs
        // that run no shot again.
        (
            "replay, not replayable",
            Needs::qubits(1).with_replay(),
            &not_replayable,
            Err(vec![UnmetNeed::Replayable]),
        ),
        (
            "replay, not seedable",
            Needs::qubits(1).with_replay(),
            &not_seedable,
            Err(vec![UnmetNeed::Replayable]),
        ),
        (
            "replay, on an unlimited offer",
            Needs::qubits(1).with_replay(),
            &Offer::unlimited(),
            Ok(()),
        ),
        (
            "no replay, not replayable",
            Needs::qubits(1),
            &not_replayable,
            Ok(()),
        ),
        (
            "no replay, not seedable",
            Needs::qubits(1),
            &not_seedable,
            Ok(()),
        ),
    ] {
        assert_eq!(needs.fit(offer), met, "{row}");
    }
}

/// Every unmet need is listed at once, the blocks by type, then the
/// parities, then the replay.
#[test]
fn every_unmet_need_is_listed() {
    let needs = Needs::qubits(9)
        .blocks(carbon(), 3)
        .with_readout_parity()
        .with_replay();
    let offer = Offer::unlimited()
        .max_qubits(8)
        .max_blocks(carbon(), 2)
        .without_readout_parity()
        .not_replayable();
    assert_eq!(
        needs.fit(&offer),
        Err(vec![
            unmet_blocks(carbon(), 3, 2),
            unmet_blocks(BlockType::qubit(), 9, 8),
            UnmetNeed::ReadoutParity,
            UnmetNeed::Replayable,
        ])
    );
}

/// A refusal's message lists each unmet need, for a user who reads which
/// need to lower or which part to change: each names what is needed, and,
/// for blocks, the block type with both counts.
#[test]
fn an_unmet_need_says_what_is_needed_and_what_the_offer_allows() {
    for (unmet, named) in [
        (unmet_blocks(carbon(), 3, 2), &["carbon", "3", "2"][..]),
        (UnmetNeed::ReadoutParity, &["readout parit"]),
        (UnmetNeed::Replayable, &["replay"]),
    ] {
        let message = unmet.to_string();
        for word in named {
            assert!(message.contains(word), "{unmet:?} names {word}: {message}");
        }
    }
}
