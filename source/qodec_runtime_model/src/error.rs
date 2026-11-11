// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PlanError`]: what reading a qodec, or planning a piece from it, reports.

use std::fmt;

/// A configuration error: a qodec, a plan, a decoder or a resource requirement
/// is not supported or not well formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError(pub String);

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlanError {}

impl From<String> for PlanError {
    fn from(message: String) -> Self {
        PlanError(message)
    }
}

impl From<&str> for PlanError {
    fn from(message: &str) -> Self {
        PlanError(message.to_string())
    }
}
