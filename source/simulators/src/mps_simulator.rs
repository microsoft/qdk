// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulation method: runs QIR programs on matrix product states
//! (MPS), on any [`MpsEngine`], the `tensornet` contract.
//!
//! An MPS simulates many more qubits than a state vector when a program
//! entangles them little, as chemistry programs do. A caller builds an
//! [`MpsResumer`] over an engine and runs a program's shots on it, as on
//! any [`QirResumer`]; it may rely on each shot recording what it would
//! record alone, and on a report of what truncation cost.
//!
//! [`qir`](crate::qir) runs a program's classical part and hands each
//! shot's quantum calls to a resumer; an engine holds states and knows only
//! sites and matrices. This module joins them: [`MpsResumer`], in this
//! file, runs the calls of each shot on an MPS that an engine holds, and
//! its parts, in `mps_simulator/`, map qubits to sites (`qubit_order.rs`),
//! turn calls into steps on sites (`lowering.rs`), run the shots in groups
//! (`grouped_run.rs`), give each shot its stream (`shot_stream.rs`), and
//! report and fail a run (`run_report.rs`, `error.rs`).
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ, so a run costs what its distinct histories cost; each shot
//! draws from its own stream, so its records do not depend on the shots
//! beside it.
//!
//! An overview, with the phases of a run, the public types and an example,
//! is in `src/mps_simulator/README.md`; each phase's mechanics and engine
//! calls, in `src/mps_simulator/design.md`.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::OutputRecord;
use crate::qir::{QirMachine, QirResumeError, QirResumer, QirShotId};
use tensornet::MpsEngine;

mod error;
mod grouped_run;
pub(crate) mod lowering;
pub(crate) mod qubit_order;
mod run_report;
pub(crate) mod shot_stream;

pub use error::MpsResumerError;
pub use run_report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};

use grouped_run::GroupedRun;

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`, shots that drew the same values sharing one state (how: the header
/// of `mps_simulator/grouped_run.rs`).
///
/// Each shot draws from its own stream, seeded from the resumer's seed and
/// the shot's id (the rule is in `mps_simulator/shot_stream.rs`). So a
/// shot's records depend only on the program, the seed, the shot's id and
/// the settings: not on the shots beside it, nor on its place among them,
/// nor on earlier runs.
///
/// The qubit order, which site holds each qubit, chosen from the program's
/// two-qubit gates, changes the bonds and the time, not the state (to
/// rounding, and within the truncation error of an engine that truncates):
/// on an exact engine, a record differs between two orders only where a
/// uniform falls within rounding of a probability, and the shot's later
/// draws may then differ.
///
/// The cutoff below which an engine drops singular values is the engine's
/// own setting: the engine contract takes only a capacity (here
/// `MpsShape::qubit_chain(n, χ)`).
pub struct MpsResumer<E: MpsEngine> {
    engine: E,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
    stop_flag: Option<Arc<AtomicBool>>,
    last_run: MpsRunReport,
}

/// The max bond dimension χ by default: 128. H₂'s iterative phase
/// estimation (29 qubits), run on a CPU engine with this default, kept
/// every bond it needed within it (its largest was 24).
pub const DEFAULT_MAX_BOND_DIMENSION: NonZeroUsize =
    NonZeroUsize::new(128).expect("128 is not zero");

impl<E: MpsEngine> MpsResumer<E> {
    /// The resumer running shots on `engine`, drawing from streams derived
    /// from `seed`, with bonds of at most [`DEFAULT_MAX_BOND_DIMENSION`].
    pub fn new(engine: E, seed: u64) -> Self {
        Self {
            engine,
            seed,
            max_bond_dimension: DEFAULT_MAX_BOND_DIMENSION,
            stop_flag: None,
            last_run: MpsRunReport::new(DEFAULT_MAX_BOND_DIMENSION.get()),
        }
    }

    /// The resumer preparing each state with bonds of at most
    /// `max_bond_dimension` (χ): an engine that truncates keeps its chain
    /// within it, losing the weight its report gives.
    #[must_use]
    pub fn with_max_bond_dimension(mut self, max_bond_dimension: NonZeroUsize) -> Self {
        self.max_bond_dimension = max_bond_dimension;
        self.last_run = MpsRunReport::new(max_bond_dimension.get());
        self
    }

    /// The resumer stopping its runs once `stop_flag` is set: a user who
    /// interrupts a run of hours (Ctrl-C from Python, say) gets it back
    /// between batches, its states freed. The flag is read before each
    /// batch a group runs, and before a group split off by a draw runs on,
    /// so a program that is one batch (Base profile) runs all the gates of
    /// the group under way before the flag is read. Once it is set, the run
    /// fails with [`MpsResumerError::Stopped`], naming the first shot that
    /// had not ended. Whoever sets it clears it before the next run.
    #[must_use]
    pub fn with_stop_flag(mut self, stop_flag: Arc<AtomicBool>) -> Self {
        self.stop_flag = Some(stop_flag);
        self
    }

    /// What the last run cost: the largest bond any of its states held
    /// where a run of gates ended, and the least weight kept by a shot
    /// that ended, with the warning they call for
    /// ([`MpsRunReport::truncation_warning`]). After a failed run, what it
    /// read until it stopped: groups holding a shot before the failing one
    /// run on after the failure, and what they read counts too. Before any
    /// run, and after a run of no shot, that of a run that prepared no state.
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
    /// same values sharing one state. It never waits: it runs them when
    /// called, and its future is ready at once.
    ///
    /// # Errors
    ///
    /// The first failing shot in the order of `shots`: its context trapped,
    /// or the resumer failed it ([`MpsResumerError`]): a noise or loss call,
    /// a gate on one qubit twice, a capacity that is no chain, the engine's
    /// failure, or the stop flag, once set, naming the first shot that had
    /// not ended. An engine's failure to fork a group's state fails
    /// all the group's shots, as the engine leaves that state unspecified;
    /// its failure to settle the fork (project it, and apply a reset's X)
    /// fails only the shots that were to wait with the fork. Either fails
    /// shots that alone would not have forked: a shot's failure, unlike its
    /// records, may depend on the shots beside it.
    fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> impl Future<Output = Result<Vec<Vec<OutputRecord>>, QirResumeError<Self::Error>>> {
        self.last_run = MpsRunReport::new(self.max_bond_dimension.get());
        if shots.is_empty() {
            return std::future::ready(Ok(Vec::new()));
        }
        std::future::ready(
            GroupedRun::start(
                &self.engine,
                machine,
                shots,
                self.seed,
                self.max_bond_dimension,
                self.stop_flag.as_deref(),
                &mut self.last_run,
            )
            .and_then(GroupedRun::run),
        )
    }
}
