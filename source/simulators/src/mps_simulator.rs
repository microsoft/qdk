// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulation method: runs QIR programs on matrix product states
//! (MPS), on any [`MpsEngine`], the `tensornet` contract.
//!
//! An MPS simulates many more qubits than a state vector when a program
//! entangles them little, as chemistry programs do. A caller builds an
//! [`MpsResumer`] over an engine and runs a program's shots on it, as on
//! any [`QirResumer`]; it may rely on each shot recording what it would
//! record alone (to what precision: [`MpsResumer`]), and on a report of
//! what truncation cost.
//!
//! [`qir`](crate::qir) runs a program's classical part and hands each shot's
//! quantum calls to a resumer; an engine holds states on a chain of sites,
//! applies matrices to them and answers probabilities. This module joins
//! them; its parts, and the file of each:
//!
//! ```text
//! MpsResumer           runs a program's shots       (this file)
//! └─ GroupedRun        groups, splits and failures  grouped_run.rs
//!    ├─ QubitOrder     the site of each qubit       qubit_order.rs
//!    ├─ lower          each call as steps on sites  lowering.rs
//!    ├─ shot_stream    each shot's uniforms         shot_stream.rs
//!    └─ MpsEngine      states on a chain of sites   (tensornet)
//! MpsRunReport         what a run cost              mps_run_report.rs
//! MpsResumerError      why a shot failed            mps_resumer_error.rs
//! ```
//!
//! Shots that drew the same values share one state, forked where their draws
//! differ, so a program whose draws never differ holds at most one; as the
//! smaller group goes on where they differ, a run of N shots holds at most
//! ⌊log₂ N⌋ + 1 states at once.
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

mod grouped_run;
mod lowering;
mod mps_resumer_error;
mod mps_run_report;
// Reached by their own tests too.
pub(crate) mod qubit_order;
pub(crate) mod shot_stream;

pub use mps_resumer_error::MpsResumerError;
pub use mps_run_report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};

use grouped_run::GroupedRun;

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`, shots that drew the same values sharing one state (how: the header
/// of `mps_simulator/grouped_run.rs`).
///
/// Each shot draws from its own stream, seeded from the resumer's seed and
/// the shot's id (the rule is in `mps_simulator/shot_stream.rs`). So, on an
/// engine whose fork is an exact copy and whose operations are
/// deterministic (`tensornet`'s exact reference, for instance), a shot's
/// records depend only on the program, the seed, the shot's id and the
/// settings: not on the shots beside it, nor on its place among them, nor
/// on earlier runs. On another engine, whose fork reads what its state
/// reads only to rounding (all [`MpsEngine::fork`] promises), each
/// probability a shot draws against agrees to rounding with the one it
/// would draw against alone: a record then differs only where a uniform
/// falls within rounding of a probability, and the shot's later draws may
/// then differ.
///
/// The qubit order (`mps_simulator/qubit_order.rs`) changes the bonds and
/// the time, and, on an engine that truncates, the state.
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

/// The default max bond dimension χ. H₂'s iterative phase
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
    /// interrupts a run of hours (Ctrl-C from Python, say) gets it back once
    /// the batch under way ends, its states freed. The flag is read before a
    /// group runs its steps (when it first runs, when a group split off by a
    /// draw runs on, and once each batch is lowered), and after each resume
    /// that gives a group its next batch or a trap, before the batch is lowered
    /// or the trap reported. So a flag set while a group runs its batch lets
    /// that group run the rest of the batch, and stops each group split off by
    /// a draw before it runs on. A Base Profile program that measures into
    /// each result once and records its results after its last measurement
    /// is one batch ([where a batch ends](QirMachine)): the group under way
    /// runs to its end, and the groups split off do not run on. Once read
    /// set, the flag fails the run with [`MpsResumerError::Stopped`], naming
    /// the first shot that had not ended, whatever the batch after the read
    /// would have done (trapped, or been refused); a run whose every shot has
    /// ended returns its records, and a run of no shots returns none. Whoever
    /// sets it clears it before the next run.
    #[must_use]
    pub fn with_stop_flag(mut self, stop_flag: Arc<AtomicBool>) -> Self {
        self.stop_flag = Some(stop_flag);
        self
    }

    /// What the last run cost: the largest bond any of its states held
    /// where a sequence of gates ended, and the least weight kept by a shot
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
    /// failure, or the stop flag, once read set, naming the first shot that
    /// had not ended ([`MpsResumer::with_stop_flag`]). A shot's failure,
    /// unlike its records, may depend on the shots beside it: a fork that
    /// fails fails shots that alone would not have forked
    /// (`mps_simulator/design.md`, "How a run fails").
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
