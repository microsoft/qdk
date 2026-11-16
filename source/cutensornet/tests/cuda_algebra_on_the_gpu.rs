// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra and the CUDA MPS engine on a GPU, against the
//! reference algebra, the exact reference and the engine on the CPU: the
//! rules the unit tests check on the fake libraries, on larger shapes and
//! the real libraries, and what only the real libraries decide.
//!
//! It needs a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13, so
//! it is ignored in CI; run it with `--ignored` on such a host. The
//! algebra opens on the real libraries only on Linux on x86-64, the
//! platform they were audited on.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

#[path = "../src/tests/cuda_algebra/algebra_rules.rs"]
mod algebra_rules;
#[path = "../src/tests/cuda_algebra/engine_circuits.rs"]
mod engine_circuits;

use algebra_rules::{Split, random_gate, random_pair};
use canonical_mps::algebra::CentreSide;
use num_complex::Complex64;
use qdk_cutensornet::{CudaAlgebra, discover};

/// Pairs from one value per side to 64 × 64, a bond of one between wide
/// sites among them (whose gated pair has at most four nonzero singular
/// values of eight).
const SHAPES: [(usize, usize, usize); 6] = [
    (1, 1, 1),
    (2, 2, 4),
    (4, 1, 4),
    (8, 8, 8),
    (16, 12, 8),
    (32, 32, 32),
];

/// Products agree with the reference algebra's within this, the libraries'
/// rounding on the largest shapes with room to spare.
const TOLERANCE: f64 = 1e-10;

fn algebra() -> CudaAlgebra {
    let availability = discover().expect("the audited libraries are installed");
    CudaAlgebra::new(&availability).expect("an algebra opens on device 0")
}

/// Splits give the reference algebra's truncated SVD (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_split_gives_the_reference_algebras_truncated_svd() {
    algebra_rules::splits_give_the_reference_algebras_truncated_svd(&algebra(), &SHAPES, TOLERANCE);
}

/// The site away from the centre is an isometry (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_split_leaves_an_isometry_away_from_the_centre() {
    algebra_rules::a_split_leaves_an_isometry_away_from_the_centre(&algebra(), &SHAPES, TOLERANCE);
}

/// The bond kept lies between 1 and its bounds (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_bond_of_a_split_lies_between_one_and_its_bounds() {
    algebra_rules::the_bond_of_a_split_lies_between_one_and_its_bounds(&algebra(), &SHAPES);
}

/// A split returns the bond and share cuTensorNet kept
/// (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_split_returns_the_bond_and_share_it_kept() {
    algebra_rules::a_split_returns_the_bond_and_share_it_kept(&algebra());
}

/// A gated pair of a bond of one between sites of four values has at most
/// four nonzero singular values of eight: under a cutoff of 0, the share
/// kept is exactly 1, whichever of the values at rounding cuTensorNet
/// keeps.
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_cutoff_of_zero_keeps_the_retained_fraction_at_one() {
    let (a, b) = random_pair(4, 1, 4, 30);
    let split = Split {
        a: &a,
        b: &b,
        l: 4,
        m: 1,
        r: 4,
        gate: random_gate(31),
        cutoff: 0.0,
        max_extent: usize::MAX,
        centre: CentreSide::Left,
    };
    let outcome = split.on(&algebra()).expect("the split succeeds");
    assert_eq!(outcome.retained_fraction.to_bits(), 1.0_f64.to_bits());
}

/// A Bell pair's two singular values are equal, so a cutoff of one half
/// is a tie: cuTensorNet may keep one value or both, and either way the
/// share kept is the bond's.
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_tie_at_the_cutoff_meets_the_contract_either_way() {
    let zero = [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
    let half = std::f64::consts::FRAC_1_SQRT_2;
    // Takes |00⟩ to (|00⟩ + |11⟩)/√2: the first column, rows 0 and 3.
    let mut gate = [Complex64::new(0.0, 0.0); 16];
    gate[0] = Complex64::new(half, 0.0);
    gate[12] = Complex64::new(half, 0.0);
    let split = Split {
        a: &zero,
        b: &zero,
        l: 1,
        m: 1,
        r: 1,
        gate,
        cutoff: 0.5,
        max_extent: usize::MAX,
        centre: CentreSide::Right,
    };
    let outcome = split.on(&algebra()).expect("the split succeeds");
    assert!((1..=2).contains(&outcome.bond), "{}", outcome.bond);
    #[expect(clippy::cast_precision_loss, reason = "a bond of 1 or 2")]
    let share = outcome.bond as f64 / 2.0;
    let kept = outcome.retained_fraction;
    assert!(
        (kept - share).abs() < 1e-12,
        "bond {}: {kept}",
        outcome.bond
    );
}

/// Moves give the reference algebra's product and bond
/// (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_move_gives_the_reference_algebras_product_and_bond() {
    let shapes = [(1, 4, 2), (2, 5, 1), (8, 3, 16), (32, 32, 32)];
    let algebra = algebra();
    algebra_rules::moves_give_the_reference_algebras_product_and_bond(&algebra, &SHAPES, TOLERANCE);
    algebra_rules::moves_give_the_reference_algebras_product_and_bond(&algebra, &shapes, TOLERANCE);
}

/// The site the centre leaves is an isometry (`algebra_rules.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_move_leaves_an_isometry_behind() {
    algebra_rules::a_move_leaves_an_isometry_behind(&algebra(), &SHAPES, TOLERANCE);
}

/// The engine on the GPU runs a long circuit as the exact reference does,
/// without truncation (`engine_circuits.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_engine_on_the_gpu_agrees_with_the_reference() {
    engine_circuits::a_long_circuit_agrees_with_the_reference_without_truncation(algebra());
}

/// The engine on the GPU, truncating, stays within the distance its
/// splits allow of the exact reference (`engine_circuits.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_engine_on_the_gpu_truncates_within_its_bound() {
    engine_circuits::a_truncated_circuit_stays_within_the_distance_its_splits_allow(algebra());
}

/// The engine on the GPU, capped, reads as the engine on the CPU
/// (`engine_circuits.rs`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_engine_on_the_gpu_reads_as_on_the_cpu_when_capped() {
    engine_circuits::a_capped_circuit_reads_as_on_the_cpu_engine(algebra());
}
