// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Truncation`]: which singular values the CPU algebra's split
//! keeps, and what that keeps of the split's weight.
//!
//! [`SiteAlgebra::gate_split`](crate::site_algebra::SiteAlgebra::gate_split)
//! bounds what a split may drop. A backend whose library truncates keeps
//! the library's rule within those bounds. The CPU algebra
//! ([`CpuAlgebra`](crate::CpuAlgebra)) computes the singular values itself.
//! It truncates them by this rule:
//!
//! 1. Drop the longest tail whose squares sum to at most the cutoff's
//!    share, a tail of exactly that share included. So it keeps the fewest
//!    values that meet the cutoff.
//! 2. Then keep at most the cap.
//!
//! For example, the values 0.9, 0.3, 0.3 and 0.1, at a cutoff of 0.095
//! and a cap of 2. Each value is divided by the largest, 0.9, then
//! squared:
//!
//! ```text
//!  squares       1      0.111  0.111  0.012     total 1.235
//!  cutoff's share: 0.095 × 1.235 = 0.117
//!  tail sum, from the end:
//!    0.012                                     ≤ 0.117: 0.012 dropped
//!    0.012 + 0.111 = 0.123                     > 0.117: 0.111 kept
//!  the cutoff keeps 3; the cap keeps 2: 1.111 of 1.235, a retained
//!  fraction of 0.9
//! ```
//!
//! Dividing by the largest makes the total, and the sum of the kept
//! values' squares, at least 1. Neither underflows, however small the
//! values are. A value whose square, against the largest, underflows to
//! zero counts as zero: a cutoff of 0 drops it ([`Truncation::of`] says
//! how small). The CPU algebra's splits rarely meet one. Its SVD first
//! leaves out the rows of rounding's size (`Svd::of`). So only a matrix
//! whose pivoted QR hides a value that small could bring one.

/// Which singular values a split keeps, from [`Truncation::of`].
#[derive(Clone, Debug, PartialEq)]
pub struct Truncation {
    /// The kept values, largest first, divided by their L2 norm (the zero
    /// spectrum's one value stays 0). Their number is the split's bond.
    pub normalised_values: Vec<f64>,
    /// The share of the squared values kept, in `(0, 1]`. It is exactly 1
    /// when every dropped value is zero.
    pub retained_fraction: f64,
}

impl Truncation {
    /// The truncation of the singular `values` (nonempty, nonnegative,
    /// nonincreasing) at `cutoff` (in `[0, 1)`) and `max_extent`. It drops
    /// the longest tail whose squares sum to at most `cutoff` of the
    /// squares of all of them, but never the first value. Then it keeps at
    /// most `max_extent` values. The squares are of the values divided by
    /// the largest. So a cutoff of 0 drops the zeros, and the values below
    /// about 1e-162 of the largest, whose squares underflow.
    ///
    /// The values of the zero matrix, all zero, keep one value, zero, and
    /// a retained fraction of 1.
    ///
    /// # Panics
    ///
    /// When `values` is empty, or `max_extent` is 0. A split keeps at least
    /// one value.
    #[must_use]
    pub fn of(values: &[f64], cutoff: f64, max_extent: usize) -> Self {
        assert!(!values.is_empty(), "a split has singular values");
        assert!(max_extent >= 1, "a split keeps at least one value");
        let largest = values[0];
        if largest == 0.0 {
            return Self::of_zero_values();
        }
        let total: f64 = values
            .iter()
            .map(|&value| scaled_square(value, largest))
            .sum();
        let kept_count = count_kept_by_cutoff(values, largest, cutoff * total).min(max_extent);
        Self::of_kept(&values[..kept_count], largest, total)
    }

    /// The number of values kept, the split's bond.
    #[must_use]
    pub fn bond(&self) -> usize {
        self.normalised_values.len()
    }

    /// The truncation of values that are all zero: one value, zero, and
    /// nothing of weight dropped.
    fn of_zero_values() -> Self {
        Self {
            normalised_values: vec![0.0],
            retained_fraction: 1.0,
        }
    }

    /// The `kept` values divided by their L2 norm, and the share of the
    /// `total` of the squares (each [`scaled_square`] against `largest`)
    /// they keep.
    fn of_kept(kept: &[f64], largest: f64, total: f64) -> Self {
        // A prefix of the same sum in the same order: exactly `total` when
        // every dropped value is zero.
        let kept_squares: f64 = kept
            .iter()
            .map(|&value| scaled_square(value, largest))
            .sum();
        let norm = kept_squares.sqrt();
        Self {
            normalised_values: kept.iter().map(|&value| value / largest / norm).collect(),
            retained_fraction: kept_squares / total,
        }
    }
}

/// How many of the nonincreasing `values` the cutoff keeps: all but the
/// longest tail whose squares ([`scaled_square`] against `largest`) sum,
/// from the smallest, to at most `allowed`, and never fewer than one.
fn count_kept_by_cutoff(values: &[f64], largest: f64, allowed: f64) -> usize {
    let mut kept_count = values.len();
    let mut dropped = 0.0;
    while kept_count > 1 {
        let dropped_with_next = dropped + scaled_square(values[kept_count - 1], largest);
        if dropped_with_next > allowed {
            break;
        }
        dropped = dropped_with_next;
        kept_count -= 1;
    }
    kept_count
}

/// The square of `value` divided by `largest`, the weight the truncation
/// compares. The largest value's is 1, so the sums of them do not
/// underflow (the module doc says why).
#[inline]
fn scaled_square(value: f64, largest: f64) -> f64 {
    (value / largest) * (value / largest)
}

#[cfg(test)]
mod tests;
