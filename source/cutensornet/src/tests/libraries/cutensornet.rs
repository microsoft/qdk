// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of cuTensorNet: the functions it looks up, and the
//! one version it accepts, built against the audited CUDA Runtime.

use super::function_table::assert_each_field_is_named_for_its_symbol;
use crate::tests::stand_ins::{StandInResolver, cutensornet_version};
use crate::{
    LibraryFault,
    libraries::function_table::{FunctionTable, SymbolResolver},
    libraries::{
        NativeLibrary,
        cutensornet::{CuTensorNet, CuTensorNetFunctions, CuTensorNetVersionFunctions},
    },
};

/// Each cuTensorNet table looks each field up by the function it is named
/// for: its two version functions, of one signature, can be neither
/// swapped nor repeated.
#[test]
fn the_cutensornet_tables_look_each_field_up_by_its_symbol() {
    for rows in [
        CuTensorNetFunctions::ROWS,
        CuTensorNetVersionFunctions::ROWS,
    ] {
        assert_each_field_is_named_for_its_symbol(rows, "cutensornet");
    }
}

/// cuTensorNet's table holds only the functions the crate calls, and its
/// optional ones only add detail (`cutensornetGetLastError` words native
/// errors): a cuTensorNet exporting just the required ones, without the rest
/// of its API (for instance its state API, which the crate does not use),
/// is used.
#[test]
fn only_the_required_functions_must_be_exported() {
    let required = CuTensorNetFunctions::ROWS
        .iter()
        .filter(|row| row.2)
        .map(|row| row.1)
        .collect();
    let functions = CuTensorNetFunctions::resolve(&ExportsOnly(required))
        .expect("every required function is exported");
    assert!(functions.get_last_error.is_none());
}

/// A cuTensorNet that exports only the given functions, each as
/// `StandInResolver::audited` resolves it.
struct ExportsOnly(Vec<&'static str>);

impl SymbolResolver for ExportsOnly {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        if !self.0.contains(&symbol) {
            return None;
        }
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { StandInResolver::audited().resolve(symbol) }
    }
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
