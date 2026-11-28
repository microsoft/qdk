// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A pair's tensors on the algebra, as the split's and the move's cases
//! handle them: uploaded before the call, then the new sites and the
//! inputs read back after it, and the read one past a new site's end.
//!
//! A pair `[l, m, r]` and a call that leaves a bond `k` give:
//!
//! ```text
//!   tensor          elements   read back as
//!   input left      2·l·m      elements, compared with the pair's left
//!   input right     m·2·r      elements, compared with the pair's right
//!   new left        2·l·k      a (2·l) × k matrix
//!   new right       k·2·r      a k × (2·r) matrix
//! ```

use super::inputs::Pair;
use super::{read, upload};
use crate::site_algebra::SiteAlgebra;
use dense_linalg::Matrix;
use num_complex::Complex64;
use std::panic::{self, AssertUnwindSafe};

/// The pair's left and right sites, uploaded in that order.
pub(super) fn upload_pair<A: SiteAlgebra>(
    algebra: &A,
    pair: &Pair,
) -> Result<[A::Tensor; 2], String> {
    let left = upload(algebra, &pair.left)?;
    let right = upload(algebra, &pair.right)?;
    Ok([left, right])
}

/// The new sites `[left, right]` of bond `k`, read back as the
/// `(2·l) × k` and `k × (2·r)` matrices, left first.
pub(super) fn read_new_sites<A: SiteAlgebra>(
    algebra: &A,
    pair: &Pair,
    [left, right]: [&A::Tensor; 2],
    k: usize,
) -> Result<[Matrix; 2], String> {
    let Pair { l, r, .. } = *pair;
    let new_left = read(algebra, left, 2 * l * k)?;
    let new_right = read(algebra, right, k * 2 * r)?;
    Ok([
        Matrix::new(2 * l, k, new_left),
        Matrix::new(k, 2 * r, new_right),
    ])
}

/// The input tensors `[left, right]`, read back after the call, left
/// first, to compare with the pair's sites.
pub(super) fn read_inputs<A: SiteAlgebra>(
    algebra: &A,
    pair: &Pair,
    [left, right]: &[A::Tensor; 2],
) -> Result<[Vec<Complex64>; 2], String> {
    let Pair { l, m, r, .. } = *pair;
    Ok([
        read(algebra, left, 2 * l * m)?,
        read(algebra, right, m * 2 * r)?,
    ])
}

/// The failure message, if a new site of bond `k` holds more than its
/// elements: for the first of `[left, right]` that answers a read of one
/// element past its end.
pub(super) fn failure_if_a_new_site_reads_past_its_end<A: SiteAlgebra>(
    algebra: &A,
    pair: &Pair,
    [left, right]: [&A::Tensor; 2],
    k: usize,
) -> Option<String> {
    let Pair { l, r, .. } = *pair;
    let sites = [("left", left, 2 * l * k), ("right", right, k * 2 * r)];
    sites.into_iter().find_map(|(side, site, held)| {
        (!refuses_a_read_past(algebra, site, held)).then(|| {
            format!(
                "bond {k}: the {side} site answered a read of {} elements, one past its {held}",
                held + 1
            )
        })
    })
}

/// Whether `algebra` refuses a read of one element more than the `held`
/// elements of `tensor`, as [`SiteAlgebra::read`] promises. It refuses with
/// an error or a panic, which this catches, rather than with elements.
fn refuses_a_read_past<A: SiteAlgebra>(algebra: &A, tensor: &A::Tensor, held: usize) -> bool {
    let answered = panic::catch_unwind(AssertUnwindSafe(|| algebra.read(tensor, held + 1).is_ok()));
    !matches!(answered, Ok(true))
}
