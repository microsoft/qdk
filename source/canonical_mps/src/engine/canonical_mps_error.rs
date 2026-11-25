// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsError`]: why a
//! [`CanonicalMpsEngine`](super::CanonicalMpsEngine) refused or failed an
//! operation.
//!
//! A simulator that gets an error must decide whether the state can still
//! be used. After a refusal ([`CanonicalMpsError::Refused`]), it can: every
//! read sees the state unchanged. After every other variant, a failure, the
//! state is unspecified, as [`MpsEngine`] says, and the caller drops it.
//! [`MpsEngineError::refusal`] tells a refusal from a failure.

use tensornet::sampling::TooManyValues;
use tensornet::{MpsEngineError, Refusal};

#[cfg(doc)]
use crate::site_algebra::SiteAlgebra;
#[cfg(doc)]
use tensornet::MpsEngine;

/// Why a [`CanonicalMpsEngine`](super::CanonicalMpsEngine) on an algebra
/// whose error is `E` refused or failed an operation. A refusal carries the
/// contract's [`Refusal`]. The other variants are failures.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CanonicalMpsError<E> {
    /// A call the contract refuses. Either way, the state every read sees
    /// is unchanged:
    ///
    /// - a call whose arguments break the contract is refused before any
    ///   call of the algebra;
    /// - a projection or condition on a value of probability zero (or NaN)
    ///   is refused after the engine applied its pending window, as a read
    ///   does.
    ///
    /// It displays as the refusal, word for word.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// A call of the algebra failed ([`SiteAlgebra::Error`], its source).
    /// A failed application of the pending window may have lost its gates.
    /// A read applies a pending window too. The state is then unspecified
    /// ([`CanonicalMpsEngine`](super::CanonicalMpsEngine)'s Failures).
    #[error("the site algebra failed")]
    Algebra(#[source] E),

    /// A sample whose values do not fit in memory: `shots × sites`
    /// overflows a `usize`, or the allocation fails. The arguments break no
    /// rule, so it is a failure, not a refusal.
    #[error("{shots} shots of {sites} sites are more values than memory holds")]
    TooManyValues {
        /// The shots asked for.
        shots: usize,
        /// How many sites each shot draws.
        sites: usize,
    },

    /// A split whose truncation would take the retained weight below the
    /// smallest normal `f64` ([`f64::MIN_POSITIVE`]). Below it, the weight
    /// loses precision, down to zero. Callers divide by it, so the contract
    /// keeps it above zero.
    #[error("a truncation would take the retained weight below the smallest normal f64")]
    WeightUnderflow,

    /// The factorisation of a pending window's operator into a matrix
    /// product operator failed. The host's SVD of the operator
    /// ([`SvdError`](crate::SvdError), its source) did not converge or
    /// failed its checks. It is not expected. A window holds only gates
    /// checked finite and unitary, whose SVD converges. So only a fault of
    /// the host's SVD would reach it.
    #[error("the factorisation of a window's operator failed")]
    OperatorDecomposition(#[source] crate::SvdError),
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

#[cfg(test)]
mod tests;
