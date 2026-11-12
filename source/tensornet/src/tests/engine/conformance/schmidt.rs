// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests that call the kit's `truncated` (nine) and `normalized` (the
//! last) directly, an internal seam: they pin the Schmidt truncation the
//! kit's reference applies, and its division by the kept weight, at their
//! exact edges (which pairs a bond keeps, ties, subnormal couplings), which
//! `check` cannot reach without contrived engines. The
//! `truncated_reads_catches_*` and `truncated_chain_catches_*` tests, in
//! `capacity.rs`, cover the public path.

use super::REFERENCE;
use crate::conformance::{cnot, controlled_y, kron, normalized, product, ry, s_h, truncated};
use crate::reference::ReferenceState;
use crate::tests::{H, c, hadamard};
use crate::{MpsEngine, MpsShape};
use num_complex::Complex64;

fn two_sites(amplitudes: [Complex64; 4]) -> ReferenceState {
    ReferenceState::from_amplitudes(&amplitudes)
}

fn assert_close(actual: &ReferenceState, expected: &ReferenceState) {
    for i in 0..4 {
        let (a, e) = (actual.amplitude(i), expected.amplitude(i));
        assert!((a - e).norm() < 1e-12, "amplitude {i}: {a}, not {e}");
    }
}

/// 0.8|00⟩ + 0.6|11⟩, whose Schmidt pairs are the basis states.
#[test]
fn a_bond_of_one_keeps_the_larger_schmidt_pair() {
    let o = c(0.0, 0.0);
    let state = two_sites([c(0.8, 0.0), o, o, c(0.6, 0.0)]);
    assert_close(
        &truncated(&state, 0, 1).expect("no tie"),
        &two_sites([c(0.8, 0.0), o, o, o]),
    );
}

#[test]
fn a_bond_of_one_keeps_the_larger_pair_when_it_is_the_second() {
    let o = c(0.0, 0.0);
    let state = two_sites([c(0.6, 0.0), o, o, c(0.8, 0.0)]);
    assert_close(
        &truncated(&state, 0, 1).expect("no tie"),
        &two_sites([o, o, o, c(0.8, 0.0)]),
    );
}

/// The same state turned by local gates (H and S·H on site 0, H on site 1)
/// keeps the same pair turned by them: the cut is basis-free.
#[test]
fn a_bond_of_one_keeps_the_larger_pair_in_any_local_basis() {
    let o = c(0.0, 0.0);
    let state = turn(two_sites([c(0.8, 0.0), o, o, c(0.6, 0.0)]));
    let expected = turn(two_sites([c(0.8, 0.0), o, o, o]));
    assert_close(&truncated(&state, 0, 1).expect("no tie"), &expected);
}

/// The same state turned by local gates with phases neither real nor
/// imaginary keeps the same pair turned by them: the reduced density
/// matrices the truncation diagonalizes have generic complex entries.
#[test]
fn a_bond_of_one_keeps_the_larger_pair_of_complex_schmidt_vectors() {
    let o = c(0.0, 0.0);
    let (a, b) = (
        Complex64::from_polar(H, 0.7),
        Complex64::from_polar(H, -0.4),
    );
    let gates = [
        (0, [c(H, 0.0), c(H, 0.0), a, -a]),
        (1, [c(H, 0.0), b, c(H, 0.0), -b]),
    ];
    let turned = |mut state: ReferenceState| {
        for (site, gate) in &gates {
            REFERENCE
                .apply(&mut state, &[*site], gate)
                .expect("a unitary on a site of the state");
        }
        state
    };
    let state = turned(two_sites([c(0.8, 0.0), o, o, c(0.6, 0.0)]));
    let expected = turned(two_sites([c(0.8, 0.0), o, o, o]));
    assert_close(&truncated(&state, 0, 1).expect("no tie"), &expected);
}

/// Turns sites 0 and 1 by S·H and H, and site 3, when the state has it,
/// by Ry(0.3).
fn turn(mut state: ReferenceState) -> ReferenceState {
    let s_h = [c(H, 0.0), c(H, 0.0), c(0.0, H), c(0.0, -H)];
    let (sin, cos) = 0.15f64.sin_cos();
    let ry = [c(cos, 0.0), c(-sin, 0.0), c(sin, 0.0), c(cos, 0.0)];
    let mut gates = vec![(0, s_h), (1, hadamard())];
    if state.site_count() > 3 {
        gates.push((3, ry));
    }
    for (site, gate) in gates {
        REFERENCE
            .apply(&mut state, &[site], &gate)
            .expect("a unitary on a site of the state");
    }
    state
}

/// Four sites whose amplitudes are `amplitudes` on the basis states with
/// sites 1 and 2 equal to sites 0 and 3 (`|abba⟩`, site 0 first) and zero
/// elsewhere: at cut 1, the middle one, its Schmidt pairs are the basis
/// states of sites 0 and 1 with those of sites 2 and 3 mirrored, and its
/// Schmidt values are the amplitudes.
fn schmidt_form(amplitudes: [f64; 4]) -> ReferenceState {
    let mut all = vec![c(0.0, 0.0); 16];
    for (k, &amplitude) in amplitudes.iter().enumerate() {
        let (a, b) = (k & 1, k >> 1);
        // Site i is bit i of the index.
        all[a | b << 1 | b << 2 | a << 3] = c(amplitude, 0.0);
    }
    ReferenceState::from_amplitudes(&all)
}

/// A bond of two at the middle cut of four sites keeps the two largest
/// pairs, whatever local gates turn them.
#[test]
fn a_bond_of_two_keeps_the_two_largest_pairs_at_an_interior_cut() {
    let state = turn(schmidt_form([0.2, 0.8, 0.5, 0.26]));
    let expected = turn(schmidt_form([0.0, 0.8, 0.5, 0.0]));
    assert_close_all(&truncated(&state, 1, 2).expect("no tie"), &expected);
}

/// At cut 2 of four sites the right side, site 3, is the smaller: the
/// truncation keeps its larger pair.
#[test]
fn a_bond_of_one_keeps_the_larger_pair_where_the_right_side_is_smaller() {
    let mut all = vec![c(0.0, 0.0); 16];
    all[0] = c(0.6, 0.0);
    all[0b1111] = c(0.8, 0.0);
    let state = turn(ReferenceState::from_amplitudes(&all));
    let mut kept = vec![c(0.0, 0.0); 16];
    kept[0b1111] = c(0.8, 0.0);
    let expected = turn(ReferenceState::from_amplitudes(&kept));
    assert_close_all(&truncated(&state, 2, 1).expect("no tie"), &expected);
}

/// Two equal Schmidt values: two engines may keep either, so the kit has
/// no state to expect.
#[test]
fn a_truncation_between_equal_schmidt_values_is_an_error() {
    let o = c(0.0, 0.0);
    let state = two_sites([c(H, 0.0), o, o, c(H, 0.0)]);
    assert!(truncated(&state, 0, 1).is_err());
}

/// The kit's five sites under a bond of two (its "truncated chain" case):
/// after six gates on neighbours, three of which truncate from rank four,
/// the weight kept is the one a dense singular value decomposition keeps,
/// truncating to the two largest values at each gate's cut:
/// 0.910911777288767 by `numpy.linalg.svd`. The reduced density matrices of
/// that chain leave off-diagonal entries of subnormal size during the
/// diagonalization; a rotation built from one of them was not unitary,
/// and the kit expected 0.9109117580678607.
#[test]
fn repeated_truncations_keep_the_weight_a_dense_decomposition_keeps() {
    let g = product(
        &controlled_y(),
        &product(&kron(&ry(0.8), &s_h()), &cnot(), 4),
        4,
    );
    let g2 = product(
        &cnot(),
        &product(&kron(&s_h(), &ry(-1.1)), &controlled_y(), 4),
        4,
    );
    let capacity = MpsShape::qubit_chain(5, 4).expect("a chain of five qubits");
    let mut state = REFERENCE.prepare(&capacity).expect("five qubits");
    for (site, matrix) in [
        (0, ry(0.9)),
        (1, s_h()),
        (2, ry(1.3)),
        (3, s_h()),
        (4, ry(-0.6)),
    ] {
        REFERENCE
            .apply(&mut state, &[site], &matrix)
            .expect("a unitary on a site of the state");
    }
    for (sites, matrix) in [
        ([0, 1], &g),
        ([2, 3], &g2),
        ([1, 2], &g),
        ([3, 4], &g),
        ([2, 3], &g2),
        ([1, 2], &g),
    ] {
        REFERENCE
            .apply(&mut state, &sites, matrix)
            .expect("a unitary on neighbours of the state");
        let cut = sites[0].min(sites[1]);
        let rank = REFERENCE
            .shape(&state)
            .expect("a chain of qubits")
            .bond_dim(cut)
            .expect("a cut of the chain");
        if rank > 2 {
            state = truncated(&state, cut, 2).expect("no tie, and a sound decomposition");
        }
    }
    let weight: f64 = (0..1 << 5).map(|i| state.amplitude(i).norm_sqr()).sum();
    assert!(
        (weight - 0.910_911_777_288_767).abs() < 1e-13,
        "the kit kept {weight}"
    );
}

/// 0.6|00⟩ + 0.8|11⟩ with a subnormal amplitude on |01⟩ (site 0 One, site
/// 1 Zero): site 0's reduced density matrix has a subnormal off-diagonal
/// entry, from which a rotation's phase has too few mantissa bits to be of
/// modulus 1. The truncation keeps the larger pair as if the entry were
/// zero.
#[test]
fn a_truncation_leaves_a_subnormal_coupling_unrotated() {
    let o = c(0.0, 0.0);
    let state = two_sites([c(0.6, 0.0), c(1e-321, 2e-321), o, c(0.8, 0.0)]);
    assert_close_all(
        &truncated(&state, 0, 1).expect("no tie, and a sound decomposition"),
        &two_sites([o, o, o, c(0.8, 0.0)]),
    );
}

fn assert_close_all(actual: &ReferenceState, expected: &ReferenceState) {
    for i in 0..1 << expected.site_count() {
        let (a, e) = (actual.amplitude(i), expected.amplitude(i));
        assert!((a - e).norm() < 1e-12, "amplitude {i}: {a}, not {e}");
    }
}

/// The kit's truncated state keeps the weight of the larger Schmidt
/// value, 0.64, which `normalized` divides out.
#[test]
fn normalized_divides_by_the_norm() {
    let o = c(0.0, 0.0);
    let kept = two_sites([c(0.8, 0.0), o, o, o]);
    assert_close(&normalized(&kept), &two_sites([c(1.0, 0.0), o, o, o]));
}
