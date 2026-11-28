// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`svd_by_rotations`]: the SVD of the reduced matrix `W`, by one-sided
//! Jacobi rotations. [`ColumnMajorSvd`]: the factors it returns.
//!
//! It is step 5 of the checked SVD (the parent module). That SVD reduces
//! each matrix to `W`, of full rank: the adjoint of the rows of `R` its
//! trim keeps. It factors `W` here, then carries the factors back to the
//! input's SVD. Rotations are used for their accuracy: when `W`'s columns
//! scaled to unit norm are well conditioned, they find the small singular
//! values about as accurately, relative to their size, as the large ones
//! (Demmel and Veselić).
//!
//! Each rotation multiplies `W` on the right by a `J`: the identity, except
//! a 2 × 2 unitary on one pair of columns `p < q`. So it mixes those two
//! columns alone. `J` is chosen to make them orthogonal: a phase on `w_q`
//! makes the inner product `w_p† w_q` real, then a plane rotation by the
//! angle that zeroes it ([`mod@rotation`] gives the formula). A later
//! rotation may spoil a pair made orthogonal before, so sweeps over every
//! pair repeat until none rotates. The rotations, applied to the identity,
//! accumulate in `V = J₁·J₂·…`:
//!
//! ```text
//!  W = [w₀ w₁ w₂ …]  ── sweeps of rotations ──▶  W·V, orthogonal columns
//!  I               ── the same rotations ───▶  V
//! ```
//!
//! Then `W = U·diag(S)·V†`, once the sweeps are done and the columns sorted
//! by norm, largest first:
//!
//! - the column norms of `W·V` are the singular values;
//! - its normalized columns are the left singular vectors;
//! - `V`'s columns are the right ones.
//!
//! Where `W` is nearly orthogonal, as on the matrices of an MPS simulation
//! of H₂ (a chemistry instance the QDK simulates), a few sweeps over the
//! pairs suffice. Far from orthogonal, they take more. The Terms of
//! `docs/algorithms.md` define the words orthogonal, rotation, Gram matrix
//! and well conditioned.
//!
//! [`svd_by_rotations`] runs sweeps
//! ([`Orthogonalization::sweep_over_pairs`]) until one rotates no pair.
//! Then it reads the SVD off the columns ([`NormedColumns::into_svd`]). A
//! sweep visits every pair of columns `(p, q)`, `p < q`, row by row. Here,
//! for 4 columns:
//!
//! ```text
//!           q = 1   q = 2   q = 3
//!  p = 0      1st     2nd     3rd
//!  p = 1              4th     5th
//!  p = 2                      6th
//! ```
//!
//! At each pair, [`Orthogonalization::make_pair_orthogonal`]:
//!
//! 1. skips it when it is one of the [`SettledPairs`], found orthogonal before
//!    and unchanged since;
//! 2. asks for the rotation that makes it orthogonal
//!    ([`NormedColumns::rotation_making_orthogonal`]). There is none when its
//!    columns are orthogonal already, to [`NormedColumns`]' threshold.
//!    Then it settles the pair;
//! 3. otherwise turns the two columns by that rotation
//!    ([`mod@rotation`] draws it), and the accumulated rotations alike. It
//!    updates the two columns' squared norms from the rotation's angle,
//!    rather than measure them again, and unsettles every pair of the two
//!    columns ([`SettledPairs`]), which have changed.
//!
//! The rotations converge on a matrix of full rank, but may need more sweeps
//! than they allow ([`MAX_SWEEPS`]). They report a failure rather than answer
//! columns that are not yet orthogonal. When the last sweep allowed still
//! rotates a pair, the SVD fails
//! ([`SvdError::DidNotConverge`](super::SvdError::DidNotConverge)).
//!
//! The columns are stored as `[re…, im…]` rather than as interleaved complex
//! numbers ([`SplitComplexColumns`]), so that their loops vectorize.
//!
//! # Further reading
//!
//! - G. H. Golub and C. F. Van Loan, *Matrix Computations*, 4th ed., Johns
//!   Hopkins University Press, 2013.
//!   - §8.5, "Jacobi Methods", covers the 2 × 2 rotation and its stable
//!     tangent ([`mod@rotation`]).
//!   - §8.6, "Computing the SVD", covers Jacobi's SVD.
//! - J. Demmel and K. Veselić, "Jacobi's method is more accurate than QR",
//!   SIAM J. Matrix Anal. Appl. 13(4):1204–1245, 1992. It says why Jacobi:
//!   its accuracy on the small singular values.

mod normed_columns;
mod rotation;
mod settled_pairs;
mod split_complex_columns;

use self::normed_columns::NormedColumns;
use self::settled_pairs::SettledPairs;
use self::split_complex_columns::SplitComplexColumns;
use num_complex::Complex64;

/// An SVD `W = U·diag(S)·V†` as column-major arrays, `W` being
/// `rows × cols` and `r` the number of singular values. The fields are
/// named because `U` and `V` share a type, so a swap would compile.
/// [`svd_by_rotations`] returns one.
pub(crate) struct ColumnMajorSvd {
    /// `U`: the left singular vectors, column-major, `rows × r`, column `j`
    /// that of `singular_values[j]`.
    pub(crate) left_vectors: Vec<Complex64>,
    /// `S`: the singular values, nonincreasing, `r` of them.
    pub(crate) singular_values: Vec<f64>,
    /// `V`, not its adjoint: the right singular vectors, column-major,
    /// `cols × r`, column `j` that of `singular_values[j]`.
    pub(crate) right_vectors: Vec<Complex64>,
}

/// The sweeps the rotations allow, after which they report a failure. The
/// rotations converge quadratically once the columns are nearly orthogonal
/// (Golub and Van Loan, §8.5), so the cap leaves a wide margin.
pub(crate) const MAX_SWEEPS: usize = 30;

/// The SVD of `w` (`rows × cols`, columns stored as `[re…, im…]`,
/// `rows ≥ cols`), by multiplying it on the right by rotations,
/// `W·J₁·J₂·…`, until its columns are orthogonal. Each `Jₖ` mixes one pair
/// of columns, by the angle that makes that pair orthogonal.
/// Returns `U`, `S` and `V` ([`ColumnMajorSvd`]), column-major, their
/// columns in the order of `S`, so that `W = U·diag(S)·V†`:
///
/// - `U`, the normalized columns (`rows × cols`);
/// - `S`, the column norms, nonincreasing;
/// - `V`, the accumulated rotations' columns (`cols × cols`).
///
/// `None` when the last of `max_sweeps` sweeps still rotates a pair.
pub(crate) fn svd_by_rotations(
    rows: usize,
    cols: usize,
    w: Vec<f64>,
    max_sweeps: usize,
) -> Option<ColumnMajorSvd> {
    let mut orthogonalization = Orthogonalization::of(SplitComplexColumns::new(rows, cols, w));
    for _ in 0..max_sweeps {
        let rotated_any = orthogonalization.sweep_over_pairs();
        if !rotated_any {
            return Some(orthogonalization.into_svd());
        }
    }
    None
}

/// The columns being made orthogonal pair by pair, with the rotations
/// applied so far and the pairs found orthogonal.
struct Orthogonalization {
    /// The columns, rotated so far, with their squared norms.
    columns: NormedColumns,
    /// The product of the rotations so far, `V`: the identity at first.
    rotations: SplitComplexColumns,
    /// The pairs found orthogonal since one of their columns last rotated.
    settled: SettledPairs,
}

impl Orthogonalization {
    /// The `columns`, not yet rotated.
    fn of(columns: SplitComplexColumns) -> Self {
        let cols = columns.cols;
        Self {
            columns: NormedColumns::new(columns),
            rotations: SplitComplexColumns::identity(cols),
            settled: SettledPairs::new(cols),
        }
    }

    /// One sweep. It visits every pair of columns `p < q` in row-cyclic
    /// order (row by row of the pairs' upper triangle). It rotates each pair
    /// not yet orthogonal, applying the same rotation to `rotations`.
    /// Returns whether it rotated any pair. When it did not, every two
    /// columns are orthogonal to [`NormedColumns`]' threshold.
    fn sweep_over_pairs(&mut self) -> bool {
        let mut rotated_any = false;
        let cols = self.columns.count();
        for p in 0..cols {
            for q in p + 1..cols {
                rotated_any |= self.make_pair_orthogonal(p, q);
            }
        }
        rotated_any
    }

    /// Makes columns `p < q` orthogonal by a rotation, unless they are
    /// settled, or orthogonal already, which settles them. Returns whether it
    /// rotated them.
    fn make_pair_orthogonal(&mut self, p: usize, q: usize) -> bool {
        if self.settled.contains(p, q) {
            return false;
        }
        let Some(rotation) = self.columns.rotation_making_orthogonal(p, q) else {
            self.settled.settle(p, q);
            return false;
        };
        self.columns.rotate(p, q, &rotation);
        self.rotations.rotate(p, q, &rotation);
        self.settled.unsettle_pairs_of(p, q);
        true
    }

    /// The SVD the orthogonal columns and their rotations give
    /// ([`NormedColumns::into_svd`]).
    fn into_svd(self) -> ColumnMajorSvd {
        self.columns.into_svd(&self.rotations)
    }
}

#[cfg(test)]
mod tests;
