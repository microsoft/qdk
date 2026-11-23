// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the CPU algebra promises beyond `SiteAlgebra`'s contract,
//! which the conformance kit checks it against
//! (`tests/site_algebra_conformance.rs`): a gate split reports a failed SVD,
//! and a read borrows a site's elements rather than copying them.
//! Its truncation rule is tested in `truncation`.
#![expect(
    clippy::many_single_char_names,
    reason = "the contract's extents l, m, r and sites a, b"
)]

mod truncation;

use super::*;
use crate::CpuAlgebra;
use crate::dense_linalg::SvdError;
use crate::site_algebra::conformance::inputs::{identity_gate, random};
use crate::site_algebra::{CentreSide, GateSplitCall, SiteAlgebra};
use std::borrow::Cow;

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

/// A tensor longer than the site it holds, as a backend that reuses
/// buffers may keep: a read returns the site's elements alone, borrowed
/// from the vector rather than copied.
#[test]
fn a_read_borrows_the_first_len_elements() {
    let tensor: Vec<Complex64> = (0..6).map(|k| c(f64::from(k), -1.0)).collect();
    let read = CpuAlgebra.read(&tensor, 4).expect("reads never fail");
    assert_eq!(*read, tensor[..4]);
    assert!(
        matches!(read, Cow::Borrowed(_)),
        "a host read copies nothing"
    );
}
