// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operator's rank: the singular values a window's operator keeps
//! when it is factored, those above its rank cutoff times the largest,
//! at least one.

use super::kept_operator_bond;

/// An operator's singular values, largest first, keep the bond the values
/// above `OPERATOR_RANK_CUTOFF` (`1e-12`) times the largest give. Each
/// row: the values and the bond.
///
/// - `[2, 2.2e-12]`: the second is above `2e-12`, so 2.
/// - `[2, 2e-12]`: the second is at the cutoff, `2 · 1e-12` being exactly
///   `2e-12` in `f64`, and at it is dropped, so 1.
/// - `[2, 1.8e-12]`: the second is above `1e-12` but below `2e-12`, so 1.
///   A cutoff not scaled by the largest would keep it.
/// - `[1, 0.5, 1e-13]`: the third is below `1e-12`, so 2.
/// - `[0]`: none is above the cutoff, but the bond is at least 1.
#[test]
fn keeps_the_operator_values_above_the_rank_cutoff_times_the_largest() {
    let rows: [(&[f64], usize); 5] = [
        (&[2.0, 2.2e-12], 2),
        (&[2.0, 2e-12], 1),
        (&[2.0, 1.8e-12], 1),
        (&[1.0, 0.5, 1e-13], 2),
        (&[0.0], 1),
    ];
    for (values, bond) in rows {
        assert_eq!(kept_operator_bond(values), bond, "{values:?}");
    }
}
