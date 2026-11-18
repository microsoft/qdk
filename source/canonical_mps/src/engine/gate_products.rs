// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The arithmetic of the gates the chain keeps pending on the host: a
//! one-site gate waiting on its site, and the operator of the pending
//! window ([`fusion`](super::fusion)), the product of the gates on its
//! sites.
//!
//! The gates are row-major, the first site the most significant bit: a one-site
//! gate `G[s, p]` at `2s + p`, a gate on `k` sites at `row·2ᵏ + column`, the
//! first site's value the highest bit of the row and of the column, as
//! [`GateSplitCall`](crate::site_algebra::GateSplitCall) reads a two-site gate.

use num_complex::Complex64;

/// The one-site identity.
pub(super) const IDENTITY: [Complex64; 4] = [
    Complex64::ONE,
    Complex64::ZERO,
    Complex64::ZERO,
    Complex64::ONE,
];

/// `a·b` of two one-site gates.
pub(super) fn product_on_one_site(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 4] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 2, at % 2);
        (0..2).map(|k| a[2 * row + k] * b[2 * k + col]).sum()
    })
}

/// `a·b` of two operators on the same sites.
pub(super) fn product_on_same_sites(a: &[Complex64], b: &[Complex64]) -> Vec<Complex64> {
    let dim = dimension(a.len());
    debug_assert_eq!(a.len(), b.len(), "two operators on the same sites");
    let mut product = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for k in 0..dim {
            let x = a[row * dim + k];
            if x == Complex64::ZERO {
                continue;
            }
            for col in 0..dim {
                product[row * dim + col] += x * b[k * dim + col];
            }
        }
    }
    product
}

/// `operator`, on the sites `on` in that order, as an operator on the
/// sites `into`, the identity on those of `into` not in `on`. The sites of
/// `on` are distinct and all in `into`.
pub(super) fn widen(operator: &[Complex64], on: &[usize], into: &[usize]) -> Vec<Complex64> {
    let k = into.len();
    let dim = 1 << k;
    let small = 1 << on.len();
    debug_assert_eq!(operator.len(), small * small, "an operator on `on`");
    // The bit of `into`'s index that holds each site of `on`.
    let bits: Vec<usize> = on
        .iter()
        .map(|site| {
            let at = into
                .iter()
                .position(|s| s == site)
                .expect("a site of `on` in `into`");
            k - 1 - at
        })
        .collect();
    let mask = bits.iter().fold(0, |mask, bit| mask | (1 << bit));
    let index_on = |index: usize| {
        bits.iter()
            .fold(0, |value, bit| (value << 1) | ((index >> bit) & 1))
    };
    let mut widened = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for col in 0..dim {
            if row & !mask == col & !mask {
                widened[row * dim + col] = operator[index_on(row) * small + index_on(col)];
            }
        }
    }
    widened
}

/// The side of a square operator of `len` elements.
fn dimension(len: usize) -> usize {
    let dim = len.isqrt();
    debug_assert_eq!(dim * dim, len, "a square operator");
    dim
}
