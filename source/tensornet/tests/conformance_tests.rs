// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reference conforms, checked as an engine's own tests check it:
//! outside the crate, through the public `conformance::check`.

#[test]
fn reference_conforms() {
    let engine = tensornet::reference::ExactReference::new();
    let failures = tensornet::conformance::check(&engine);
    assert!(failures.is_empty(), "{failures:#?}");
}
