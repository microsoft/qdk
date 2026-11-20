// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One term of a step's guard ([`Guard`]): a parameter, or an earlier
//! outcome.

/// One term of a step's guard, Qodec's `if: [correction, outcomes[0]]`:
/// the instruction's `bit` parameter at a position, or an outcome of an
/// earlier observation of the same action, counted across them from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Guard {
    /// The instruction's `bit` parameter at this position, from 0.
    Parameter(usize),
    /// The action's outcome at this position, from 0.
    Outcome(u32),
}
