// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): a copy of a state's
//! site tensors, so that a development example can check a prepared state
//! against a reference MPS (an overlap). Not part of the engine contract;
//! it changes no value the engine computes.

use crate::CanonicalMps;
use num_complex::Complex64;

/// One site tensor `T[a, s, b]` of extents `[left, 2, right]`,
/// column-major: element `a + left·(s + 2·b)`.
#[derive(Clone, Debug)]
pub struct SiteTensor {
    /// The left bond.
    pub left: usize,
    /// The right bond.
    pub right: usize,
    /// The `left · 2 · right` elements.
    pub elements: Vec<Complex64>,
}

/// A copy of the site tensors of `state`, site 0 first; `None` while a
/// gate's contracted pair is not split yet (any read splits it, for
/// instance `MpsEngine::retained_weight`).
#[must_use]
pub fn site_tensors(state: &CanonicalMps) -> Option<Vec<SiteTensor>> {
    let chain = state.chain.borrow();
    if chain.pair.is_some() {
        return None;
    }
    Some(
        chain
            .sites
            .iter()
            .map(|site| SiteTensor {
                left: site.left,
                right: site.right,
                elements: site.elements.clone(),
            })
            .collect(),
    )
}
