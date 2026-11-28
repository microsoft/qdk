// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of `ColumnNorms`
//! (`kernels::matrixmultiply_householder::householder_reflectors`), on
//! hand-made norms and columns: after step `k`, a later column's squared
//! norm loses its entry in row `k`, and is measured again from the rows
//! below once that leaves it at `√ε` of the norm last measured; a swap of
//! columns carries both norms.

use super::ColumnNorms;
use crate::test_utils::c;

/// Column 1 of a 2 × 2 matrix, with its squared norm `squared` and the one
/// last measured, `last_measured`, after step 0 has reflected it to
/// `[entry, 0.5]ᵀ`. The rows below row 0 hold `0.5`, of squared norm
/// `0.25`, which a measure gives. `√ε` is `2⁻²⁶`. Each case: the norms
/// before, the entry, and the norms after:
///
/// - `1`, less `0.5²`, is `0.75`, far above `2⁻²⁶`: kept;
/// - `0.25 + 2⁻²⁵`, less `0.5²`, is `2⁻²⁵`, twice `√ε` of 1: kept;
/// - `0.25 + 2⁻²⁶`, less `0.5²`, is `2⁻²⁶`, `√ε` of 1: measured again,
///   `0.25`, which becomes the norm last measured;
/// - `0.25 + 2⁻²⁵` again, against a norm last measured of 4, of which
///   `2⁻²⁵` is half `√ε`: measured again.
#[test]
fn measures_a_norm_again_once_its_downdate_falls_to_sqrt_eps_of_the_last_measured() {
    let measured = 0.25;
    let cases = [
        ((1.0, 1.0), (0.75, 1.0)),
        ((0.25 + 2_f64.powi(-25), 1.0), (2_f64.powi(-25), 1.0)),
        ((0.25 + 2_f64.powi(-26), 1.0), (measured, measured)),
        ((0.25 + 2_f64.powi(-25), 4.0), (measured, measured)),
    ];
    for ((squared, last_measured), after) in cases {
        let mut norms = ColumnNorms {
            squared: vec![1.0, squared],
            last_measured: vec![1.0, last_measured],
        };
        let columns = [c(1.0, 0.0), c(0.0, 0.0), c(0.5, 0.0), c(0.5, 0.0)];
        norms.leave_out_row(&columns, 2, 0);
        assert_eq!(
            (norms.squared[1], norms.last_measured[1]),
            after,
            "squared {squared:e}, last measured {last_measured}"
        );
    }
}

/// Columns 0 and 1 swapped, as a pivot is swapped into place: each keeps
/// its norm last measured with its squared norm, so that the next downdate
/// of each is tested against its own.
#[test]
fn a_swap_carries_each_columns_last_measured_norm() {
    let mut norms = ColumnNorms {
        squared: vec![1.0, 2.0],
        last_measured: vec![3.0, 4.0],
    };
    norms.swap(0, 1);
    assert_eq!(norms.squared, [2.0, 1.0]);
    assert_eq!(norms.last_measured, [4.0, 3.0]);
}
