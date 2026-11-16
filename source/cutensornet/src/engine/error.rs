// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`EngineError`]: what a caller of the cuTensorNet engine learns when a
//! call does not succeed, and what it can do about it.
//!
//! - **Refused** ([`EngineError::Refused`], a `tensornet::Refusal`): the
//!   call broke the `MpsEngine` contract, as every engine refuses it. It is
//!   the caller's bug; the state is unchanged and may be used again.
//! - **Failed in the engine** (every other variant): a state of another
//!   engine, a sample too large for memory, or a native failure
//!   ([`EngineError::Native`]): no device, a native call that returned an
//!   error, a result the engine cannot use, or a limit of memory. The state
//!   is then unspecified, as the contract says; a native failure while the
//!   state changed leaves it poisoned ([`NativeError::Poisoned`]), so the
//!   caller drops it (an MPS simulation method fails the shot).
//!
//! `MpsEngineError::refusal` tells the two apart without matching variants.

use crate::native_calls::error::NativeError;
use tensornet::{MpsEngineError, Refusal, sampling::TooManyValues};
use thiserror::Error;

/// Why the cuTensorNet engine refused or failed an operation.
///
/// A refused call leaves the state unchanged. After a failure the state is
/// unspecified, as the contract says: a native call that failed while the
/// state was changed or computed makes every later call on it return
/// [`NativeError::Poisoned`], and only dropping it is left.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The call's arguments break the `MpsEngine` contract, refused as
    /// every engine refuses them.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// The state was used with an engine that did not create it.
    #[error("the state belongs to another engine")]
    ForeignState,

    /// A sample's values do not fit in memory.
    #[error(transparent)]
    TooManyValues(#[from] TooManyValues),

    /// A native call, or the engine's work around it, failed.
    #[error(transparent)]
    Native(#[from] NativeError),
}

impl MpsEngineError for EngineError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }
}
