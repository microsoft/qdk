// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Refusal`]: why an engine refused a call, the same on every engine.

/// Why an engine refused a call: its arguments break the contract of
/// [`MpsEngine`](crate::MpsEngine), or name an outcome of probability zero
/// or NaN. Each variant names the methods that make it, and the method's
/// `# Errors` section names the variants.
///
/// A refusal leaves the state unchanged. A refusal of the arguments is a
/// bug in the caller. An engine's error exposes a refusal through
/// [`MpsEngineError::refusal`](crate::MpsEngineError::refusal), which
/// tells it apart from the engine's own failures.
///
/// Every engine makes the refusals of the arguments with
/// [`checks`](crate::checks), so all refuse a call with one fault alike.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// A capacity with a site that is not two-level: an engine holds qubits.
    /// Made by `prepare`.
    #[error("site {site} has {levels} levels, not 2")]
    NotTwoLevel {
        /// The site.
        site: usize,
        /// Its physical extent.
        levels: usize,
    },

    /// A site the state does not have. Made by every method that names a
    /// site: `apply`, `probability_of_one`, `conditional_probability_of_one`,
    /// `project`, `sample` and `expectation`.
    #[error("site {site} is out of range: the state's site count is {sites}")]
    SiteOutOfRange {
        /// The site named.
        site: usize,
        /// How many sites the state has.
        sites: usize,
    },

    /// A site named twice where distinct sites are needed: an operator's
    /// sites, a condition's sites, or the sites sampled. Made by `apply`,
    /// `conditional_probability_of_one` and `sample`.
    #[error("site {site} is named twice")]
    RepeatedSite {
        /// The site.
        site: usize,
    },

    /// An operator on no site. Made by `apply`.
    #[error("the operator acts on no site")]
    NoSites,

    /// An operator on more than two sites: an engine applies operators on
    /// one or two sites, and the caller decomposes larger ones first. Made
    /// by `apply`.
    #[error("an operator acts on at most two sites, not {sites}")]
    TooManySites {
        /// How many sites it acts on.
        sites: usize,
    },

    /// A matrix whose length is not `4^sites`. Made by `apply`.
    #[error("a matrix has {actual} elements, not {expected}: its site count is {sites}")]
    MatrixLength {
        /// How many sites it acts on: one or two.
        sites: usize,
        /// `4^sites`.
        expected: usize,
        /// Its length.
        actual: usize,
    },

    /// A matrix that is not unitary: the entries of `M·M† − I` sum, in
    /// magnitude, to `deviation`, above
    /// [`UNITARITY_TOLERANCE`](crate::checks::UNITARITY_TOLERANCE), or to NaN.
    /// Made by `apply`.
    #[error("the matrix is not unitary: the entries of M·M† − I sum to {deviation} in magnitude")]
    NotUnitary {
        /// The sum of the magnitudes of the entries of `M·M† − I`.
        deviation: f64,
    },

    /// An outcome of probability exactly zero, or NaN, in the normalized
    /// state: the value `project` projects a site on, or the values a
    /// condition names jointly. No projection or conditional probability
    /// then exists. Made by `project` and `conditional_probability_of_one`,
    /// which read the state, once the checks of their arguments pass.
    /// Passed on by `sample` in an engine that draws from its conditional
    /// probabilities.
    ///
    /// Rounding may leave an impossible outcome a tiny probability, which
    /// is then not refused. A method that must refuse it does so by its own
    /// threshold, before it calls the engine.
    #[error("the outcome has probability zero or NaN")]
    ZeroProbabilityOutcome,
}
