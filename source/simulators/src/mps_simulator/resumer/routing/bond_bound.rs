// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): [`BondBound`], an
//! upper bound on every bond of a state's chain that the resumer keeps
//! from integers alone, so that the routing's guard knows where a batch
//! could need more than the max bond dimension before the batch runs.
//!
//! **What it promises.** After the operators walked since the last
//! [`read`](BondBound::read) run, each bond the engine reports at cut `k`
//! (between chain positions `k` and `k + 1`), and each bond a split at `k`
//! would keep without the capacity (its need), is at most the bound at
//! `k` (for a need: the bound right after the operator split there).
//!
//! **The rule.** A read sets the bound to the bonds the engine reports. A
//! one-site operator changes no cut. A two-site operator `G` on the
//! neighbours `i` and `i + 1` changes only cut `i`:
//!
//! ```text
//! β_i ← min(2·β_{i−1}, 2·β_{i+1}, r(G)·β_i, 2^min(i+1, n−i−1))
//! ```
//!
//! with `β` of 1 past either end. The block the engine splits at `i` is a
//! `(2·β_{i−1}) × (2·β_{i+1})` matrix, of at most that rank; `G`, a sum of
//! `r(G)` products ([`operator_schmidt_rank`]), multiplies the rank across
//! the cut by at most `r(G)`; and no state of `n` qubits has a larger bond
//! at `i` than the last term. Truncation only lowers a bond, so the rule
//! holds for the truncated state too; and a product of gates on one pair
//! (fused by the resumer, or by the engine before one split) has a rank of
//! at most the product of theirs, so walking them one at a time bounds the
//! fused block. The bound is not clamped to the capacity: between splits,
//! nothing truncates.
//!
//! A two-site operator on sites further apart is routed by the engine, which
//! leaves each cut it crosses at most at the capacity's bond.

use num_complex::Complex64;
use tensornet::MpsShape;

use super::super::lowering::Operator;

/// An upper bound on each bond of a state's chain (see the module
/// documentation), with the capacity it is compared with.
#[derive(Clone, Debug)]
pub(crate) struct BondBound {
    /// The bound at each cut.
    bonds: Vec<usize>,
    /// The largest bond any state has at each cut.
    exact: Vec<usize>,
    /// The capacity's bond at each cut.
    capacity: Vec<usize>,
    /// The cut of the last return SWAP of the engine's last route, while it
    /// may still be fused with what follows on its pair (see
    /// [`walk`](Self::walk)).
    route_end: Option<usize>,
    /// The two-site operators walked since the bound was last read.
    walked_since_read: usize,
}

impl BondBound {
    /// The bound of a state just prepared within `capacity`: a product
    /// state, of bonds 1, as read.
    pub(crate) fn prepared(capacity: &MpsShape) -> Self {
        let n = capacity.site_count();
        let cuts = n.saturating_sub(1);
        let exact = (0..cuts)
            .map(|cut| {
                u32::try_from((cut + 1).min(n - cut - 1))
                    .ok()
                    .and_then(|exponent| 1_usize.checked_shl(exponent))
                    .unwrap_or(usize::MAX)
            })
            .collect();
        let capacity = (0..cuts)
            .map(|cut| capacity.bond_dim(cut).expect("a cut of the chain"))
            .collect();
        Self {
            bonds: vec![1; cuts],
            exact,
            capacity,
            route_end: None,
            walked_since_read: 0,
        }
    }

    /// Sets the bound to the bonds of `shape`, which the engine reports.
    pub(crate) fn read(&mut self, shape: &MpsShape) {
        for (cut, bond) in self.bonds.iter_mut().enumerate() {
            *bond = shape.bond_dim(cut).expect("a cut of the chain");
        }
        self.route_end = None;
        self.walked_since_read = 0;
    }

    /// Whether no two-site operator was walked since the last read: the
    /// bound is then the state's bonds.
    pub(crate) fn is_read(&self) -> bool {
        self.walked_since_read == 0
    }

    /// The bound at `cut`.
    #[cfg(test)]
    pub(crate) fn at(&self, cut: usize) -> usize {
        self.bonds[cut]
    }

    /// Walks `operator`, on chain positions; whether it leaves a cut's
    /// bound above the capacity's bond there (the operator is at risk:
    /// its split could truncate). An operator the engine routes is never
    /// at risk: it truncates only as it would without routing. An engine
    /// that fuses may leave the route's last return SWAP, at cut
    /// `high − 1`, unsplit, and fuse the next operator on that pair into
    /// it: that operator's split then follows an outbound split, which the
    /// capacity does not bound, so its rank term is not used.
    pub(crate) fn walk(&mut self, operator: &Operator) -> bool {
        let &[a, b] = operator.sites() else {
            return false;
        };
        self.walked_since_read += 1;
        let (low, high) = (a.min(b), a.max(b));
        if high - low > 1 {
            self.bonds[low..high].copy_from_slice(&self.capacity[low..high]);
            self.route_end = Some(high - 1);
            return false;
        }
        let cut = low;
        let rank_term = if self.route_end.take() == Some(cut) {
            usize::MAX
        } else {
            operator_schmidt_rank(operator.matrix.row_major()).saturating_mul(self.bonds[cut])
        };
        let left = cut.checked_sub(1).map_or(1, |c| self.bonds[c]);
        let right = self.bonds.get(cut + 1).copied().unwrap_or(1);
        let bound = (2 * left)
            .min(2 * right)
            .min(rank_term)
            .min(self.exact[cut]);
        self.bonds[cut] = bound;
        bound > self.capacity[cut]
    }

    /// The cuts where `shape`, the engine's report, has a bond above the
    /// bound: none, unless the rule is wrong.
    pub(crate) fn exceeded_by(&self, shape: &MpsShape) -> usize {
        self.bonds
            .iter()
            .enumerate()
            .filter(|&(cut, &bound)| shape.bond_dim(cut).is_some_and(|bond| bond > bound))
            .count()
    }
}

/// The operator Schmidt rank of a two-qubit operator `matrix` (4×4,
/// row-major, the first operand the most significant): the fewest
/// products `A ⊗ B` it is a sum of. 1 for a product, 2 for a CNOT or a
/// `ZZ` rotation, 4 for a SWAP. It is the rank of the realigned matrix
/// `R[(a_out, a_in)][(b_out, b_in)] = M[(a_out, b_out)][(a_in, b_in)]`,
/// counted by Gaussian elimination with a tolerance relative to the
/// largest entry, so that rounding does not raise it.
pub(crate) fn operator_schmidt_rank(matrix: &[Complex64]) -> usize {
    assert_eq!(matrix.len(), 16, "a two-qubit operator");
    let mut realigned = [[Complex64::new(0.0, 0.0); 4]; 4];
    for a_out in 0..2 {
        for b_out in 0..2 {
            for a_in in 0..2 {
                for b_in in 0..2 {
                    realigned[2 * a_out + a_in][2 * b_out + b_in] =
                        matrix[(2 * a_out + b_out) * 4 + 2 * a_in + b_in];
                }
            }
        }
    }
    let largest = matrix.iter().map(|z| z.norm()).fold(0.0_f64, f64::max);
    let tolerance = 1e-10 * largest;
    let mut rank = 0;
    for column in 0..4 {
        let Some(pivot) = (rank..4)
            .max_by(|&x, &y| {
                realigned[x][column]
                    .norm()
                    .total_cmp(&realigned[y][column].norm())
            })
            .filter(|&row| realigned[row][column].norm() > tolerance)
        else {
            continue;
        };
        realigned.swap(rank, pivot);
        let top = realigned[rank];
        for row in &mut realigned[rank + 1..] {
            let factor = row[column] / top[column];
            for (x, t) in row.iter_mut().zip(top) {
                *x -= factor * t;
            }
        }
        rank += 1;
    }
    rank
}
