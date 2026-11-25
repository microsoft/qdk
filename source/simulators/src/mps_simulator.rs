// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulation method: runs QIR programs on matrix product states
//! (MPS), on any [`MpsEngine`], the `tensornet` contract.
//!
//! An MPS simulates many more qubits than a state vector when a program
//! entangles them little, as chemistry programs do. A caller builds an
//! [`MpsResumer`] over an engine and runs a program's shots on it, as on
//! any [`QirResumer`]. Each shot records what it would record alone
//! ([`MpsResumer`] says to what precision). A report says what truncation
//! cost.
//!
//! [`qir`](crate::qir) runs a program's classical part and hands each
//! shot's quantum calls to a resumer. An engine holds states on a chain of
//! sites, applies matrices to them and answers probabilities. This module
//! joins them. Its parts, and the file of each:
//!
//! ```text
//! MpsResumer         runs a program's shots       (this file)
//! └─ GroupedRun      groups, splits and failures  grouped_run.rs
//!    ├─ ShotGroup    shots sharing one state      grouped_run/shot_group.rs
//!    │  └─ shot_rng  each shot's uniforms         shot_rng.rs
//!    ├─ FirstFailure the first failing shot       grouped_run/first_failure.rs
//!    ├─ apply, draw  each step on a group's state grouped_run/site_steps.rs
//!    ├─ QubitOrder   the site of each qubit       qubit_order.rs
//!    ├─ lower        each call as steps on sites  lowering.rs
//!    └─ MpsEngine    states on a chain of sites   (tensornet)
//! MpsRunReport       what a run cost              mps_run_report.rs
//! MpsResumerError    why a shot failed            mps_resumer_error.rs
//! ```
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ.
//!
//! `src/mps_simulator/README.md` gives an overview, the public types and an
//! example. `src/mps_simulator/design.md` gives each phase of a run, its
//! mechanics and its engine calls.

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
mod qubit_order;
mod shot_rng;

pub use mps_resumer_error::MpsResumerError;
pub use mps_run_report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};

use grouped_run::GroupedRun;

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`. Shots that drew the same values share one state
/// (`src/mps_simulator/design.md` gives how, phase by phase).
///
/// Each shot draws its uniforms from its own random number generator
/// (RNG), fixed by the resumer's seed and the shot's id alone. The RNG is
/// `tensornet`'s [`SplitMix64`](tensornet::sampling::SplitMix64), so a seed
/// gives the same draws on every platform.
///
/// On an exact engine, a shot's records depend only on the program, the
/// seed, the shot's id and the settings. They do not depend on the shots
/// beside it, its place among them, or earlier runs. An exact engine forks
/// an exact copy and runs deterministic operations, as `tensornet`'s exact
/// reference does.
///
/// Up to rounding, the same holds on any engine. [`MpsEngine::fork`]
/// promises a fork that reads what its state reads to rounding. So each
/// probability a shot draws against agrees to rounding with the one it
/// would draw against alone. A record differs only where a uniform falls
/// within rounding of a probability; the shot's later draws may then
/// differ.
///
/// The qubit order places qubits that share two-qubit gates on nearby
/// sites. It changes the bonds and the time and, on an engine that
/// truncates, the state.
///
/// The engine contract takes only a capacity, here
/// [`MpsShape::qubit_chain`](tensornet::MpsShape::qubit_chain) of the
/// program's qubits and χ. The engine's own settings, such as its cutoff,
/// are the engine's.
pub struct MpsResumer<E: MpsEngine> {
    /// The engine that holds the states of each run.
    engine: E,
    /// The seed each shot's RNG derives from, with the shot's id.
    seed: u64,
    /// The max bond dimension χ each state is prepared within.
    max_bond_dimension: NonZeroUsize,
    /// The flag that stops a run once set; `None` until
    /// [`with_stop_flag`](Self::with_stop_flag) gives one.
    stop_flag: Option<Arc<AtomicBool>>,
    /// The report [`last_run`](Self::last_run) gives.
    last_run: MpsRunReport,
}

/// The default max bond dimension χ. H₂'s iterative phase estimation (29
/// qubits), run on a CPU engine with this default, kept every bond it
/// needed within it: its largest was 24.
pub const DEFAULT_MAX_BOND_DIMENSION: NonZeroUsize =
    NonZeroUsize::new(128).expect("128 is not zero");

impl<E: MpsEngine> MpsResumer<E> {
    /// The resumer running shots on `engine`, each drawing from an RNG seeded
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
    /// `max_bond_dimension` (χ). An engine that truncates keeps its chain
    /// within it, losing the weight its report gives.
    ///
    /// Set it before the runs it applies to. The
    /// [`last_run`](Self::last_run) it leaves is that of a run that
    /// prepared no state at the new χ.
    #[must_use]
    pub fn with_max_bond_dimension(mut self, max_bond_dimension: NonZeroUsize) -> Self {
        self.max_bond_dimension = max_bond_dimension;
        self.last_run = MpsRunReport::new(max_bond_dimension.get());
        self
    }

    /// The resumer stopping its runs once `stop_flag` is set. A user who
    /// interrupts a run of hours (Ctrl-C from Python, say) gets it back
    /// once the batch under way ends, its states freed.
    ///
    /// The flag is read before a group runs each batch, or the rest of the
    /// batch it split off in. It is also read before a trap or a refused
    /// call is reported.
    ///
    /// So a flag set while a group runs its batch lets that group run the
    /// rest of the batch. It stops each group split off by a draw before it
    /// runs on. A Base Profile program that measures into each result once,
    /// and records its results after its last measurement, is one batch
    /// ([where a batch ends](QirMachine)). The group under way runs to its
    /// end, and the groups split off do not run on.
    ///
    /// Once read set, the flag fails the run with
    /// [`MpsResumerError::Stopped`], naming the first shot that had not
    /// ended. It does so whatever the batch after the read would have done
    /// (trapped, or been refused). A run whose every shot has ended returns
    /// its records, and a run of no shots returns none. Whoever sets the
    /// flag clears it before the next run.
    #[must_use]
    pub fn with_stop_flag(mut self, stop_flag: Arc<AtomicBool>) -> Self {
        self.stop_flag = Some(stop_flag);
        self
    }

    /// What the last run cost: the largest bond dimension any of its states
    /// held where a sequence of gates ended, and the least weight kept by a
    /// shot that ended, with the warning they call for
    /// ([`MpsRunReport::truncation_warning`]).
    ///
    /// After a failed run, it holds what the run read until it stopped.
    /// Groups holding a shot before the failing one run on after the
    /// failure, and what they read counts too.
    ///
    /// Before any run, after a run of no shots, and once the max bond
    /// dimension is set
    /// ([`with_max_bond_dimension`](Self::with_max_bond_dimension)), it is
    /// that of a run that prepared no state at the resumer's χ.
    #[must_use]
    pub fn last_run(&self) -> &MpsRunReport {
        &self.last_run
    }
}

impl<E: MpsEngine> QirResumer for MpsResumer<E> {
    type Error = MpsResumerError<E::Error>;

    /// Runs `shots` as [`QirResumer::run`] requires. It never waits: it
    /// runs them when called, and its future is ready at once.
    ///
    /// # Errors
    ///
    /// The first failing shot in the order of `shots`: its context trapped,
    /// or the resumer failed it ([`MpsResumerError`]). The resumer fails a
    /// shot for a noise or loss call, a gate on one qubit twice, a capacity
    /// that is no chain, the engine's refusal or failure, or the stop flag
    /// once read set ([`MpsResumer::with_stop_flag`]).
    ///
    /// Unlike its records, a shot's failure may depend on the shots beside
    /// it. A fork that fails fails shots that alone would not have forked
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

#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;
