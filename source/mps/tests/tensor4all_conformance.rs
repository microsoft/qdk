// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![cfg(feature = "tensor4all-cpu")]

mod common;

use qdk_mps::tensor4all::factory;

#[test]
fn gate_parity() {
    common::gate_parity(&factory());
}

#[test]
fn state_updates() {
    common::state_updates(&factory());
}

#[test]
fn truncation_regression() {
    common::truncation_regression(&factory());
}

#[test]
fn truncation_policy() {
    common::truncation_policy(&factory());
}

#[test]
fn lifecycle() {
    common::lifecycle(&factory());
}

#[test]
fn measurement() {
    common::measurement(&factory());
}

#[test]
fn observables() {
    common::observables(&factory());
}

#[test]
fn capabilities() {
    common::capabilities(&factory());
}

#[test]
fn resource_policy_rejection() {
    let before = std::env::var_os("RAYON_NUM_THREADS");
    common::resource_policy_rejection(&factory());
    assert_eq!(std::env::var_os("RAYON_NUM_THREADS"), before);
}

#[test]
fn report() {
    common::report(&factory());
}
