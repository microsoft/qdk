// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsError`]: why a [`CanonicalMpsEngine`] refused or failed
//! an operation.
//!
//! A simulator that gets an error must decide whether the state can still
//! be used. It can after a refusal: the contract refuses a call whose
//! arguments break its rules ([`Refusal`]: a site out of range, a gate on
//! more than two sites, a projection on a value of probability zero, among
//! others), before the state changes, or so that every read sees it
//! unchanged. The engine's own failures are a sample too large for memory,
//! a failure of the backend, a truncation that would leave too little
//! weight, and a failed factorisation of a window's operator; the last
//! three may fail the application of a pending window, after which the
//! state is unspecified and the caller drops it.
//! [`MpsEngineError::refusal`] tells a refusal from a failure.
//!
//! [`CanonicalMpsEngine`]: super::CanonicalMpsEngine

use tensornet::sampling::TooManyValues;
use tensornet::{MpsEngineError, Refusal};

#[cfg(doc)]
use crate::algebra::SiteAlgebra;
#[cfg(doc)]
use tensornet::MpsEngine;

/// Why a [`CanonicalMpsEngine`](super::CanonicalMpsEngine) on an algebra
/// whose error is `E` refused or failed an operation: refused, with the
/// contract's [`Refusal`], or failed, by one of the other variants.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CanonicalMpsError<E> {
    /// A call the contract refuses: one whose arguments break the
    /// contract, refused before the state changed, or a projection or
    /// condition on a value of probability zero, refused after the engine
    /// applied its pending window, as any read does, so that the state every
    /// read sees is unchanged. It displays as the refusal, word for word.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// A call of the algebra failed ([`SiteAlgebra::Error`], its source).
    /// The state is then unspecified, as [`MpsEngine`] says, and the
    /// caller must drop it: a failed application of the pending window
    /// may have lost its gates, or left its span half compressed, and a
    /// read applies a pending window too. A failed read or upload of a site
    /// of an MPO's span leaves the span's sites unmultiplied, so neighbours
    /// still share each bond.
    #[error("the site algebra failed")]
    Algebra(#[source] E),

    /// A sample whose values do not fit in memory (`shots × sites`
    /// overflows a `usize`, or the allocation fails): a failure, not a
    /// refusal, since the arguments break no rule.
    #[error("{shots} shots of {sites} sites are more values than memory holds")]
    TooManyValues {
        /// The shots asked for.
        shots: usize,
        /// How many sites each shot draws.
        sites: usize,
    },

    /// A split whose truncation would take the retained weight below the
    /// smallest normal `f64` ([`f64::MIN_POSITIVE`]): below it the weight
    /// loses precision, down to zero, and callers divide by it, which is
    /// why the contract keeps it above zero. The state is then unspecified,
    /// as for [`Self::Algebra`], and the caller must drop it.
    #[error("a truncation would take the retained weight below the smallest normal f64")]
    WeightUnderflow,

    /// The factorisation of a pending window's operator into a matrix
    /// product operator failed: the host's SVD of the operator
    /// ([`SvdError`](dense_linalg::SvdError), its source) did not converge
    /// or failed its checks. The factorisation runs before the window
    /// leaves the chain and before any call of the algebra, so the error
    /// keeps the pending window, its sites and its operator, and the
    /// chain's sites, centre, pending one-site gates, retained weight and
    /// statistics as they were when the window's application began. The
    /// call that applied the window has taken no other effect, except that,
    /// without fusion, a gate's call has put its own gate in the window
    /// first. The state is still unspecified, as for [`Self::Algebra`], and
    /// the caller must drop it. It is not expected: a window holds only
    /// gates checked finite and unitary, whose SVD converges, so only a
    /// fault of the host's SVD would reach it.
    #[error("the factorisation of a window's operator failed")]
    OperatorDecomposition(#[source] dense_linalg::SvdError),
}

impl<E> From<TooManyValues> for CanonicalMpsError<E> {
    fn from(TooManyValues { shots, sites }: TooManyValues) -> Self {
        Self::TooManyValues { shots, sites }
    }
}

impl<E: std::error::Error + 'static> MpsEngineError for CanonicalMpsError<E> {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::Algebra(_)
            | Self::TooManyValues { .. }
            | Self::WeightUnderflow
            | Self::OperatorDecomposition(_) => None,
        }
    }
}
