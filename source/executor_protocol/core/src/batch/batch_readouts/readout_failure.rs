// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a readout has no value ([`ReadoutFailure`]).

/// Why a readout has no value.
///
/// Non-exhaustive, so that a reason can be added without breaking a part
/// that matches on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadoutFailure {
    /// The qubit was lost, such as an atom that left its trap.
    Loss,
    /// Any other reason, with no detail.
    Other,
}
