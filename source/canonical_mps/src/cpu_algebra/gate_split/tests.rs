// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the CPU algebra's gate split promises beyond `SiteAlgebra`'s
//! contract: it reports a failed SVD.
#![expect(
    clippy::many_single_char_names,
    reason = "the contract's extents l, m, r and sites a, b"
)]

use crate::CpuAlgebra;
use crate::cpu_algebra::tests::c;
use crate::site_algebra::conformance::inputs::{identity_gate, random};
use crate::site_algebra::{CentreSide, GateSplitCall, SiteAlgebra};
use dense_linalg::SvdError;

/// A pair with a value that is not finite, as a fault upstream could leave:
/// the split reports the SVD's failure, with the gated pair's shape, rather
/// than spreading the value through the state.
#[test]
fn a_split_whose_svd_fails_reports_the_failure() {
    let (l, m, r) = (2, 3, 1);
    let mut a = random(2 * l, m, 22).elements().to_vec();
    a[1] = c(f64::NAN, 0.0);
    let b = random(m, 2 * r, 23).elements().to_vec();
    let call = GateSplitCall {
        left: &a,
        right: &b,
        left_extent: l,
        bond: m,
        right_extent: r,
        gate: identity_gate(),
        max_extent: usize::MAX,
        cutoff: 0.0,
        centre: CentreSide::Left,
    };
    let error = CpuAlgebra
        .gate_split(&call)
        .expect_err("a value that is not finite");
    assert_eq!(error, SvdError::FailedChecks { rows: 4, cols: 2 });
}
