// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reference algebra against the `SiteAlgebra` conformance kit, as a
//! backend's own tests run it.

use canonical_mps::CpuAlgebra;
use canonical_mps::algebra::conformance;

/// [`CpuAlgebra`] keeps every promise of the contract the kit checks.
#[test]
fn the_reference_algebra_conforms() {
    let failures = conformance::check(&CpuAlgebra);
    assert!(failures.is_empty(), "{failures:#?}");
}
