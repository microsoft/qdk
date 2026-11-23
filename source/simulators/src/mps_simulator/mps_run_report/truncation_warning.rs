// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TruncationWarning`]: the warning a run's report gives when a shot lost
//! more weight to truncation than [`TRUNCATION_TOLERANCE`], and the message
//! a user reads.

use std::fmt;

use super::TRUNCATION_TOLERANCE;

/// A run whose truncations lost more weight than [`TRUNCATION_TOLERANCE`];
/// when `lost_weight` bounds the error, and when it is only an estimate, is
/// in the header of `mps_simulator/mps_run_report.rs`. It displays as the
/// message a user reads.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TruncationWarning {
    /// The most weight a shot lost: 1 − its retained weight.
    pub lost_weight: f64,
    /// The max bond dimension χ that made the truncations.
    pub max_bond_dimension: usize,
}

impl fmt::Display for TruncationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the matrix product states lost up to {:.3e} of their weight to truncation at max \
             bond dimension {}, more than {TRUNCATION_TOLERANCE:e}: the results are \
             approximate; raise the max bond dimension to reduce the error",
            self.lost_weight, self.max_bond_dimension
        )
    }
}
