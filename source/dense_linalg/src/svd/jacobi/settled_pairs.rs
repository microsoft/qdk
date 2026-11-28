// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SettledPairs`]: the pairs of columns a sweep may skip, because they
//! were found orthogonal and neither column has turned since.

/// The pairs of columns whose product `w_p† w_q` was found below the
/// threshold and has not changed since, because neither column has rotated.
/// A sweep skips them rather than compute the product again.
///
/// It keeps three clocks:
///
/// - one that counts the rotations;
/// - the clock at each column's last rotation;
/// - the clock at which each pair was settled (0: never).
///
/// So a rotation unsettles the pairs of its two columns at the cost of two
/// writes, not one per pair.
pub(super) struct SettledPairs {
    /// The number of columns.
    cols: usize,
    clock: usize,
    rotated_at: Vec<usize>,
    /// The clock when pair `(p, q)` was settled, at `p·cols + q`.
    settled_at: Vec<usize>,
}

impl SettledPairs {
    /// No pair of `cols` columns settled.
    pub(super) fn new(cols: usize) -> Self {
        Self {
            cols,
            clock: 1,
            rotated_at: vec![0; cols],
            settled_at: vec![0; cols * cols],
        }
    }

    /// Whether the pair `p < q` was settled and neither column has rotated
    /// since.
    pub(super) fn contains(&self, p: usize, q: usize) -> bool {
        let settled_at = self.settled_at[p * self.cols + q];
        settled_at != 0 && self.rotated_at[p] <= settled_at && self.rotated_at[q] <= settled_at
    }

    /// Settles the pair `p < q`, found below the threshold.
    pub(super) fn settle(&mut self, p: usize, q: usize) {
        self.settled_at[p * self.cols + q] = self.clock;
    }

    /// Unsettles every pair of column `p` or `q`, which have just rotated.
    pub(super) fn unsettle_pairs_of(&mut self, p: usize, q: usize) {
        self.clock += 1;
        self.rotated_at[p] = self.clock;
        self.rotated_at[q] = self.clock;
    }
}

#[cfg(test)]
mod tests;
