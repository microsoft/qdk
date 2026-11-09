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
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::OutputRecord;
use crate::qir::{QirMachine, QirResumeError, QirResumer, QirShotId};
use tensornet::MpsEngine;

mod grouped_run;
pub(crate) mod layout;
pub(crate) mod lowering;
pub(super) mod report;
pub(super) mod resumer_error;
pub(crate) mod rewrites;
pub(crate) mod shot_stream;

use grouped_run::GroupedRun;
use report::MpsRunReport;
use resumer_error::MpsResumerError;
use rewrites::fusion::GateFusion;

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`, shots that drew the same values sharing one state (how: the header
/// of `resumer/grouped_run.rs`).
///
/// Each shot draws from its own stream, derived from the resumer's seed and
/// the shot's id as the executor protocol (planned) will derive the seed
/// of the only participant that draws in an executor chain (the protocol's
/// chain of participants, not the MPS's chain of sites). So a shot's
/// records depend only on the program, the seed, the shot's id and the
/// settings: not on the shots beside it, nor on its place among them, nor
/// on earlier runs.
/// The layout, an order of the qubits chosen from the program's two-qubit
/// gates, changes the bonds and the time, not the state (to rounding, and
/// within the truncation error of an engine that truncates), so on an
/// exact engine not the records.
///
/// The cutoff below which an engine drops singular values is the engine's
/// own setting: the engine contract takes only a capacity (here
/// `MpsShape::qubit_chain(n, χ)`).
pub struct MpsResumer<E: MpsEngine> {
    engine: E,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
    fusion: GateFusion,
    stop: Option<Arc<AtomicBool>>,
    last_run: MpsRunReport,
}

/// The max bond dimension χ by default: 128. H₂'s phase estimation, run on
/// a CPU engine during development, kept every bond it needed within it
/// (its largest was 24).
pub const DEFAULT_MAX_BOND_DIMENSION: NonZeroUsize =
    NonZeroUsize::new(128).expect("128 is not zero");

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
            stop: None,
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

    /// The resumer stopping its runs once `stop` is set: a user who
    /// interrupts a run of hours (Ctrl-C from Python, say) gets it back
    /// between batches, its states freed. The flag is read before each
    /// batch a group runs, and before a group split off by a draw runs on,
    /// so a program that is one batch (Base profile) runs all the gates of
    /// the group under way before the flag is read. Once it is set, the run
    /// fails with [`MpsResumerError::Stopped`], naming the first shot that
    /// had not ended. Whoever sets it clears it before the next run.
    #[must_use]
    pub fn with_stop(mut self, stop: Arc<AtomicBool>) -> Self {
        self.stop = Some(stop);
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
        if shots.is_empty() {
            return Ok(Vec::new());
        }
        GroupedRun::start(
            &self.engine,
            machine,
            shots,
            self.seed,
            self.max_bond_dimension,
            self.fusion,
            &mut self.last_run,
        )?
        .with_stop(self.stop.as_deref())
        .run()
    }
}
