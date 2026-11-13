// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): [`CudaMpsError`], why a
//! [`CudaMpsEngine`](super::CudaMpsEngine) refused or failed an operation,
//! after `qdk_cpu_mps`'s `CpuMpsError`, with a failed tensor call in place
//! of a failed factorization.

use tensornet::sampling::TooManyValues;
use tensornet::{MpsEngineError, Refusal};

/// Why a [`CudaMpsEngine`](super::CudaMpsEngine) refused or failed an
/// operation: refused, with the contract's [`Refusal`], or failed in the
/// engine, by one of the other variants.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CudaMpsError {
    /// A call the contract refuses. It displays as the refusal, word for
    /// word.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// A sample whose values do not fit in memory.
    #[error("{shots} shots of {sites} sites are more values than memory holds")]
    TooManyValues {
        /// The shots asked for.
        shots: usize,
        /// How many sites each shot draws.
        sites: usize,
    },

    /// A tensor call failed (a CUDA or cuTensorNet status, or the host
    /// stand-in's SVD). The state is then unspecified and the caller must
    /// drop it.
    #[error("a tensor call failed: {0}")]
    TensorCallFailed(String),

    /// A split whose truncation would take the retained weight below
    /// [`f64::MIN_POSITIVE`]; the state is then unspecified.
    #[error("a truncation would take the retained weight below the smallest normal f64")]
    WeightUnderflow,
}

impl From<TooManyValues> for CudaMpsError {
    fn from(TooManyValues { shots, sites }: TooManyValues) -> Self {
        Self::TooManyValues { shots, sites }
    }
}

impl MpsEngineError for CudaMpsError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::TooManyValues { .. } | Self::TensorCallFailed(_) | Self::WeightUnderflow => None,
        }
    }
}
