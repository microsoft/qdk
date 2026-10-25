// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program conditioned on given measurement outcomes, as one circuit.
//!
//! Fixing every outcome turns an adaptive program into a linear sequence of
//! unitaries and rank-one projections. A backend that evaluates the circuit
//! without sampling gives the probability of those outcomes, a reference to
//! check samplers against; an MPS applies each projection as a non-unitary
//! operator.

use std::fmt;

use executor_protocol::{Batch, Executor, Immediate, Outcome, Rejected};

use crate::{MeasurementResult, QubitID};

use crate::execution::{
    AdaptiveExecutionError, MeasurementKind, MeasurementRequest, PreparedAdaptiveProgram,
    QuantumEvolutionRegion, RegionInstruction, RuntimeAdapter, RuntimeError, UnitaryOperation,
};

/// One operation on the path selected by fixed measurement outcomes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConditionedOperation {
    Unitary(UnitaryOperation),
    /// Projects `qubit` onto the basis state of the fixed `outcome`.
    ///
    /// The projector |b⟩⟨b| has rank one, so afterwards the qubit is exactly
    /// |b⟩, or |0⟩ when `reset` is set. `reset` covers measure-and-reset and a
    /// reset that follows this measurement with no operation on the qubit in
    /// between; any other reset has no form in a conditioned circuit.
    Measure {
        qubit: QubitID,
        result_id: usize,
        outcome: MeasurementResult,
        reset: bool,
    },
}

/// The operations of the path selected by fixed outcomes, starting from |0…0⟩.
///
/// Measurement operands and outcomes are validated here. Unitary operands are
/// validated by each backend against the operations it supports, as for
/// [`crate::execution::QuantumEvolutionRegion`].
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionedCircuit {
    qubit_count: usize,
    operations: Vec<ConditionedOperation>,
}

impl ConditionedCircuit {
    pub fn new(
        qubit_count: usize,
        operations: Vec<ConditionedOperation>,
    ) -> Result<Self, ConditionedCircuitError> {
        let mut result_ids = rustc_hash::FxHashSet::default();
        for (operation_index, operation) in operations.iter().enumerate() {
            if let ConditionedOperation::Measure {
                qubit,
                result_id,
                outcome,
                ..
            } = *operation
            {
                if qubit >= qubit_count {
                    return Err(ConditionedCircuitError::QubitOutOfRange {
                        operation_index,
                        qubit,
                        qubit_count,
                    });
                }
                // Loss is a classical event with state-dependent handling, not
                // a projection, so it has no single operator in a conditioned
                // circuit.
                if outcome == MeasurementResult::Loss {
                    return Err(ConditionedCircuitError::LossOutcome { operation_index });
                }
                // Each result has one fixed outcome, so a result id names one
                // projection.
                if !result_ids.insert(result_id) {
                    return Err(ConditionedCircuitError::DuplicateResult {
                        operation_index,
                        result_id,
                    });
                }
            }
        }
        Ok(Self {
            qubit_count,
            operations,
        })
    }

    #[must_use]
    pub fn qubit_count(&self) -> usize {
        self.qubit_count
    }

    #[must_use]
    pub fn operations(&self) -> &[ConditionedOperation] {
        &self.operations
    }

    /// Returns this circuit with the projection of `result_id` onto `outcome`
    /// (`true` = |1⟩) and every other operation unchanged.
    ///
    /// The operations after the projection stay those of the original path,
    /// so this is not the path the program would take for the new record.
    /// That makes it a check on evaluation alone: a backend evaluating the
    /// circuit must observe the changed projection, for example a zero
    /// probability when the original outcome was certain.
    pub fn with_outcome(
        &self,
        result_id: usize,
        outcome: bool,
    ) -> Result<Self, ConditionedCircuitError> {
        let mut operations = self.operations.clone();
        let fixed = operations
            .iter_mut()
            .find_map(|operation| match operation {
                ConditionedOperation::Measure {
                    result_id: id,
                    outcome,
                    ..
                } if *id == result_id => Some(outcome),
                _ => None,
            })
            .ok_or(ConditionedCircuitError::UnknownResult { result_id })?;
        *fixed = basis_outcome(outcome);
        Ok(Self {
            qubit_count: self.qubit_count,
            operations,
        })
    }

    /// Runs `prepared` once, answering every measurement of QIR result `i`
    /// with `outcomes[i]`, and returns the path taken.
    ///
    /// Branches follow the fixed outcomes, so the result is one linear circuit
    /// whatever the number of regions. `outcomes` must hold one value per
    /// program result (`true` = |1⟩). Output records are not part of the
    /// circuit.
    ///
    /// A reset folds into the circuit only when the qubit is known to be in a
    /// basis state: untouched since allocation or its last reset (dropped), or
    /// measured with no operation since (the measurement gains `reset`). A
    /// reset after a gate is the channel `ρ ↦ |0⟩⟨0| ⊗ Tr_q ρ`, which sums over
    /// the qubit's unobserved value and so is not one operator on the
    /// amplitude; it is rejected with [`ConditioningError::ResetOfLiveQubit`].
    ///
    /// A record that fails a selection check, such as a `REQUIRE` in a Stim
    /// `SELECT` block, makes the program restart the block and measure a
    /// result again; see [`ConditioningError::ResultMeasuredAgain`] for an
    /// example. The restart usually resets qubits that are still in use before
    /// that second measurement. Those resets are a consequence of the failed
    /// check, not its cause, so [`ConditioningError::ResetOfLiveQubit`] is
    /// reported only if the program completes, and the failed check is
    /// reported as [`ConditioningError::ResultMeasuredAgain`].
    // TODO(selection-normalization): the circuit gives the probability of one
    // pass through the program. When a selection check can fail (noise, or a
    // non-deterministic REQUIRE), outputs are that probability divided by the
    // acceptance probability, which is not computed yet. See the example in
    // the execution README ("conditioned circuit gives the probability of one
    // pass").
    pub fn from_prepared_program(
        prepared: &PreparedAdaptiveProgram<u64>,
        outcomes: &[bool],
    ) -> Result<Self, ConditioningError> {
        let program = prepared.program();
        let result_count = usize::try_from(program.num_results)
            .expect("adaptive result count should fit in usize");
        if outcomes.len() != result_count {
            return Err(ConditioningError::OutcomeCountMismatch {
                expected: result_count,
                actual: outcomes.len(),
            });
        }
        let qubit_count =
            usize::try_from(program.num_qubits).expect("adaptive qubit count should fit in usize");
        let mut runtime = RuntimeAdapter::new(ConditionedTerminal::new(qubit_count, outcomes));
        let ran = runtime.run(prepared, &mut Vec::new());
        let terminal = runtime.into_inner();
        match ran {
            Ok(()) => terminal.finish()?,
            Err(RuntimeError::Control(control)) => return Err(ConditioningError::Control(control)),
            // The terminal fails only with an error it keeps, and never
            // rejects or leaves out an outcome, so the walk ended on that
            // error; the missing response is what remains.
            Err(_) => {
                return Err(terminal.error.unwrap_or(ConditioningError::Control(
                    AdaptiveExecutionError::MissingResponse,
                )));
            }
        }
        Self::new(qubit_count, terminal.operations).map_err(ConditioningError::Circuit)
    }
}

fn basis_outcome(one: bool) -> MeasurementResult {
    if one {
        MeasurementResult::One
    } else {
        MeasurementResult::Zero
    }
}

/// What `ConditionedTerminal` knows about one qubit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QubitState {
    /// Exactly |0⟩: untouched since allocation or since a folded reset.
    Fresh,
    /// A gate acted on the qubit since it was last in a known basis state.
    Live,
    /// Measured with no operation since; holds the index of that `Measure`.
    JustMeasured(usize),
}

struct ConditionedTerminal<'outcomes> {
    outcomes: &'outcomes [bool],
    measured_results: Vec<bool>,
    qubits: Vec<QubitState>,
    operations: Vec<ConditionedOperation>,
    /// The first live reset, reported only if the program completes.
    live_reset: Option<QubitID>,
    /// The first error, which ends the run.
    error: Option<ConditioningError>,
}

impl<'outcomes> ConditionedTerminal<'outcomes> {
    fn new(qubit_count: usize, outcomes: &'outcomes [bool]) -> Self {
        Self {
            outcomes,
            measured_results: vec![false; outcomes.len()],
            qubits: vec![QubitState::Fresh; qubit_count],
            operations: Vec::new(),
            live_reset: None,
            error: None,
        }
    }

    fn qubit_state(&mut self, qubit: QubitID) -> Result<&mut QubitState, ConditioningError> {
        let qubit_count = self.qubits.len();
        self.qubits
            .get_mut(qubit)
            .ok_or(ConditioningError::QubitOutOfRange { qubit, qubit_count })
    }
}

impl ConditionedTerminal<'_> {
    fn execute_region(&mut self, region: &QuantumEvolutionRegion) -> Result<(), ConditioningError> {
        for &operation in region.operations() {
            let (first, second) = operation.qubits();
            // The identity leaves a basis state unchanged, so it does not make
            // the qubit live.
            let is_identity = matches!(operation, UnitaryOperation::I { .. });
            for qubit in std::iter::once(first).chain(second) {
                let state = self.qubit_state(qubit)?;
                if !is_identity {
                    *state = QubitState::Live;
                }
            }
            self.operations
                .push(ConditionedOperation::Unitary(operation));
        }
        Ok(())
    }

    fn measure(
        &mut self,
        request: MeasurementRequest,
    ) -> Result<MeasurementResult, ConditioningError> {
        self.qubit_state(request.qubit)?;
        let result_count = self.outcomes.len();
        let measured = self.measured_results.get_mut(request.result_id).ok_or(
            ConditioningError::ResultOutOfRange {
                result_id: request.result_id,
                result_count,
            },
        )?;
        if *measured {
            return Err(ConditioningError::ResultMeasuredAgain {
                result_id: request.result_id,
            });
        }
        *measured = true;
        let outcome = basis_outcome(self.outcomes[request.result_id]);
        let index = self.operations.len();
        self.operations.push(ConditionedOperation::Measure {
            qubit: request.qubit,
            result_id: request.result_id,
            outcome,
            reset: request.kind == MeasurementKind::MeasureResetZ,
        });
        *self.qubit_state(request.qubit)? = QubitState::JustMeasured(index);
        Ok(outcome)
    }

    fn reset(&mut self, qubit: QubitID) -> Result<(), ConditioningError> {
        let state = self.qubit_state(qubit)?;
        match *state {
            QubitState::Fresh => {}
            QubitState::JustMeasured(index) => {
                *state = QubitState::Fresh;
                if let ConditionedOperation::Measure { reset, .. } = &mut self.operations[index] {
                    *reset = true;
                }
            }
            // The circuit is already invalid, but driving on lets a record
            // that fails a selection check surface as `ResultMeasuredAgain`
            // when the restarted block measures its result again.
            QubitState::Live => {
                *state = QubitState::Fresh;
                self.live_reset.get_or_insert(qubit);
            }
        }
        Ok(())
    }

    fn run(
        &mut self,
        instruction: &RegionInstruction,
    ) -> Result<Option<Outcome>, ConditioningError> {
        match instruction {
            RegionInstruction::ExecuteRegion { region, .. } => self.execute_region(region)?,
            RegionInstruction::Measure(request) => {
                let outcome = self.measure(*request)?;
                let tag = u32::try_from(request.result_id).map_err(|_| {
                    ConditioningError::ResultOutOfRange {
                        result_id: request.result_id,
                        result_count: self.outcomes.len(),
                    }
                })?;
                return Ok(Some(Outcome {
                    tag,
                    value: outcome.into(),
                }));
            }
            RegionInstruction::Reset { qubit } => self.reset(*qubit)?,
        }
        Ok(None)
    }

    fn finish(&self) -> Result<(), ConditioningError> {
        match self.live_reset {
            Some(qubit) => Err(ConditioningError::ResetOfLiveQubit { qubit }),
            None => Ok(()),
        }
    }
}

/// A terminal of the region ISA that records the path instead of evolving a
/// state. It keeps the first error of the path, which ends the run, for
/// [`ConditionedCircuit::from_prepared_program`] to report.
impl Executor<Batch<RegionInstruction>> for ConditionedTerminal<'_> {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        let mut outcomes = Vec::new();
        if let Some(error) = self.error {
            return Ok(Immediate::failed(outcomes, error.to_string()));
        }
        for instruction in batch {
            match self.run(instruction) {
                Ok(outcome) => outcomes.extend(outcome),
                Err(error) => {
                    self.error = Some(error);
                    return Ok(Immediate::failed(outcomes, error.to_string()));
                }
            }
        }
        Ok(Immediate::done(outcomes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedCircuitError {
    QubitOutOfRange {
        operation_index: usize,
        qubit: QubitID,
        qubit_count: usize,
    },
    LossOutcome {
        operation_index: usize,
    },
    DuplicateResult {
        operation_index: usize,
        result_id: usize,
    },
    UnknownResult {
        result_id: usize,
    },
}

impl fmt::Display for ConditionedCircuitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QubitOutOfRange {
                operation_index,
                qubit,
                qubit_count,
            } => write!(
                formatter,
                "operation {operation_index} measures qubit {qubit}, but the circuit has {qubit_count} qubits"
            ),
            Self::LossOutcome { operation_index } => write!(
                formatter,
                "operation {operation_index} fixes a loss outcome, which is not a projection"
            ),
            Self::DuplicateResult {
                operation_index,
                result_id,
            } => write!(
                formatter,
                "operation {operation_index} fixes result {result_id}, which an earlier operation already fixes"
            ),
            Self::UnknownResult { result_id } => {
                write!(formatter, "no operation fixes result {result_id}")
            }
        }
    }
}

impl std::error::Error for ConditionedCircuitError {}

/// Why a program and given outcomes do not give a [`ConditionedCircuit`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditioningError {
    OutcomeCountMismatch {
        expected: usize,
        actual: usize,
    },
    ResultOutOfRange {
        result_id: usize,
        result_count: usize,
    },
    /// The path would measure `result_id` a second time, but a record holds
    /// one value per result, so it does not describe a single pass through
    /// the program. There are two causes, which cannot be told apart here:
    ///
    /// 1. **The record fails a selection check.** Take this Stim program,
    ///    compiled to QIR by `qdk.stim.compile`, and the record `[r0, r1]`:
    ///
    ///    ```text
    ///    SELECT {             select_0: reset q0, q1; H q1; CX q1, q0
    ///      R 0 1                        mresetz q0 -> r0
    ///      H 1                          restart = read_loss(r0) | read_result(r0)
    ///      CX 1 0                       br restart ? select_0 : continue
    ///      MR 0               continue: mz q1 -> r1
    ///      REQUIRE rec[-1]
    ///    }
    ///    M 1
    ///    ```
    ///
    ///    `REQUIRE rec[-1]` passes only when `r0 = 0`. The record `[0, 0]`
    ///    passes once and gives `H q1; CX q1, q0; M q0 = 0 (reset); M q1 = 0`.
    ///    The record `[1, 1]` is a possible measurement outcome of the block
    ///    (probability 1/2) but fails the check, so the program goes back to
    ///    `select_0` and measures `r0` again, and this error reports
    ///    `result_id = 0`.
    ///
    ///    This follows selection semantics: a sampler runs the program,
    ///    discards records that fail a check and samples again, so such a
    ///    record is never a program output and its probability among the
    ///    outputs is 0. Records from a sampler therefore always pass every
    ///    check; a failing record usually comes from a wrong bit order, a
    ///    different program version, or bits taken from a failed attempt.
    ///
    /// 2. **The program measures the result more than once on every path**,
    ///    for example a loop that reuses a result id. Conditioning does not
    ///    support such programs.
    ///
    /// This is an error rather than a zero probability because no single
    /// circuit exists for the record, and because cause 2 would make zero the
    /// wrong answer.
    ResultMeasuredAgain {
        result_id: usize,
    },
    QubitOutOfRange {
        qubit: QubitID,
        qubit_count: usize,
    },
    ResetOfLiveQubit {
        qubit: QubitID,
    },
    Control(AdaptiveExecutionError),
    Circuit(ConditionedCircuitError),
}

impl fmt::Display for ConditioningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutcomeCountMismatch { expected, actual } => write!(
                formatter,
                "the program has {expected} results, but {actual} outcomes were given"
            ),
            Self::ResultOutOfRange {
                result_id,
                result_count,
            } => write!(
                formatter,
                "the program measures result {result_id}, but it declares {result_count} results"
            ),
            Self::ResultMeasuredAgain { result_id } => write!(
                formatter,
                "result {result_id} would be measured a second time, so the outcome record does not describe a single pass through the program: either the record fails a selection check (for example, a REQUIRE in a SELECT block rejects it and the block restarts; only records that pass every check are program outputs), or the program measures result {result_id} more than once, which conditioning does not support"
            ),
            Self::QubitOutOfRange { qubit, qubit_count } => write!(
                formatter,
                "the program uses qubit {qubit}, but it declares {qubit_count} qubits"
            ),
            Self::ResetOfLiveQubit { qubit } => write!(
                formatter,
                "qubit {qubit} is reset after a gate without an intervening measurement; only a reset of a qubit in a known basis state has a form in a conditioned circuit"
            ),
            Self::Control(error) => write!(formatter, "{error}"),
            Self::Circuit(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ConditioningError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Control(error) => Some(error),
            Self::Circuit(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConditionedCircuit, ConditionedCircuitError, ConditionedOperation};
    use crate::{MeasurementResult, execution::UnitaryOperation};

    fn measure(qubit: usize, outcome: MeasurementResult) -> ConditionedOperation {
        ConditionedOperation::Measure {
            qubit,
            result_id: 0,
            outcome,
            reset: false,
        }
    }

    #[test]
    fn keeps_operations_in_order() {
        let operations = vec![
            ConditionedOperation::Unitary(UnitaryOperation::H { target: 0 }),
            measure(0, MeasurementResult::One),
        ];
        let circuit = ConditionedCircuit::new(1, operations.clone()).expect("valid circuit");
        assert_eq!(circuit.qubit_count(), 1);
        assert_eq!(circuit.operations(), operations.as_slice());
    }

    #[test]
    fn rejects_measurement_outside_the_register() {
        assert_eq!(
            ConditionedCircuit::new(1, vec![measure(1, MeasurementResult::Zero)]),
            Err(ConditionedCircuitError::QubitOutOfRange {
                operation_index: 0,
                qubit: 1,
                qubit_count: 1,
            })
        );
    }

    #[test]
    fn rejects_a_result_fixed_twice() {
        assert_eq!(
            ConditionedCircuit::new(
                2,
                vec![
                    measure(0, MeasurementResult::Zero),
                    measure(1, MeasurementResult::One)
                ]
            ),
            Err(ConditionedCircuitError::DuplicateResult {
                operation_index: 1,
                result_id: 0,
            })
        );
    }

    #[test]
    fn rejects_loss_outcome() {
        assert_eq!(
            ConditionedCircuit::new(1, vec![measure(0, MeasurementResult::Loss)]),
            Err(ConditionedCircuitError::LossOutcome { operation_index: 0 })
        );
    }
}
