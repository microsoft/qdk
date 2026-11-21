// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires against what the part below provides, at one
//! boundary: every mismatch of each kind, at once. The one rule of
//! `statements.rs`, which every kind follows, is pinned per kind, in
//! `features.rs` and `resources.rs`.

mod features;
mod resources;

/// The boundary a run's batches cross.
mod boundary {
    mod batch_limits;
}

use crate::tests::fake_parts::{CoinGates, ParityGates};
use crate::{
    BlockType, BoundaryCompatible, Capabilities, CapabilitiesMismatch, Feature, FeatureMismatch,
    Features, InstructionMismatch, Resource, ResourceMismatch, Resources,
};

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

/// Capabilities of [`CoinGates`] stating `resources` and `features`.
fn coin(resources: Resources, features: Features) -> Capabilities {
    Capabilities::new(CoinGates::set(), resources, features)
}

/// A driver sending [`ParityGates`] on 9 qubits, 3 carbon blocks, runs of
/// 10 shots and batches of 4, using every feature, over an executor
/// accepting [`CoinGates`] on 8 qubits and 2 carbon blocks, runs of 5 and
/// batches of 1, serving no feature.
fn every_kind_short() -> (Capabilities, Capabilities) {
    let required = Capabilities::new(
        ParityGates::set(),
        Resources::unconstrained()
            .with_qubits(9)
            .with_blocks(carbon(), 3)
            .with_shots(10)
            .with_instructions_per_batch(4),
        Features::unconstrained()
            .with_seeded_runs(true)
            .with_replay(true)
            .with_readout_parity(true),
    );
    let provided = coin(
        Resources::unconstrained()
            .with_qubits(8)
            .with_blocks(carbon(), 2)
            .with_shots(5)
            .with_instructions_per_batch(1),
        Features::unconstrained()
            .with_readout_parity(false)
            .with_replay(false)
            .with_seeded_runs(false),
    );
    (required, provided)
}

/// Whoever builds the chain fixes every mismatch at once: the instructions
/// that do not fit, as the instruction set's fit lists them, then each
/// short amount, the blocks by type, the shots, the instructions per
/// batch, then each feature not served, readout parity, replay, seeded
/// runs.
#[test]
fn every_mismatch_of_each_kind_is_listed_at_once_in_order() {
    let (required, provided) = every_kind_short();
    let mismatch = required.fits(&provided).expect_err("nothing fits");
    let short = |key, required, provided| ResourceMismatch {
        key,
        required,
        provided,
    };
    let missing = |key| FeatureMismatch {
        key,
        required: true,
        provided: false,
    };
    let expected = CapabilitiesMismatch {
        instructions: vec![
            InstructionMismatch::OtherSet,
            InstructionMismatch::Missing {
                mnemonic: "x_if".into(),
            },
        ],
        resources: vec![
            short(Resource::Blocks(carbon()), 3, 2),
            short(Resource::Blocks(BlockType::qubit()), 9, 8),
            short(Resource::Shots, 10, 5),
            short(Resource::InstructionsPerBatch, 4, 1),
        ],
        features: vec![
            missing(Feature::ReadoutParity),
            missing(Feature::Replay),
            missing(Feature::SeededRuns),
        ],
    };
    assert_eq!(mismatch, expected);
}

/// The instructions fit alone do not make capabilities fit: a short
/// resource is a mismatch with no instruction listed.
#[test]
fn capabilities_fit_only_when_every_kind_fits() {
    let required = Capabilities::new(
        CoinGates::set(),
        Resources::unconstrained().with_qubits(9),
        Features::unconstrained(),
    );
    let provided = Capabilities::new(
        CoinGates::set(),
        Resources::unconstrained().with_qubits(8),
        Features::unconstrained(),
    );
    let mismatch = required.fits(&provided).expect_err("9 qubits on 8");
    assert!(mismatch.instructions.is_empty(), "{mismatch:?}");
    assert_eq!(mismatch.resources.len(), 1, "{mismatch:?}");
    assert!(
        Capabilities::new(
            CoinGates::set(),
            Resources::unconstrained(),
            Features::unconstrained()
        )
        .fits(&provided)
        .is_ok(),
        "nothing stated fits"
    );
}

/// A mismatch's message names what is at fault, for a user who reads which
/// requirement to lower, or which part to change: each of its mismatches,
/// a short amount with both amounts.
#[test]
fn a_mismatch_names_what_is_at_fault() {
    let (required, provided) = every_kind_short();
    let message = required
        .fits(&provided)
        .expect_err("nothing fits")
        .to_string();
    for named in [
        &["x_if"][..],
        &["carbon", "3", "2"],
        &["qubit", "9", "8"],
        &["shots", "10", "5"],
        &["instructions per batch", "4", "1"],
        &["readout parit"],
        &["replay"],
        &["seed"],
    ] {
        let piece = message
            .split("; ")
            .find(|piece| named.iter().all(|word| piece.contains(word)));
        assert!(piece.is_some(), "one mismatch names {named:?}: {message}");
    }
}
