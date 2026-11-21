// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires against what the part below provides, at one
//! boundary: every mismatch of each kind, at once.

mod features;
mod resources;

use crate::tests::fake_parts::{CoinGates, ParityGates};
use crate::{
    BoundaryCompatible, Capabilities, CapabilitiesMismatch, FeatureMismatch, Features,
    InstructionMismatch, ResourceMismatch, Resources,
};

/// A driver sending [`ParityGates`] on 9 qubits, with readout parities,
/// over an executor accepting [`CoinGates`] on 8, resolving none.
fn parity_driver_over_coin_bits() -> (Capabilities, Capabilities) {
    let required = Capabilities::new(ParityGates::set())
        .with_resources(Resources::new().with_qubits(9))
        .with_features(Features::new().with_readout_parity(true));
    let provided = Capabilities::new(CoinGates::set())
        .with_resources(Resources::new().with_qubits(8))
        .with_features(Features::new().with_readout_parity(false));
    (required, provided)
}

/// Whoever builds the chain fixes every mismatch at once: the instructions
/// that do not fit, the resources provided short and the features not
/// served are all listed, each as its kind's own fit lists them.
#[test]
fn every_mismatch_of_each_kind_is_listed_at_once() {
    let (required, provided) = parity_driver_over_coin_bits();
    let mismatch = required.fits(&provided).expect_err("nothing fits");
    let expected = CapabilitiesMismatch {
        instructions: vec![
            InstructionMismatch::OtherSet,
            InstructionMismatch::Missing {
                mnemonic: "x_if".into(),
            },
        ],
        resources: vec![ResourceMismatch::Blocks {
            block_type: crate::BlockType::qubit(),
            required: 9,
            provided: 8,
        }],
        features: vec![FeatureMismatch::ReadoutParity],
    };
    assert_eq!(mismatch, expected);
}

/// The instructions fit alone do not make capabilities fit: a short
/// resource is a mismatch with no instruction listed.
#[test]
fn capabilities_fit_only_when_every_kind_fits() {
    let required =
        Capabilities::new(CoinGates::set()).with_resources(Resources::new().with_qubits(9));
    let provided =
        Capabilities::new(CoinGates::set()).with_resources(Resources::new().with_qubits(8));
    let mismatch = required.fits(&provided).expect_err("9 qubits on 8");
    assert!(mismatch.instructions.is_empty(), "{mismatch:?}");
    assert_eq!(mismatch.resources.len(), 1, "{mismatch:?}");
    assert!(
        Capabilities::new(CoinGates::set()).fits(&provided).is_ok(),
        "nothing stated fits"
    );
}

/// A mismatch's message names every mismatch of every kind, for a user
/// who fixes them all at once.
#[test]
fn a_mismatch_names_each_of_its_mismatches() {
    let (required, provided) = parity_driver_over_coin_bits();
    let mismatch = required.fits(&provided).expect_err("nothing fits");
    let message = mismatch.to_string();
    for word in ["x_if", "qubit", "readout parit"] {
        assert!(message.contains(word), "names {word}: {message}");
    }
}
