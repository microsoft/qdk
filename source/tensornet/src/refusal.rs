// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Refusal`]: why an engine refused a call whose arguments break the
//! contract, the same on every engine; and [`MpsEngineError`], how an
//! engine's own error exposes it.
//!
//! An engine's error says either that the call was refused, with a
//! `Refusal`, or that the engine failed (out of memory, a device error, a
//! limit of its own). The caller tells them apart with
//! [`MpsEngineError::refusal`]: a refusal is a bug in the caller, the same
//! on every engine, and leaves the state unchanged, while a failure
//! belongs to the engine, and leaves the state unspecified, for the caller
//! to drop.

/// Why an engine refused a call: its arguments break the contract of
/// [`MpsEngine`](crate::MpsEngine). Each variant names the methods that
/// make it, and the method's `# Errors` section names the variants.
///
/// Every engine refuses a call with one fault with the same refusal: each
/// makes the refusals of the arguments with the [`checks`](crate::checks)
/// function of its method, before the state changes. Which refusal a call
/// with several faults gets is unspecified, but for `apply`, whose
/// [`checks::apply`](crate::checks::apply) states the order it checks in.
/// The conformance kit checks the exact refusal of calls with one fault
/// each.
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
    #[error("an operator acts on at least one site")]
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

    /// An outcome of probability exactly zero (as the QDK's `PostSelectZ`
    /// refuses it), or NaN, in the normalized state: the value `project`
    /// projects a site on, or the values a condition names jointly, so no
    /// projection or conditional probability exists. Made by `project` and
    /// `conditional_probability_of_one`, which read the state, once the
    /// checks of their arguments pass. An outcome that is impossible may
    /// keep a tiny probability from rounding, and is then not refused: a
    /// method that must refuse it does so by its own threshold, before it
    /// calls the engine.
    #[error("the outcome has probability zero")]
    ZeroProbabilityOutcome,
}

/// The error of an [`MpsEngine`](crate::MpsEngine): an engine's own type,
/// which says whether the call was refused.
///
/// An engine's error is its own, because each fails in its own ways (a
/// device error, a limit of its own), and holds a [`Refusal`] beside them,
/// built from one by `From`, so that an engine's method passes a check's
/// refusal on with `?`. It displays as the refusal it holds, word for word,
/// so a user reads the same message whatever engine refused the call; the
/// conformance kit checks both.
///
/// ```
/// use tensornet::{MpsEngineError, Refusal};
///
/// #[derive(Debug, thiserror::Error)]
/// enum EngineError {
///     #[error(transparent)]
///     Refused(#[from] Refusal),
///     #[error("the device ran out of memory")]
///     OutOfMemory,
/// }
///
/// impl MpsEngineError for EngineError {
///     fn refusal(&self) -> Option<&Refusal> {
///         match self {
///             Self::Refused(refusal) => Some(refusal),
///             Self::OutOfMemory => None,
///         }
///     }
/// }
///
/// let error = EngineError::from(Refusal::NoSites);
/// assert_eq!(error.refusal(), Some(&Refusal::NoSites));
/// assert_eq!(error.to_string(), Refusal::NoSites.to_string());
/// assert_eq!(EngineError::OutOfMemory.refusal(), None);
/// ```
pub trait MpsEngineError: std::error::Error + From<Refusal> + 'static {
    /// The refusal, when the call was refused; `None` when the engine
    /// failed.
    fn refusal(&self) -> Option<&Refusal>;
}
