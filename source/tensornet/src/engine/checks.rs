// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The argument checks of every [`MpsEngine`](crate::MpsEngine) method:
//! one function per method, which every engine calls before it changes the
//! state, so that every engine refuses a call with one fault with the same
//! [`Refusal`]. They make every refusal but
//! [`Refusal::ZeroProbabilityOutcome`], which reads the state, so the
//! projection and the conditional probability make it after their checks.
//!
//! Each function takes the method's arguments in the method's order, with
//! the state's site count in place of the state, and returns the refusal
//! of a fault it finds. Which refusal a call with several faults gets is
//! unspecified, but for `apply`, whose function states the order it checks
//! in: no caller may rely on it for the other methods. An engine passes the
//! refusal on with `?`:
//!
//! ```
//! use num_complex::Complex64;
//! use tensornet::{Refusal, checks};
//!
//! let x = [0.0, 1.0, 1.0, 0.0].map(|re| Complex64::new(re, 0.0));
//! assert_eq!(checks::apply(2, &[1], &x), Ok(()));
//! assert_eq!(
//!     checks::apply(2, &[2], &x),
//!     Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
//! );
//! ```
//!
//! An `apply` on `k` sites checks `8^k` products: 8 for a one-site gate,
//! 64 for a two-site one.

use crate::{MpsShape, PauliSum, Refusal};
use num_complex::Complex64;

/// How far the entries of `M·M† − I` may sum, in magnitude, for `apply` to
/// take `M` as unitary: the sparse simulator's tolerance for Q#'s `Apply`,
/// so a matrix one simulator takes, the other takes too.
pub const UNITARITY_TOLERANCE: f64 = 1e-9;

/// The refusal of [`MpsEngine::prepare`](crate::MpsEngine::prepare): a
/// site of `capacity` that is not two-level.
///
/// # Errors
///
/// [`Refusal::NotTwoLevel`].
pub fn prepare(capacity: &MpsShape) -> Result<(), Refusal> {
    match (0..capacity.site_count())
        .map(|site| {
            let levels = capacity
                .physical_dim(site)
                .expect("a site below the capacity's site count has a physical extent");
            (site, levels)
        })
        .find(|&(_, levels)| levels != 2)
    {
        Some((site, levels)) => Err(Refusal::NotTwoLevel { site, levels }),
        None => Ok(()),
    }
}

/// The refusal of [`MpsEngine::apply`](crate::MpsEngine::apply) on a state
/// of `site_count` sites: no site or more than two, a site out of range or
/// named twice, a matrix whose length is not `4^sites.len()`, or a matrix
/// of that length that is not unitary.
///
/// It checks in that order, and refuses the first fault it finds: the
/// number of sites, then the sites, then the matrix's length, then its
/// unitarity. So an operator on more than two sites is refused as such,
/// whatever its sites and its matrix.
///
/// A matrix `M` is unitary when the entries of `M·M† − I` sum, in
/// magnitude, to at most [`UNITARITY_TOLERANCE`]: the sparse simulator's
/// rule for Q#'s `Apply` (`qsc_eval`'s `backend.rs`), so that both refuse
/// the same matrices. A sum that is NaN (a matrix with a NaN or an infinite
/// entry) is refused too, which the sparse simulator's rule does not.
///
/// # Errors
///
/// [`Refusal::NoSites`], [`Refusal::TooManySites`],
/// [`Refusal::SiteOutOfRange`], [`Refusal::RepeatedSite`],
/// [`Refusal::MatrixLength`] and [`Refusal::NotUnitary`].
pub fn apply(site_count: usize, sites: &[usize], matrix: &[Complex64]) -> Result<(), Refusal> {
    if sites.is_empty() {
        return Err(Refusal::NoSites);
    }
    if sites.len() > 2 {
        return Err(Refusal::TooManySites { sites: sites.len() });
    }
    distinct_sites(site_count, sites, |&site| site)?;
    let expected = 1 << (2 * sites.len());
    if matrix.len() != expected {
        return Err(Refusal::MatrixLength {
            sites: sites.len(),
            expected,
            actual: matrix.len(),
        });
    }
    // The sparse simulator's rule. If this check ever costs too much beside
    // an engine's work, the fallback is a `Unitary` matrix type, checked once
    // when built, that `apply` takes; not an unchecked `apply`.
    let deviation = unitarity_deviation(matrix, 1 << sites.len());
    if deviation <= UNITARITY_TOLERANCE {
        Ok(())
    } else {
        Err(Refusal::NotUnitary { deviation })
    }
}

/// `Σ_{i,j} |(M·M†)[i][j] − δ_ij|` for the row-major `dim`×`dim` matrix `M`.
pub(crate) fn unitarity_deviation(matrix: &[Complex64], dim: usize) -> f64 {
    let mut deviation = 0.0;
    for (i, row) in matrix.chunks_exact(dim).enumerate() {
        for (j, other) in matrix.chunks_exact(dim).enumerate() {
            let product: Complex64 = row.iter().zip(other).map(|(a, b)| a * b.conj()).sum();
            let identity = if i == j { 1.0 } else { 0.0 };
            // `norm_sqr().sqrt()`, not `norm()`: `hypot` avoids an overflow
            // only an entry far from unitary reaches (refused either way, as
            // infinity), and costs twice as much.
            deviation += (product - identity).norm_sqr().sqrt();
        }
    }
    deviation
}

/// The refusal of
/// [`MpsEngine::probability_of_one`](crate::MpsEngine::probability_of_one)
/// on a state of `site_count` sites: `site` out of range.
///
/// # Errors
///
/// [`Refusal::SiteOutOfRange`].
pub fn probability_of_one(site_count: usize, site: usize) -> Result<(), Refusal> {
    in_range(site_count, site)
}

/// `site` out of range.
fn in_range(site_count: usize, site: usize) -> Result<(), Refusal> {
    if site < site_count {
        Ok(())
    } else {
        Err(Refusal::SiteOutOfRange {
            site,
            sites: site_count,
        })
    }
}

/// The refusals of [`MpsEngine::conditional_probability_of_one`][method]
/// that its arguments alone decide, on a state of `site_count` sites: a
/// site of `given` out of range or named twice, or `site` out of range. A
/// condition of probability zero ([`Refusal::ZeroProbabilityOutcome`])
/// is refused by the method, which reads the state, once these pass.
///
/// `site` may be among the sites of `given`: the answer is then 0 or 1.
///
/// # Errors
///
/// [`Refusal::SiteOutOfRange`] and [`Refusal::RepeatedSite`].
///
/// [method]: crate::MpsEngine::conditional_probability_of_one
pub fn conditional_probability_of_one(
    site_count: usize,
    site: usize,
    given: &[(usize, bool)],
) -> Result<(), Refusal> {
    distinct_sites(site_count, given, |&(at, _)| at)?;
    in_range(site_count, site)
}

/// The refusal of [`MpsEngine::project`](crate::MpsEngine::project) that
/// its arguments alone decide, on a state of `site_count` sites: `site` out
/// of range. An outcome of probability zero
/// ([`Refusal::ZeroProbabilityOutcome`]) is refused by the method, which
/// reads the state, once this passes.
///
/// # Errors
///
/// [`Refusal::SiteOutOfRange`].
pub fn project(site_count: usize, site: usize) -> Result<(), Refusal> {
    in_range(site_count, site)
}

/// The refusal of [`MpsEngine::sample`](crate::MpsEngine::sample) on a
/// state of `site_count` sites: a site out of range or named twice. An
/// empty list of sites is no fault in itself; the number of shots is not
/// checked, so zero shots of a site out of range, or of a repeated site,
/// are refused.
///
/// # Errors
///
/// [`Refusal::SiteOutOfRange`] and [`Refusal::RepeatedSite`].
pub fn sample(site_count: usize, sites: &[usize]) -> Result<(), Refusal> {
    distinct_sites(site_count, sites, |&site| site)
}

/// The refusal of [`MpsEngine::expectation`](crate::MpsEngine::expectation)
/// on a state of `site_count` sites: a factor, of any term, on a site out
/// of range.
///
/// # Errors
///
/// [`Refusal::SiteOutOfRange`].
pub fn expectation(site_count: usize, observable: &PauliSum) -> Result<(), Refusal> {
    observable
        .terms()
        .iter()
        .flat_map(|term| term.factors().iter().map(|&(site, _)| site))
        .try_for_each(|site| in_range(site_count, site))
}

/// A site of `items` out of range or named twice. Takes the items as they
/// come (sites, or conditions), so that the check of a conditional
/// probability, called once per distinct prefix of a per-shot draw,
/// allocates nothing; its time is quadratic in the number of items, which
/// is small.
fn distinct_sites<T>(
    site_count: usize,
    items: &[T],
    site_of: impl Fn(&T) -> usize,
) -> Result<(), Refusal> {
    for (at, item) in items.iter().enumerate() {
        let site = site_of(item);
        in_range(site_count, site)?;
        if items[..at].iter().any(|earlier| site_of(earlier) == site) {
            return Err(Refusal::RepeatedSite { site });
        }
    }
    Ok(())
}
