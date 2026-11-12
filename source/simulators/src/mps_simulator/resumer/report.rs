// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsRunReport`]: what a run of an [`MpsResumer`](super::MpsResumer)
//! cost, and whether it lost enough weight to truncation to warn the user
//! ([`TruncationWarning`]).
//!
//! A user who sets a max bond dimension χ gets an approximation: an engine
//! that truncates keeps the bonds within χ and drops the smallest Schmidt
//! values. The weight it keeps
//! ([`MpsEngine::retained_weight`](tensornet::MpsEngine::retained_weight))
//! says what that cost. The engine contract bounds the error by it only
//! for a state with one truncation and gates only after it: the fidelity
//! is then the weight, and any readout is within √(1 − weight) in total
//! variation distance. A shot's state is read after its measurements,
//! which project it: a readout conditioned on an outcome may be far off
//! though the weight is near 1. After several truncations, as in a program
//! of many gates, the weight is an estimate of the fidelity, not a bound.
//! So, for the report, the weight is the bookkeeping of what truncation
//! dropped, an estimate of the loss: the report keeps the least weight
//! among the shots that ended, and warns when one lost more than
//! [`TRUNCATION_TOLERANCE`], for the user to raise χ.
//!
//! The max bond dimension is the engine's to keep: an engine that does not
//! truncate (the exact reference) may hold bonds above it, which
//! [`MpsRunReport::largest_bond`] then shows. The resumer does not refuse
//! such a run, though the engine contract
//! ([`MpsEngine`](tensornet::MpsEngine)) leaves that check to its caller:
//! bonds above χ hold the state exactly, so a refusal would discard a
//! result more accurate than the one asked for, and the report shows them
//! instead.

use std::fmt;

/// The weight a shot may lose to truncation before a run warns.
pub const TRUNCATION_TOLERANCE: f64 = 1e-3;

/// What a run cost, over its shots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MpsRunReport {
    /// The largest bond a state held where a run of operators ended,
    /// before the measurements after it
    /// ([`MpsShape::largest_bond`](tensornet::MpsShape::largest_bond)):
    /// what the run's costliest gates paid for. One when no shot applied a
    /// gate. Above [`max_bond_dimension`](Self::max_bond_dimension) only on
    /// an engine that does not truncate.
    pub largest_bond: usize,
    /// The least weight a shot's state kept through the engine's
    /// truncations, read when the shot ended: 1 when the run was exact, to
    /// rounding.
    pub least_retained_weight: f64,
    /// The max bond dimension χ the run's states were prepared within.
    pub max_bond_dimension: usize,
}

impl MpsRunReport {
    /// The report of a run that has not prepared a state.
    pub(crate) fn new(max_bond_dimension: usize) -> Self {
        Self {
            largest_bond: 1,
            least_retained_weight: 1.0,
            max_bond_dimension,
        }
    }

    /// A warning when a shot lost more than [`TRUNCATION_TOLERANCE`] of its
    /// weight to truncation (it kept less than 1 − the tolerance); `None`
    /// when every shot lost at most that.
    #[must_use]
    pub fn truncation_warning(&self) -> Option<TruncationWarning> {
        (self.least_retained_weight < 1.0 - TRUNCATION_TOLERANCE).then_some(TruncationWarning {
            lost_weight: 1.0 - self.least_retained_weight,
            max_bond_dimension: self.max_bond_dimension,
        })
    }
}

/// A run whose truncations lost more weight than [`TRUNCATION_TOLERANCE`].
/// `lost_weight` estimates the error of its results, and bounds it only
/// where the engine contract says (one truncation, gates only after it):
/// a shot that measures after a truncation may read further off. It
/// displays as the message a user reads.
#[derive(Clone, Copy, Debug, PartialEq)]
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
