// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Measurement, QubitId};
use thiserror::Error;

/// A portable MPS simulation failure.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MpsError {
    #[error("invalid execution policy: {0}")]
    InvalidPolicy(String),
    #[error("no MPS engine satisfies the requested execution policy")]
    NoEngineSatisfiesPolicy,
    #[error("qubit {0:?} is not allocated")]
    UnallocatedQubit(QubitId),
    #[error("an operation requires distinct qubits, but received {0:?} twice")]
    DuplicateQubit(QubitId),
    #[error("sites {first} and {second} are not adjacent")]
    NonAdjacentOperands { first: usize, second: usize },
    #[error("capability is planned but not implemented: {0}")]
    CapabilityNotImplemented(String),
    #[error("capability is unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("engine returned invalid probability {0}")]
    InvalidProbability(f64),
    #[error("cannot project onto zero-probability outcome {0:?}")]
    ZeroProbabilityProjection(Measurement),
    #[error("MPS engine failure: {0}")]
    EngineFailure(String),
    #[error("internal MPS invariant failed: {0}")]
    InternalInvariant(String),
}
