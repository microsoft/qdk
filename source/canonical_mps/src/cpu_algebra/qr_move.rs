// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The two moves of the centre in the CPU algebra, one per direction, each
//! one QR or LQ of the site the centre leaves.
//!
//! The site the centre leaves is factored into an isometry and a small
//! remainder (the decomposition's other factor), by `dense_linalg`'s thin
//! QR to the right and its thin LQ to the left. The isometry stays on that
//! site; the remainder is multiplied into the other site, which becomes the
//! centre.
//! The pair's product is unchanged, so nothing is lost. For a left site `A`
//! of extents `[l, 2, m]`, a right site `B` of extents `[m, 2, r]`, and `k`
//! the new bond:
//!
//! ```text
//!  to the right:   A (2l × m) = Q (2l × k) · R (k × m)
//!                  A <- Q                 [l, 2, k], orthonormal columns
//!                  B <- R · B (m × 2r)    [k, 2, r], the centre
//!
//!  to the left:    B (m × 2r) = L (m × k) · Q (k × 2r)
//!                  A <- A (2l × m) · L    [l, 2, k], the centre
//!                  B <- Q                 [k, 2, r], orthonormal rows
//! ```

use super::site_matrices::SiteMatrices;
use crate::site_algebra::QrMoveOutcome;
use dense_linalg::{Lq, Matrix, Qr};
use num_complex::Complex64;

/// Moves the centre from `leaving`, the left site, onto `right`.
pub(super) fn move_centre_right(
    leaving: &SiteMatrices<'_>,
    right: &SiteMatrices<'_>,
) -> QrMoveOutcome<Vec<Complex64>> {
    let Qr {
        q: isometry,
        r: remainder,
    } = Qr::of(leaving.qubit_in_rows_matrix());
    let centre = Matrix::product(remainder.view(), right.qubit_in_columns_matrix());
    QrMoveOutcome {
        bond: isometry.cols(),
        left: isometry.into_elements(),
        right: centre.into_elements(),
    }
}

/// Moves the centre from `leaving`, the right site, onto `left`.
pub(super) fn move_centre_left(
    left: &SiteMatrices<'_>,
    leaving: &SiteMatrices<'_>,
) -> QrMoveOutcome<Vec<Complex64>> {
    let Lq {
        l: remainder,
        q: isometry,
    } = Lq::of(leaving.qubit_in_columns_matrix());
    let centre = Matrix::product(left.qubit_in_rows_matrix(), remainder.view());
    QrMoveOutcome {
        bond: isometry.rows(),
        left: centre.into_elements(),
        right: isometry.into_elements(),
    }
}
