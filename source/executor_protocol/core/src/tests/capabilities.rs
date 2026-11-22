// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires against what the part below provides, at one
//! boundary: every mismatch of each kind, at once. The one rule every kind
//! follows is pinned once, in `statements.rs`; each kind's file pins only
//! what that kind adds.

mod features;
mod resources;
mod statements;

/// The boundary a run's batches cross.
mod boundary {
    mod batch_limits;
}

use crate::{
    BlockType, BoundaryCompatible, Capabilities, CapabilitiesMismatch, Feature, FeatureMismatch,
    Features, Resource, ResourceMismatch, Resources,
};

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

/// A driver requiring 9 qubits, 3 carbon blocks, runs of 10 shots and
/// batches of 4, using every feature, over an executor taking 8 qubits and
/// 2 carbon blocks, runs of 5 and batches of 1, and no feature.
fn every_kind_short() -> (Capabilities, Capabilities) {
    let required = Capabilities::new(
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
    let provided = Capabilities::new(
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

/// Whoever builds the chain fixes every mismatch at once: each short
/// amount, the blocks by type, the shots, the instructions per batch, then
/// each feature not provided, readout parity, replay, seeded runs.
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

/// Capabilities fit only when both kinds fit: a resource short alone, or
/// a feature missing alone, is a mismatch listing it and nothing of the
/// other kind. A fit reading one kind alone fails the row of the other.
#[test]
fn capabilities_fit_only_when_every_kind_fits() {
    let required = Capabilities::new(
        Resources::unconstrained().with_qubits(9),
        Features::unconstrained().with_replay(true),
    );
    let short_qubits = ResourceMismatch {
        key: Resource::Blocks(BlockType::qubit()),
        required: 9,
        provided: 8,
    };
    let no_replay = FeatureMismatch {
        key: Feature::Replay,
        required: true,
        provided: false,
    };
    for (row, provided, expected) in [
        (
            "a resource short alone",
            Capabilities::new(
                Resources::unconstrained().with_qubits(8),
                Features::unconstrained(),
            ),
            CapabilitiesMismatch {
                resources: vec![short_qubits],
                ..CapabilitiesMismatch::default()
            },
        ),
        (
            "a feature missing alone",
            Capabilities::new(
                Resources::unconstrained(),
                Features::unconstrained().with_replay(false),
            ),
            CapabilitiesMismatch {
                features: vec![no_replay],
                ..CapabilitiesMismatch::default()
            },
        ),
    ] {
        assert_eq!(required.fits(&provided).map(drop), Err(expected), "{row}");
    }
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
        &["carbon", "3", "2"][..],
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
