// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The products of the gates the chain keeps pending on the host. They let
//! a run of gates reach the backend as one operator, rather than one call
//! per gate ([`chain`](super::chain) says when each is applied):
//!
//! - a one-site gate waiting on its site, times each later gate there;
//! - the operator of the pending window ([`fusion`](super::fusion)), the
//!   product of the gates on its sites, each first widened to all of them.
//!
//! The gates are row-major (stored row after row), the first site the most
//! significant bit. A one-site gate `G[s, p]` is at `2s + p`. A gate on `k`
//! sites is at `row·2ᵏ + column`, the first site's value the highest bit
//! of the row and of the column. That is how
//! [`GateSplitCall`](crate::site_algebra::GateSplitCall) reads a two-site
//! gate.

use num_complex::Complex64;

/// The one-site identity.
pub(super) const ONE_SITE_IDENTITY: [Complex64; 4] = [
    Complex64::ONE,
    Complex64::ZERO,
    Complex64::ZERO,
    Complex64::ONE,
];

/// The gate that applies `earlier`, then `later`, on one site: the
/// matrix product `later·earlier`.
pub(super) fn product_on_one_site(
    later: &[Complex64; 4],
    earlier: &[Complex64; 4],
) -> [Complex64; 4] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 2, at % 2);
        (0..2)
            .map(|k| later[2 * row + k] * earlier[2 * k + col])
            .sum()
    })
}

/// The operator that applies `earlier`, then `later`, on the same sites:
/// the matrix product `later·earlier`.
pub(super) fn product_on_same_sites(later: &[Complex64], earlier: &[Complex64]) -> Vec<Complex64> {
    let dim = dimension(later.len());
    debug_assert_eq!(
        later.len(),
        earlier.len(),
        "two operators on the same sites"
    );
    let mut product = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for k in 0..dim {
            let later_entry = later[row * dim + k];
            if later_entry == Complex64::ZERO {
                continue;
            }
            for col in 0..dim {
                product[row * dim + col] += later_entry * earlier[k * dim + col];
            }
        }
    }
    product
}

/// `operator`, on the sites `on` in that order, rewritten to act on the
/// sites `into`: the identity on the others, the bits reordered to
/// `into`'s order. The sites of `on` are distinct and all in `into`.
///
/// Multiplying two gates as matrices needs both on the same sites, in the
/// same order. A gate arrives on its own sites, in its caller's order (a
/// CNOT's control first), while the window holds more sites, ascending:
///
/// ```text
///  window on [0, 1, 2], an 8 × 8 operator; a CNOT on (2, 0) arrives:
///  CNOT, 4 × 4 on (2, 0)  ── widen ──▶  8 × 8 on [0, 1, 2]
///  window's operator  ←  widened CNOT · window's operator
/// ```
///
/// A row or a column of the result gives each site of `into` a value, one
/// bit per site. An entry is zero where its row and column differ on a
/// site outside `on`, since the identity leaves those sites as they are.
/// Otherwise it is the entry of `operator` at the bits its row and column
/// give the sites of `on`, read in `on`'s order.
///
/// ```text
///  CNOT on [2, 0] (site 2 the control) widened into [0, 1, 2]:
///  index over into:  bit 2 = site 0, bit 1 = site 1, bit 0 = site 2
///  sites of on:      site 2 at bit 0, site 0 at bit 2
///  row 0b101, col 0b001:  bit 1 agrees (site 1 left as it is);
///    read in on's order: row (1, 1) = 3, col (1, 0) = 2
///    entry = CNOT[3, 2] = 1  (control 1 flips the target 0 to 1)
/// ```
pub(super) fn widen(operator: &[Complex64], on: &[usize], into: &[usize]) -> Vec<Complex64> {
    let dim = 1 << into.len();
    let small = 1 << on.len();
    debug_assert_eq!(operator.len(), small * small, "an operator on `on`");
    let on_bits = SiteBits::of(on, into);
    let mut widened = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for col in 0..dim {
            if on_bits.cleared(row) == on_bits.cleared(col) {
                widened[row * dim + col] =
                    operator[on_bits.index_over_on(row) * small + on_bits.index_over_on(col)];
            }
        }
    }
    widened
}

/// The bits of an index over the sites `into` that hold the sites `on`,
/// in `on`'s order. The first site of `into` is the highest bit.
struct SiteBits {
    /// The bit of each site of `on`.
    bits: Vec<usize>,
    /// Those bits set, the others clear.
    mask: usize,
}

impl SiteBits {
    /// The bits of `on`'s sites in an index over `into`.
    fn of(on: &[usize], into: &[usize]) -> Self {
        let site_count = into.len();
        let bits: Vec<usize> = on
            .iter()
            .map(|site| {
                let at = into
                    .iter()
                    .position(|s| s == site)
                    .expect("a site of `on` in `into`");
                site_count - 1 - at
            })
            .collect();
        let mask = bits.iter().fold(0, |mask, bit| mask | (1 << bit));
        Self { bits, mask }
    }

    /// The values `index` gives the sites of `on`, as an index over `on`.
    #[inline]
    fn index_over_on(&self, index: usize) -> usize {
        self.bits
            .iter()
            .fold(0, |value, bit| (value << 1) | ((index >> bit) & 1))
    }

    /// `index` with the bits of `on`'s sites cleared: the values of the
    /// other sites.
    #[inline]
    fn cleared(&self, index: usize) -> usize {
        index & !self.mask
    }
}

/// The side of a square operator of `len` elements.
fn dimension(len: usize) -> usize {
    let dim = len.isqrt();
    debug_assert_eq!(dim * dim, len, "a square operator");
    dim
}
