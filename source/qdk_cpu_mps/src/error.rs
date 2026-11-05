// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CpuMpsError`]: why a [`CpuMpsEngine`](crate::CpuMpsEngine) refused or
//! failed an operation.
//!
//! A simulator that gets an error must decide whether the state can still be
//! used. It can after a refusal: the contract refuses a call whose arguments
//! break its rules ([`Refusal`]: a site out of range, a gate on more than two
//! sites, a projection on a value of probability zero, among others), before
//! the state changes, or so that every read sees it unchanged. The engine's own
//! failures are a sample too large for memory, a factorization that failed, and
//! a truncation that would leave too little weight; the last two fail a split,
//! after which the state is unspecified and the caller drops it.
//! [`MpsEngineError::refusal`] tells a refusal from a failure.

use crate::linear_algebra::FactorizationError;
use tensornet::{MpsEngineError, Refusal};

#[cfg(doc)]
use tensornet::MpsEngine;

/// Why a [`CpuMpsEngine`](crate::CpuMpsEngine) refused or failed an
/// operation: refused, with the contract's [`Refusal`], or failed in the
/// engine, by one of the other variants.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CpuMpsError {
    /// A call the contract refuses: one whose arguments break the
    /// contract, refused before the state changed, or a projection or
    /// condition on a value of probability zero, refused after the engine
    /// applied its pending pair, as any read does, so that the state every
    /// read sees is unchanged. It displays as the refusal, word for word.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// A sample whose values do not fit in memory (`shots × sites` overflows
    /// a `usize`, or the allocation fails): a failure, not a refusal, since
    /// the arguments break no rule.
    #[error("{shots} shots of {sites} sites are more values than memory holds")]
    TooManyValues {
        /// The shots asked for.
        shots: usize,
        /// How many sites each shot draws.
        sites: usize,
    },

    /// The linear algebra failed. The state is then unspecified, as
    /// [`MpsEngine`] says, and the caller must drop it: the failed split may
    /// have lost the gates fused into its pair, a read splits a pending pair
    /// too, and a route that fails midway leaves its site moved.
    #[error(transparent)]
    Factorization(#[from] FactorizationError),

    /// A split whose truncation would take the retained weight below the
    /// smallest normal `f64` ([`f64::MIN_POSITIVE`]): below it the weight
    /// loses precision, down to zero, and callers divide by it, which is why
    /// the contract keeps it above zero. The state is then unspecified, as
    /// for [`Self::Factorization`], and the caller must drop it.
    #[error("a truncation would take the retained weight below the smallest normal f64")]
    WeightUnderflow,
}

impl MpsEngineError for CpuMpsError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::TooManyValues { .. } | Self::Factorization(_) | Self::WeightUnderflow => None,
        }
    }
}
