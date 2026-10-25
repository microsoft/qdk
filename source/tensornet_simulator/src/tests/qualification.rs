// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The A100 qualification: the cases of `cases` on cuTensorNet, against
//! hand-computed values, the exact reference, and M0's pinned B1–B4
//! values. Every test is `#[ignore]`d; `qualify-simulator-on-a100.sh`
//! runs them on the pinned VM with
//! `cargo test -p tensornet_simulator --release -- --ignored --nocapture
//! --test-threads=1 tests::qualification`.
//!
//! The engine is `Availability::mps_engine`, whose policy is M0's base
//! qualification policy (absolute cutoff 1e-10). M0 ran B3 and B4 with an
//! absolute cutoff of 1e-12, which the engine does not expose; their
//! values and tolerances are M0's, and a miss is to be read against that
//! difference first (see the crate README).

use qdk_cutensornet::CuTensorNetEngine;
use tensornet::reference::ExactReference;

use super::cases::{
    bell_samples_are_correlated, expectations_agree, expectations_are_the_hand_computed_ones,
    per_shot_bell_pairs, per_shot_feedback_and_resets, trotter_matches_m0, x_pattern_reads_back,
};
use crate::MpsStack;

/// cuTensorNet on device 0 with χ = `max_bond`, after printing what was
/// discovered.
fn stack(max_bond: u32) -> MpsStack<CuTensorNetEngine> {
    let availability = qdk_cutensornet::discover().expect("the native libraries are available");
    let report = availability.report();
    println!(
        "cutensornet_library={}",
        report.cutensornet_library.display()
    );
    println!("cudart_library={}", report.cuda_runtime_library.display());
    println!("cutensornet_version={}", report.cutensornet_version);
    println!(
        "cutensornet_cuda_runtime_version={}",
        report.cutensornet_cuda_runtime_version
    );
    println!("cudart_version={}", report.cuda_runtime_version);
    println!("cuda_driver_version={}", report.cuda_driver_version);
    let engine = availability.mps_engine().expect("an engine on device 0");
    MpsStack::new(engine).with_max_bond_dimension(Some(max_bond))
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn bell_on_the_a100() {
    let stack = stack(128);
    bell_samples_are_correlated(&stack, 1000);
    expectations_are_the_hand_computed_ones(&stack, 1e-10);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b0_expectations_agree_with_the_reference() {
    // χ = 128 exceeds 2^(4/2), so both chains are exact up to round-off.
    expectations_agree(
        &stack(128),
        &MpsStack::new(ExactReference::new()),
        0..16,
        1e-10,
    );
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b1_widths() {
    let stack = stack(128);
    for width in [2, 3, 63, 64, 128] {
        x_pattern_reads_back(&stack, width);
        println!("b1 width={width} ok");
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b2_trotter() {
    // M0's B2: three widths pin extensivity.
    let stack = stack(128);
    for (width, expected) in [
        (12, 4.332_869_154_633),
        (16, 6.347_012_657_087),
        (20, 8.361_156_159_877),
    ] {
        trotter_matches_m0(&stack, width, 8, expected, 1e-9);
        // The reference, where it fits (up to 20 sites).
        trotter_matches_m0(
            &MpsStack::new(ExactReference::new()),
            width,
            8,
            expected,
            1e-9,
        );
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b3_matched_bond() {
    trotter_matches_m0(&stack(128), 128, 16, 60.319_518_034_172_646, 1e-10);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b4_convergence() {
    for (max_bond, expected) in [
        (32, 122.350_509_319_616),
        (64, 122.350_509_321_997),
        (128, 122.350_509_322_001),
        (256, 122.350_509_322_001),
    ] {
        trotter_matches_m0(&stack(max_bond), 256, 16, expected, 1e-9);
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn per_shot_bell_pairs_on_the_a100() {
    per_shot_bell_pairs(&stack(128), 200);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn per_shot_feedback_and_resets_on_the_a100() {
    // M0's B5 checked a branch's capture and continuation; here a shot's
    // state continues after its measurements, resets and feedback.
    per_shot_feedback_and_resets(&stack(4), 40);
}
