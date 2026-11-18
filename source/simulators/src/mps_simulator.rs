// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulator: runs QIR programs on matrix product states (MPS), on
//! any engine of the `tensornet` contract (for instance `tensornet`'s exact
//! reference, or one over cuTensorNet, NVIDIA's tensor network library, on
//! a GPU).
//!
//! An MPS simulates many more qubits than a state vector when a program
//! entangles them little, as chemistry programs do. A caller builds an
//! [`MpsResumer`] over an engine and runs a program's shots on it, as on
//! any [`QirResumer`]; it may rely on each shot recording what it would
//! record alone, and on a report of what truncation cost.
//!
//! [`qir`](crate::qir) runs a program's classical part and hands each
//! shot's quantum calls to a resumer ([`QirResumer`]); an engine
//! ([`MpsEngine`]) holds states and knows only sites and matrices. This module joins them: [`MpsResumer`] runs the calls of
//! each shot on an MPS that an engine holds. Everything that maps qubits to
//! sites is here: [`MpsResumer`] and its settings in this file, and what it
//! runs a program with in `mps_simulator/`, one file per part, each saying
//! in its header what it holds; how a run goes, shots that drew the same
//! values sharing one state, is in `mps_simulator/grouped_run.rs`.
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ, so a run costs what its distinct histories cost; each shot
//! draws from its own stream, so its records do not depend on the shots
//! beside it.
//!
//! An overview, with the module's files and which uses which, the phases of
//! a run and an example, is in `src/mps_simulator/README.md`; the calls
//! each phase makes, in `src/mps_simulator/design.md`.

use std::num::NonZeroUsize;

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
/// the shot's id as the executor protocol (planned) will seed the last
/// executor of a chain, the one that runs the quantum calls, in that shot
/// (the protocol's chain of executors, not the MPS's chain of sites). So a
/// shot's records depend only on the program, the seed, the shot's id and
/// the settings: not on the shots beside it, nor on its place among them,
/// nor on earlier runs.
///
/// The layout, an order of the qubits chosen from the program's two-qubit
/// gates, changes the bonds and the time, not the state (to rounding, and
/// within the truncation error of an engine that truncates): on an exact
/// engine, a record differs between two orders only where a uniform falls
/// within rounding of a probability, and the shot's later draws may then
/// differ.
///
/// The cutoff below which an engine drops singular values is the engine's
/// own setting: the engine contract takes only a capacity (here
/// `MpsShape::qubit_chain(n, χ)`).
pub struct MpsResumer<E: MpsEngine> {
    engine: E,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
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
    /// a gate on one qubit twice, a capacity that is no chain, or the
    /// engine's failure. An engine's failure to fork a group's state fails
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
                &mut self.last_run,
            )
            .and_then(GroupedRun::run),
        )
    }
}
