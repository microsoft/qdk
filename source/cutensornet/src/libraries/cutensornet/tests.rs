// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of cuTensorNet: the functions it looks up, and the
//! one version it accepts, built against the audited CUDA Runtime.

use crate::test_utils::stand_ins::{StandInResolver, cutensornet_version};
use crate::{
    LibraryFault,
    libraries::function_table::{FunctionTable, SymbolResolver},
    libraries::{
        NativeLibrary,
        cutensornet::{CuTensorNet, CuTensorNetFunctions},
    },
};

/// cuTensorNet's table holds only the functions the crate calls. Its
/// optional ones only add detail (`cutensornetGetLastError` words native
/// errors). So a cuTensorNet exporting just the required ones is used,
/// without the rest of its API (for instance its state API, which the crate
/// does not use).
#[test]
fn only_the_required_functions_must_be_exported() {
    let required = CuTensorNetFunctions::ROWS
        .iter()
        .filter(|row| row.1)
        .map(|row| row.0)
        .collect();
    let functions = CuTensorNetFunctions::resolve(&ExportsOnly(required))
        .expect("every required function is exported");
    assert!(functions.cutensornetGetLastError.is_none());
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

/// cuTensorNet 2.13.0 is the audited release, and is accepted. 2.13.1 and
/// 2.12.0 are not.
#[test]
fn only_cutensornet_2_13_0_is_accepted() {
    for (found, version) in [
        (21_300, cutensornet_version::<21_300> as *const ()),
        (21_301, cutensornet_version::<21_301> as *const ()),
        (21_200, cutensornet_version::<21_200> as *const ()),
    ] {
        let resolver = StandInResolver::audited().with("cutensornetGetVersion", version);
        let expected = if found == 21_300 {
            vec![]
        } else {
            vec![LibraryFault::UnsupportedVersion {
                component: "cuTensorNet",
                found,
                supported: 21_300,
            }]
        };
        assert_eq!(version_faults(&resolver), expected);
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
