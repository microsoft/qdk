// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsRunReport`]: what a run of an [`MpsResumer`](super::MpsResumer)
//! cost. A user who set a max bond dimension χ reads it to know whether to
//! raise χ. A run whose shots lost more than [`TRUNCATION_TOLERANCE`] of
//! their weight to truncation warns ([`TruncationWarning`]).

mod truncation_warning;

pub use truncation_warning::TruncationWarning;

#[cfg(doc)]
use tensornet::MpsShape;

/// The weight a shot loses to truncation above which its run warns
/// ([`MpsRunReport::truncation_warning`]).
///
/// Chosen so that a run that does not warn has lost at most about 1 %
/// fidelity: on the cases measured (random circuits, an Ising quench,
/// polyene and H₂ phase estimation), the infidelity stayed within 11× the
/// lost weight.
pub const TRUNCATION_TOLERANCE: f64 = 1e-3;

/// What a run cost, over its shots, as [`MpsResumer::last_run`] gives it:
/// the largest bond dimension, against the max bond dimension χ, and the
/// least retained weight. They tell a user whether to raise χ
/// ([`truncation_warning`](Self::truncation_warning)).
///
/// A user who sets χ gets an approximation. An engine that truncates keeps
/// the bonds within χ and drops the smallest Schmidt values: the singular
/// values of the state across a cut. The weight it keeps says what that
/// cost.
///
/// χ is the engine's to keep. An engine that does not truncate (an exact
/// engine) holds the bonds the state needs, even where they exceed χ, and
/// [`largest_bond_dimension`](Self::largest_bond_dimension) then shows
/// them. The resumer does not refuse such a run: its state is exact.
///
/// [`MpsResumer::last_run`]: super::MpsResumer::last_run
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct MpsRunReport {
    /// The largest bond dimension a state held where a sequence of gates
    /// ended, before the measurements after it
    /// ([`MpsShape::largest_bond_dimension`]). One when no shot applied a
    /// gate. Above [`max_bond_dimension`](Self::max_bond_dimension) only on
    /// an engine that does not truncate.
    pub largest_bond_dimension: usize,
    /// The least weight a shot's state kept through the engine's
    /// truncations, read when the shot ended. Up to rounding, it is 1 when
    /// the run was exact.
    ///
    /// The engine contract makes it a bound only for a state with one
    /// truncation and gates only after it
    /// ([`MpsEngine::retained_weight`](tensornet::MpsEngine::retained_weight)).
    /// A shot's state is read after its measurements, which project it. So
    /// a readout conditioned on an outcome may be far off though the weight
    /// is near 1. After several truncations, as in a program of many gates,
    /// the weight is an estimate of the fidelity, not a bound.
    pub least_retained_weight: f64,
    /// The max bond dimension χ the run's states were prepared within.
    pub max_bond_dimension: usize,
}

impl MpsRunReport {
    /// The report of a run that has not prepared a state.
    pub(crate) fn new(max_bond_dimension: usize) -> Self {
        Self {
            largest_bond_dimension: 1,
            least_retained_weight: 1.0,
            max_bond_dimension,
        }
    }

    /// A warning when a shot lost more than [`TRUNCATION_TOLERANCE`] of its
    /// weight to truncation, that is kept less than 1 − the tolerance.
    /// `None` when every shot lost at most that.
    #[must_use]
    pub fn truncation_warning(&self) -> Option<TruncationWarning> {
        (self.least_retained_weight < 1.0 - TRUNCATION_TOLERANCE).then_some(TruncationWarning {
            lost_weight: 1.0 - self.least_retained_weight,
            max_bond_dimension: self.max_bond_dimension,
        })
    }
}

#[cfg(test)]
mod tests;
