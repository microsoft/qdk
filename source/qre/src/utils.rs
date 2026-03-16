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

#[must_use]
pub fn float_to_bits(f: f64) -> u64 {
    f.to_bits()
}

#[must_use]
pub fn float_from_bits(b: u64) -> f64 {
    f64::from_bits(b)
}
