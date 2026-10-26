// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! State queries: values read from ψ, the state a Base program reaches
//! before its terminal measurements, outside the shot protocol (Q9).

use num_complex::Complex64;
use qdk_simulators::{
    QubitID,
    execution::{Pauli as ProgramPauli, PauliSum, PreparedAdaptiveProgram},
};
use tensornet::{MpsEngine, MpsShape, MpsShapeError, Pauli, PauliTerm};

use crate::{
    QubitOrder,
    capacity::mps_capacity,
    lowering,
    program::{ProgramError, plan},
    terminal::outgrown,
};

/// One query on ψ, as M0's ④'s.
#[derive(Clone, Debug, PartialEq)]
pub enum StateQuery {
    /// ⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩.
    Expectation(PauliSum),
    /// The resources of the one MPS the call computed.
    Cost,
}

/// The answer to one [`StateQuery`], in the same position.
#[derive(Clone, Debug, PartialEq)]
pub enum StateQueryValue {
    /// ⟨O⟩: real up to rounding when every coefficient is real.
    Expectation(Complex64),
    /// The resources of the MPS.
    Cost(MpsCost),
}

/// The resources of one MPS evaluation: M0's ④'s, without its device
/// workspace, which [`MpsEngine`] does not report (see the decisions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpsCost {
    /// The largest bond ψ's chain reached, at most χ for an engine that
    /// truncates.
    pub max_bond_dimension: usize,
    /// The bytes of ψ's site tensors, at 16 bytes an element (complex f64).
    pub state_bytes: usize,
}

/// Why [`Evaluate::evaluate`] gave no values. Its own type, not
/// [`Rejected`](executor_protocol::Rejected), because a query is not a call
/// of the shot protocol.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum QueryError {
    /// No query: there is nothing to prepare ψ for.
    #[error("queries must contain at least one query")]
    NoQueries,
    /// The program cannot be run by the batch strategy, whose ψ the queries
    /// read ([`ProgramError`]).
    #[error(transparent)]
    Program(#[from] ProgramError),
    /// An expectation on a qubit the program does not have.
    #[error("Expectation acts on qubit {qubit}, but the program has {qubits} qubits")]
    Qubit {
        /// The qubit.
        qubit: QubitID,
        /// The program's qubits.
        qubits: usize,
    },
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
    /// A program with more qubits than the state the evaluator prepares.
    #[error("the program has {program} qubits, but the MPS has {sites} sites")]
    Sites {
        /// The program's qubits.
        program: usize,
        /// The capacity's sites.
        sites: usize,
    },
    /// An observable the engine does not take. Not reachable from a
    /// [`PauliSum`], whose terms are already checked as the engine checks
    /// them.
    #[error("query {at}: {reason}")]
    Observable {
        /// The query's position.
        at: usize,
        /// Why.
        reason: String,
    },
    /// The engine cannot hold ψ; nothing was computed.
    #[error("the engine cannot prepare the state: {0}")]
    Unavailable(String),
    /// The engine failed while ψ was computed or read.
    #[error("the state could not be evaluated: {0}")]
    Failed(String),
}

impl QueryError {
    /// Whether the error is the environment's (the engine's), not the
    /// program's or the queries': as M0's `is_environment_error`.
    #[must_use]
    pub const fn is_environment_error(&self) -> bool {
        matches!(self, Self::Unavailable(_) | Self::Failed(_))
    }
}

/// Values read from ψ, the state `program` reaches before its terminal
/// measurements (Q9 (A): next to the shot strategies, outside the shot
/// protocol). The program is one that the batch strategy samples, so the
/// program a caller samples is the one it queries, as in M0. The values come
/// back in the order of `queries`.
pub trait Evaluate {
    /// ψ for `program`, then one value per query.
    ///
    /// # Errors
    ///
    /// Before any state is prepared: [`QueryError::NoQueries`], then the
    /// program's error ([`QueryError::Program`]), then
    /// [`QueryError::Qubit`], [`QueryError::Capacity`] and
    /// [`QueryError::Sites`]. Then
    /// [`QueryError::Unavailable`] when the engine cannot prepare ψ, and
    /// [`QueryError::Failed`] when it fails computing or reading it.
    fn evaluate(
        &self,
        program: &PreparedAdaptiveProgram<u64>,
        queries: &[StateQuery],
    ) -> Result<Vec<StateQueryValue>, QueryError>;
}

/// The capacity ψ is prepared within: an evaluator's own, or the one for the
/// program's qubits under χ.
#[derive(Clone, Copy)]
pub(crate) enum Within<'a> {
    Capacity(&'a MpsShape),
    MaxBond(usize),
}

/// [`Evaluate::evaluate`] on `engine`, with ψ prepared `within`, each qubit
/// on its site by `order` (qubit `q` on site `q` without one). A ψ beyond
/// the capacity is [`QueryError::Failed`], since a value read from it may
/// not be the method's.
pub(crate) fn evaluate_on<E: MpsEngine>(
    engine: &E,
    within: Within<'_>,
    order: Option<&QubitOrder>,
    program: &PreparedAdaptiveProgram<u64>,
    queries: &[StateQuery],
) -> Result<Vec<StateQueryValue>, QueryError> {
    let site_of = |qubit: QubitID| order.map_or(qubit, |order| order.site(qubit));
    if queries.is_empty() {
        return Err(QueryError::NoQueries);
    }
    let plan = plan(program)?;
    let observables = queries
        .iter()
        .enumerate()
        .filter_map(|(at, query)| match query {
            StateQuery::Expectation(observable) => Some((at, observable)),
            StateQuery::Cost => None,
        })
        .map(|(at, observable)| {
            if let Some(qubit) = observable.max_qubit().filter(|&qubit| qubit >= plan.qubits) {
                return Err(QueryError::Qubit {
                    qubit,
                    qubits: plan.qubits,
                });
            }
            to_engine(observable, site_of).map_err(|reason| QueryError::Observable { at, reason })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sized;
    let capacity = match within {
        Within::Capacity(capacity) => capacity,
        Within::MaxBond(max_bond) => {
            sized = mps_capacity(plan.qubits, max_bond).map_err(|source| QueryError::Capacity {
                qubits: plan.qubits,
                max_bond,
                source,
            })?;
            &sized
        }
    };
    if plan.qubits > capacity.site_count() {
        return Err(QueryError::Sites {
            program: plan.qubits,
            sites: capacity.site_count(),
        });
    }
    let failed = |error: E::Error| QueryError::Failed(error.to_string());
    let mut psi = engine
        .prepare(capacity)
        .map_err(|error| QueryError::Unavailable(error.to_string()))?;
    for operation in plan.operations() {
        lowering::apply_placed(engine, &mut psi, operation, site_of).map_err(failed)?;
    }
    outgrown(engine, &psi, capacity).map_err(QueryError::Failed)?;
    let shape = engine.shape(&psi).map_err(failed)?;
    let cost = MpsCost {
        max_bond_dimension: shape.max_bond(),
        state_bytes: shape
            .element_counts()
            .iter()
            .sum::<usize>()
            .saturating_mul(16),
    };
    let mut observables = observables.into_iter();
    queries
        .iter()
        .map(|query| match query {
            StateQuery::Expectation(_) => match observables.next() {
                Some(observable) => engine
                    .expectation(&psi, &observable)
                    .map(StateQueryValue::Expectation)
                    .map_err(failed),
                None => Err(QueryError::Failed(
                    "unreachable: one observable per Expectation query".to_owned(),
                )),
            },
            StateQuery::Cost => Ok(StateQueryValue::Cost(cost)),
        })
        .collect()
}

/// `observable` as the engine takes it: the same terms, each factor on its
/// qubit's site by `site_of`, with the identity's coefficient apart.
fn to_engine(
    observable: &PauliSum,
    site_of: impl Fn(QubitID) -> usize,
) -> Result<tensornet::PauliSum, String> {
    let terms = observable
        .terms()
        .iter()
        .map(|term| {
            let factors = term
                .factors()
                .iter()
                .map(|&(qubit, pauli)| {
                    let pauli = match pauli {
                        ProgramPauli::X => Pauli::X,
                        ProgramPauli::Y => Pauli::Y,
                        ProgramPauli::Z => Pauli::Z,
                    };
                    (site_of(qubit), pauli)
                })
                .collect();
            PauliTerm::new(term.coefficient(), factors)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    tensornet::PauliSum::new(observable.identity_coefficient(), terms)
        .map_err(|error| error.to_string())
}
