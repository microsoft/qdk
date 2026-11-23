// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsRunReport`]: what a run of an [`MpsResumer`](super::MpsResumer)
//! cost, and whether it lost enough weight to truncation to warn the user
//! ([`TruncationWarning`], in `mps_run_report/truncation_warning.rs`).
//!
//! A user who sets a max bond dimension χ gets an approximation: an engine
//! that truncates keeps the bonds within χ and drops the smallest Schmidt
//! values. The weight it keeps says what that cost. The engine contract
//! makes it a bound only for a state with one truncation and gates only
//! after it
//! ([`MpsEngine::retained_weight`](tensornet::MpsEngine::retained_weight)).
//! A shot's state is read after its measurements, which project it: a
//! readout conditioned on an outcome may be far off though the weight is
//! near 1. After several truncations, as in a program of many gates, the
//! weight is an estimate of the fidelity, not a bound: the report keeps the
//! least weight among the shots that ended, and warns when one lost more
//! than [`TRUNCATION_TOLERANCE`], for the user to raise χ.
//!
//! The max bond dimension is the engine's to keep: an engine that does not
//! truncate (an exact engine) may hold bonds above it, which
//! [`MpsRunReport::largest_bond`] then shows. The resumer does not refuse
//! such a run, though the engine contract
//! ([`MpsEngine`](tensornet::MpsEngine)) leaves that check to its caller:
//! bonds above χ hold the state exactly, so a refusal would discard a
//! result more accurate than the one asked for, and the report shows them
//! instead.

mod truncation_warning;

pub use truncation_warning::TruncationWarning;

/// The weight a shot may lose to truncation before a run warns.
pub const TRUNCATION_TOLERANCE: f64 = 1e-3;

/// What a run cost, over its shots, as [`MpsResumer::last_run`] gives it:
/// the largest bond, against the max bond dimension χ, and the least
/// retained weight, which tell a user whether to raise χ
/// ([`truncation_warning`](Self::truncation_warning)).
///
/// [`MpsResumer::last_run`]: super::MpsResumer::last_run
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct MpsRunReport {
    /// The largest bond a state held where a sequence of gates ended,
    /// before the measurements after it
    /// ([`MpsShape::largest_bond`](tensornet::MpsShape::largest_bond)).
    /// One when no shot applied a gate. Above
    /// [`max_bond_dimension`](Self::max_bond_dimension) only on an engine
    /// that does not truncate.
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
