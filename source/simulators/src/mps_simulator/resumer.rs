// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsResumer`]: the [`QirResumer`] that runs each shot's quantum calls on
//! a matrix product state an [`MpsEngine`] holds.
//!
//! A user runs a QIR program on an MPS through it: a CPU or a GPU engine
//! beneath, the max bond dimension χ and the gate fusion as settings, and
//! records that do not depend on which shots run together, as the
//! [`QirResumer::run`] contract requires. What a run cost, and whether it
//! lost enough weight to truncation to warn, is in its report
//! ([`MpsResumer::last_run`]).
//!
//! How a run goes, shots that drew the same values sharing one state, is
//! in `resumer/grouped_run.rs`; this file holds the resumer and its
//! settings.

use std::num::NonZeroUsize;

use crate::OutputRecord;
use crate::qir::{QirMachine, QirResumeError, QirResumer, QirShotId};
use tensornet::MpsEngine;

mod grouped_run;
pub(crate) mod lowering;
pub(crate) mod qubit_order;
pub(crate) mod shot_stream;

use super::{GateFusion, MpsResumerError, MpsRunReport};
use grouped_run::GroupedRun;

/// The max bond dimension χ by default: 128. H₂'s phase estimation, run on
/// a CPU engine during development, kept every bond it needed within it
/// (its largest was 24).
pub const DEFAULT_MAX_BOND_DIMENSION: NonZeroUsize =
    NonZeroUsize::new(128).expect("128 is not zero");

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`, shots that drew the same values sharing one state (how: the header
/// of `resumer/grouped_run.rs`).
///
/// Each shot draws from its own stream, derived from the resumer's seed and
/// the shot's id as the executor protocol (planned) will derive the seed
/// of a chain's only participant that draws. So a shot's records depend
/// only on the program, the seed, the shot's id and the settings: not on
/// the shots beside it, nor on its place among them, nor on earlier runs.
/// The qubit order, chosen from the program's two-qubit gates, changes the
/// bonds and the time, not the records (to rounding, and within the
/// truncation error).
///
/// The cutoff below which an engine drops singular values is the engine's
/// own setting: the engine contract takes only a capacity (here
/// `MpsShape::qubit_chain(n, χ)`).
pub struct MpsResumer<E: MpsEngine> {
    engine: E,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
    fusion: GateFusion,
    last_run: MpsRunReport,
}

impl<E: MpsEngine> MpsResumer<E> {
    /// The resumer running shots on `engine`, drawing from streams derived
    /// from `seed`, with bonds of at most [`DEFAULT_MAX_BOND_DIMENSION`] and
    /// gates fused in pairs ([`GateFusion::Pairs`]).
    pub fn new(engine: E, seed: u64) -> Self {
        Self {
            engine,
            seed,
            max_bond_dimension: DEFAULT_MAX_BOND_DIMENSION,
            fusion: GateFusion::default(),
            last_run: MpsRunReport::new(DEFAULT_MAX_BOND_DIMENSION.get()),
        }
    }

    /// The resumer preparing each state with bonds of at most
    /// `max_bond_dimension` (χ): an engine that truncates keeps its chain
    /// within it, losing the weight its report gives.
    #[must_use]
    pub fn with_max_bond_dimension(mut self, max_bond_dimension: NonZeroUsize) -> Self {
        self.max_bond_dimension = max_bond_dimension;
        self
    }

    /// The resumer grouping each run of gates into operators as `fusion`
    /// says.
    #[must_use]
    pub fn with_gate_fusion(mut self, fusion: GateFusion) -> Self {
        self.fusion = fusion;
        self
    }

    /// What the last run cost: the largest bond any of its states held
    /// where a run of gates ended, and the least weight kept by a shot that
    /// ended, with the warning they call for
    /// ([`MpsRunReport::truncation_warning`]). After a failed run, what the
    /// run read before it failed. Before any run, and after a run of no
    /// shot, that of a run that prepared no state.
    #[must_use]
    pub fn last_run(&self) -> &MpsRunReport {
        &self.last_run
    }

    /// The engine, for the tests that observe it.
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &E {
        &self.engine
    }
}

impl<E: MpsEngine> QirResumer for MpsResumer<E> {
    type Error = MpsResumerError<E::Error>;

    /// Runs `shots` as [`QirResumer::run`] requires, shots that drew the
    /// same values sharing one state. It never waits: its future is ready
    /// when first polled.
    ///
    /// # Errors
    ///
    /// The first failing shot in the order of `shots`: its context trapped,
    /// or the resumer failed it ([`MpsResumerError`]): a noise or loss call,
    /// a gate on one qubit twice, a capacity that is no chain, or the
    /// engine's failure.
    async fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<Self::Error>> {
        self.last_run = MpsRunReport::new(self.max_bond_dimension.get());
        GroupedRun::start(
            &self.engine,
            machine,
            shots,
            self.seed,
            self.max_bond_dimension,
            self.fusion,
            &mut self.last_run,
        )?
        .run()
    }
}
