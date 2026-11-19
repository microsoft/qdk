// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsEngineError`]: the bound of an engine's error, which says whether
//! a call was refused, with a [`Refusal`], or failed in the engine.
//!
//! An engine's error says either that the call was refused, with a
//! `Refusal`, or that the engine failed (out of memory, a device error, a
//! limit of its own). The caller tells them apart with
//! [`MpsEngineError::refusal`]: a refusal is a bug in the caller, the same
//! on every engine, and leaves the state unchanged, while a failure
//! belongs to the engine, and leaves the state unspecified, for the caller
//! to drop.

use crate::Refusal;

/// The error of an [`MpsEngine`](crate::MpsEngine): an engine's own type,
/// which says whether the call was refused.
///
/// An engine's error is its own, because each fails in its own ways (a
/// device error, a limit of its own), and holds a [`Refusal`] beside them,
/// built from one by `From`, so that an engine's method passes a check's
/// refusal on with `?`. It displays as the refusal it holds, word for word,
/// so a user reads the same message whatever engine refused the call.
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
