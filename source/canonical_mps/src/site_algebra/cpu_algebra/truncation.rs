// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Truncation`]: which singular values the CPU algebra's split
//! keeps, and what that keeps of the split's weight.
//!
//! [`SiteAlgebra::gate_split`](crate::site_algebra::SiteAlgebra::gate_split)
//! bounds what a split may drop; a backend whose library truncates keeps
//! the library's rule within those bounds. The CPU algebra
//! ([`CpuAlgebra`](super::CpuAlgebra)) computes the singular values itself,
//! and truncates them by this rule: it drops the longest tail whose squares
//! sum to at most the cutoff's share, a tail of exactly that share
//! included, so it keeps the fewest values that meet the cutoff, then at
//! most the cap.
//!
//! Every square is of a value divided by the largest, so the total, and the
//! sum of the kept values' squares, are at least 1, and neither underflows
//! however small the values are. A value so small, against the largest,
//! that its square underflows to zero counts as zero: a cutoff of 0 drops
//! it ([`Truncation::of`] says how small). The CPU algebra's splits rarely
//! meet one: its SVD first leaves out the rows of rounding's size
//! (`Svd::of`), so only a matrix whose pivoted QR hides a value that small
//! could bring one.

/// Which singular values a split keeps, from [`Truncation::of`].
#[derive(Clone, Debug, PartialEq)]
pub struct Truncation {
    /// The kept values, largest first, divided by their L2 norm (the zero
    /// spectrum's one value stays 0); their number is the split's bond.
    pub normalised_values: Vec<f64>,
    /// The share of the squared values kept, in `(0, 1]`, and exactly 1
    /// when every dropped value is zero.
    pub retained_fraction: f64,
}

impl Truncation {
    /// The truncation of the singular `values` (nonempty, nonnegative,
    /// nonincreasing) at `cutoff` (in `[0, 1)`) and `max_extent`: drop the
    /// longest tail whose squares sum to at most `cutoff` of the squares of
    /// all of them, but never the first value, then keep at most
    /// `max_extent` values. The squares are of the values divided by the
    /// largest, so a cutoff of 0 drops the zeros and the values below
    /// about 1e-162 of the largest, whose squares underflow.
    ///
    /// The values of the zero matrix, all zero, keep one value, zero, and
    /// a retained fraction of 1.
    ///
    /// # Panics
    ///
    /// When `values` is empty, or `max_extent` is 0: a split keeps at least
    /// one value.
    #[must_use]
    pub fn of(values: &[f64], cutoff: f64, max_extent: usize) -> Self {
        assert!(!values.is_empty(), "a split has singular values");
        assert!(max_extent >= 1, "a split keeps at least one value");
        let largest = values[0];
        if largest == 0.0 {
            return Self {
                normalised_values: vec![0.0],
                retained_fraction: 1.0,
            };
        }
        let square = |s: f64| (s / largest) * (s / largest);
        let total: f64 = values.iter().map(|&s| square(s)).sum();
        let allowed = cutoff * total;
        let mut kept = values.len();
        let mut discarded = 0.0;
        while kept > 1 {
            let next = discarded + square(values[kept - 1]);
            if next > allowed {
                break;
            }
            discarded = next;
            kept -= 1;
        }
        let kept = kept.min(max_extent);
        // A prefix of the same sum in the same order: exactly `total` when
        // every dropped value is zero.
        let kept_squares: f64 = values[..kept].iter().map(|&s| square(s)).sum();
        let norm = kept_squares.sqrt();
        Self {
            normalised_values: values[..kept].iter().map(|&s| s / largest / norm).collect(),
            retained_fraction: kept_squares / total,
        }
    }
}
