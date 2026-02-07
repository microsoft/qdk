// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use probability::prelude::{Binomial, Inverse};

#[allow(clippy::doc_markdown)]
/// Faster implementation of SciPy's binom.ppf
#[must_use]
pub fn binom_ppf(q: f64, n: usize, p: f64) -> usize {
    let dist = Binomial::with_failure(n, 1.0 - p);
    dist.inverse(q)
}
