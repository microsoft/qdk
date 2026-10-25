// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! M1's stack of the MPS method, for ⑤′: `Sample` and state queries on a
//! prepared Base program, as M0's `run_mps_shots` and
//! `evaluate_state_queries`.

use std::rc::Rc;

use executor_protocol::{
    Close as _, Executor as _, Needs, PerShot, Rejected, ShotSet, ShotStrategy as _, SubmissionRef,
    Value, wait,
};
use qdk_simulators::{OutputRecord, execution::PreparedAdaptiveProgram};
use rand::RngExt as _;
use tensornet::{MpsEngine, MpsShapeError};

use crate::{
    batch::MpsBatchSimulator,
    program::{ProgramError, plan, replay},
    queries::{Evaluate, QueryError, StateQuery, StateQueryValue, Within, evaluate_on},
    terminal::TensorNetShots,
};

/// χ when the caller gives none: the bond cap of M0's base qualification
/// policy, which M0's `StateMethod::Mps { max_bond_dimension: None }` and
/// `run_mps_shots(…, None)` used.
pub const DEFAULT_MAX_BOND: usize = 128;

/// M1's stack of the MPS method over an engine: the batch strategy for
/// `Sample`, and [`Evaluate`] for state queries, each sized to the program
/// it is given.
///
/// ```text
/// sample(program, N, seed)
///   preflight (M0's): one region, no feedforward, no reset ── ProgramError
///   MpsBatchSimulator::open(seed, 1..=N)   one state, prepared by the engine
///     submit [region, MeasureZ per measured qubit]   one sample of N shots
///   per shot: RuntimeAdapter replays the walk, answering each measurement
///             from its qubit's column of the shot's row ── output records
/// ```
///
/// The per-shot strategy over the same engine ([`MpsStack::per_shot`]) is
/// the one `mps-a` will run Adaptive programs with.
pub struct MpsStack<E: MpsEngine> {
    engine: Rc<E>,
    max_bond: usize,
}

impl<E: MpsEngine> MpsStack<E> {
    /// The stack over `engine`, with bonds capped at [`DEFAULT_MAX_BOND`].
    #[must_use]
    pub fn new(engine: E) -> Self {
        Self::shared(Rc::new(engine))
    }

    /// The stack over a shared `engine`.
    #[must_use]
    pub fn shared(engine: Rc<E>) -> Self {
        Self {
            engine,
            max_bond: DEFAULT_MAX_BOND,
        }
    }

    /// The stack with bonds capped at χ = `max_bond_dimension`, or at
    /// [`DEFAULT_MAX_BOND`] for `None`, as M0's `max_bond_dimension` option.
    #[must_use]
    pub fn with_max_bond_dimension(mut self, max_bond_dimension: Option<u32>) -> Self {
        self.max_bond = max_bond_dimension.map_or(DEFAULT_MAX_BOND, |chi| {
            usize::try_from(chi).unwrap_or(usize::MAX)
        });
        self
    }

    /// χ, the bond cap of every state it prepares.
    #[must_use]
    pub fn max_bond(&self) -> usize {
        self.max_bond
    }

    /// The engine.
    #[must_use]
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// The batch strategy for `qubits` qubits under this stack's χ.
    ///
    /// # Errors
    ///
    /// [`mps_capacity`](crate::mps_capacity)'s, when that is not a chain.
    pub fn batch(&self, qubits: usize) -> Result<MpsBatchSimulator<E>, MpsShapeError> {
        MpsBatchSimulator::new(Rc::clone(&self.engine), qubits, self.max_bond)
    }

    /// The per-shot strategy for `qubits` qubits under this stack's χ.
    ///
    /// # Errors
    ///
    /// [`mps_capacity`](crate::mps_capacity)'s, when that is not a chain.
    pub fn per_shot(&self, qubits: usize) -> Result<PerShot<TensorNetShots<E>>, MpsShapeError> {
        TensorNetShots::new(Rc::clone(&self.engine), qubits, self.max_bond).map(PerShot)
    }

    /// `shots` shots of `program`, each its output records, as M0's
    /// `run_mps_shots`: the same program, χ and seed give M0's sample.
    /// Without a seed, one is drawn from the thread's generator.
    ///
    /// # Errors
    ///
    /// Before any state is prepared: [`StackError::Program`], then
    /// [`StackError::Capacity`], then [`StackError::Rejected`] (no shots).
    /// Then [`StackError::Rejected`] with [`Rejected::Unavailable`] when the
    /// engine cannot prepare the state, and [`StackError::Failed`] when it
    /// fails. A shot's replay can still find feedforward on a path the
    /// preflight's all-Zero walk did not take ([`StackError::Program`]), as
    /// M0's did.
    pub fn sample(
        &self,
        program: &PreparedAdaptiveProgram<u64>,
        shots: u32,
        seed: Option<u32>,
    ) -> Result<Vec<Vec<OutputRecord>>, StackError> {
        let plan = plan(program)?;
        let simulator = MpsBatchSimulator::new(Rc::clone(&self.engine), plan.qubits, self.max_bond)
            .map_err(|source| StackError::Capacity {
                qubits: plan.qubits,
                max_bond: self.max_bond,
                source,
            })?;
        let set = ShotSet::range(1..=shots)?;
        let seed = seed.map_or_else(|| rand::rng().random::<u64>(), u64::from);
        let mut session = simulator.open(seed, &set, &Needs::default())?;
        let batch = plan.batch();
        let reply = session.call(SubmissionRef {
            shots: &set,
            batch: &batch,
        });
        let mut events = Vec::new();
        let run = reply.map(|reply| wait(reply, &mut events));
        session.close();
        run?.map_err(StackError::Failed)?;
        let columns = plan.columns.len();
        let mut rows = vec![vec![false; columns]; set.len()];
        for event in events {
            let row = usize::try_from(event.shot)
                .ok()
                .and_then(|shot| rows.get_mut(shot.wrapping_sub(1)));
            let cell = row.and_then(|row| {
                usize::try_from(event.tag)
                    .ok()
                    .and_then(|tag| row.get_mut(tag))
            });
            match (cell, event.value) {
                (Some(cell), Value::One) => *cell = true,
                (Some(_), Value::Zero) => {}
                _ => {
                    return Err(StackError::Failed(format!(
                        "unreachable: the batch session reported {:?} for shot {}, tag {}",
                        event.value, event.shot, event.tag
                    )));
                }
            }
        }
        rows.iter()
            .map(|row| replay(program, &plan, row).map_err(StackError::Program))
            .collect()
    }
}

/// The batch strategy's ψ for the program, under this stack's χ, as M0's
/// `evaluate_state_queries` with `StateMethod::Mps`.
impl<E: MpsEngine> Evaluate for MpsStack<E> {
    fn evaluate(
        &self,
        program: &PreparedAdaptiveProgram<u64>,
        queries: &[StateQuery],
    ) -> Result<Vec<StateQueryValue>, QueryError> {
        evaluate_on(
            &*self.engine,
            Within::MaxBond(self.max_bond),
            program,
            queries,
        )
    }
}

/// ψ within the strategy's capacity: queries do not sample, so the batch
/// strategy changes nothing for them.
impl<E: MpsEngine> Evaluate for MpsBatchSimulator<E> {
    fn evaluate(
        &self,
        program: &PreparedAdaptiveProgram<u64>,
        queries: &[StateQuery],
    ) -> Result<Vec<StateQueryValue>, QueryError> {
        evaluate_on(
            &**self.engine(),
            Within::Capacity(self.capacity()),
            program,
            queries,
        )
    }
}

/// ψ within a shot's capacity: the same as the batch strategy's.
impl<E: MpsEngine> Evaluate for TensorNetShots<E> {
    fn evaluate(
        &self,
        program: &PreparedAdaptiveProgram<u64>,
        queries: &[StateQuery],
    ) -> Result<Vec<StateQueryValue>, QueryError> {
        evaluate_on(
            &**self.engine(),
            Within::Capacity(self.capacity()),
            program,
            queries,
        )
    }
}

/// Why [`MpsStack::sample`] gave no shots.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StackError {
    /// The program cannot be run by the batch strategy.
    #[error(transparent)]
    Program(#[from] ProgramError),
    /// No MPS for the program's qubits under χ (no qubits, or χ = 0).
    #[error("no MPS of {qubits} qubits with bonds capped at {max_bond}: {source}")]
    Capacity {
        /// The program's qubits.
        qubits: usize,
        /// χ.
        max_bond: usize,
        /// Why.
        source: MpsShapeError,
    },
    /// The batch strategy refused the session or its submission: no shots,
    /// or an engine that cannot prepare the state
    /// ([`Rejected::Unavailable`]).
    #[error(transparent)]
    Rejected(#[from] Rejected),
    /// The engine failed during the run.
    #[error("MPS batch sampling failed: {0}")]
    Failed(String),
}

impl StackError {
    /// Whether the error is the environment's (the engine's), not the
    /// program's: as M0's `is_environment_error`.
    #[must_use]
    pub fn is_environment_error(&self) -> bool {
        matches!(
            self,
            Self::Failed(_) | Self::Rejected(Rejected::Unavailable(_))
        )
    }
}
