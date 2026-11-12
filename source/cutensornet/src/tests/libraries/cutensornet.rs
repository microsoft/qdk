// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of cuTensorNet: the functions it looks up, and the
//! one version it accepts, built against the audited CUDA Runtime.

use crate::tests::stand_ins::{StandInResolver, cutensornet_version};
use crate::{
    LibraryFault,
    libraries::loader::FunctionTable,
    libraries::{
        NativeLibrary,
        cutensornet::{CuTensorNet, CuTensorNetFunctions},
    },
};

/// An installation whose cuTensorNet lacks required functions is refused,
/// naming each one.
#[test]
fn every_missing_required_cutensornet_function_is_reported() {
    let required: Vec<_> = CuTensorNetFunctions::ROWS
        .iter()
        .filter(|row| row.2)
        .map(|row| row.1)
        .collect();
    let resolver = required
        .iter()
        .fold(StandInResolver::audited(), |resolver, &symbol| {
            resolver.without(symbol)
        });
    let missing = CuTensorNetFunctions::resolve(&resolver)
        .err()
        .expect("every required function is missing");
    let expected: Vec<_> = required
        .into_iter()
        .map(|symbol| LibraryFault::MissingRequiredSymbol { symbol })
        .collect();
    assert_eq!(missing, expected);
}

/// `cutensornetGetLastError` only adds detail to native error messages, so a
/// library without it is still used.
#[test]
fn a_missing_optional_function_is_absent_from_the_table() {
    let functions = CuTensorNetFunctions::resolve(
        &StandInResolver::audited().without("cutensornetGetLastError"),
    )
    .expect("an optional function is not required");
    assert!(functions.get_last_error.is_none());
}

/// The faults found in the versions of a cuTensorNet whose version
/// functions are `resolver`'s.
fn version_faults(resolver: &StandInResolver) -> Vec<LibraryFault> {
    CuTensorNet::probe_versions(resolver).faults
}

/// The audited cuTensorNet is accepted.
#[test]
fn the_audited_cutensornet_is_accepted() {
    assert_eq!(version_faults(&StandInResolver::audited()), []);
}

/// cuTensorNet 2.13.0 is the audited release; 2.13.1 and 2.12.0 are not.
#[test]
fn only_cutensornet_2_13_0_is_accepted() {
    for (found, version) in [
        (21_301, cutensornet_version::<21_301> as *const ()),
        (21_200, cutensornet_version::<21_200> as *const ()),
    ] {
        let resolver = StandInResolver::audited().with("cutensornetGetVersion", version);
        assert_eq!(
            version_faults(&resolver),
            [LibraryFault::UnsupportedVersion {
                component: "cuTensorNet",
                found,
                supported: 21_300,
            }]
        );
    }
}

/// A cuTensorNet 2.13.0 built for the CUDA Runtime 12.8 is refused, even
/// with the CUDA Runtime 12.9 loaded.
#[test]
fn cutensornet_must_be_built_against_cuda_runtime_12_9() {
    let resolver = StandInResolver::audited().with(
        "cutensornetGetCudartVersion",
        cutensornet_version::<12_080> as *const (),
    );
    assert_eq!(
        version_faults(&resolver),
        [LibraryFault::UnsupportedVersion {
            component: "cuTensorNet's CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        }]
    );
}
