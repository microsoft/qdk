// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One program path with every measurement outcome fixed in advance.
//!
//! Fixing the outcomes turns an adaptive program into a linear sequence of
//! unitaries and rank-one projections, which backends can evaluate without
//! sampling: a tensor network closes each projection with a basis-state cap,
//! and an MPS applies it as a non-unitary operator.

use std::fmt;

use crate::{MeasurementResult, QubitID};

use super::{
    AdaptiveExecutionError, MeasurementKind, MeasurementRequest, PreparedAdaptiveProgram,
    QuantumEvolutionRegion, RegionConsumer, ShotExecutionError, UnitaryOperation,
    drive_prepared_shot,
};

/// One operation on the path selected by fixed measurement outcomes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FixedOutcomeOperation {
    Unitary(UnitaryOperation),
    /// Projects `qubit` onto the basis state of the fixed `outcome`.
    ///
    /// The projector |b⟩⟨b| has rank one, so afterwards the qubit is exactly
    /// |b⟩, or |0⟩ when `reset` is set. `reset` covers measure-and-reset and a
    /// reset that follows this measurement with no operation on the qubit in
    /// between; any other reset has no fixed-outcome form.
    Measure {
        qubit: QubitID,
        result_id: usize,
        outcome: MeasurementResult,
        reset: bool,
    },
}

/// The operations of one fixed-outcome path, starting from |0…0⟩.
///
/// Measurement operands and outcomes are validated here. Unitary operands are
/// validated by each consumer against the operations it supports, as for
/// [`super::QuantumEvolutionRegion`].
#[derive(Clone, Debug, PartialEq)]
pub struct FixedOutcomeCircuit {
    qubit_count: usize,
    operations: Vec<FixedOutcomeOperation>,
}

impl FixedOutcomeCircuit {
    pub fn new(
        qubit_count: usize,
        operations: Vec<FixedOutcomeOperation>,
    ) -> Result<Self, FixedOutcomeCircuitError> {
        let mut result_ids = rustc_hash::FxHashSet::default();
        for (operation_index, operation) in operations.iter().enumerate() {
            if let FixedOutcomeOperation::Measure {
                qubit,
                result_id,
                outcome,
                ..
            } = *operation
            {
                if qubit >= qubit_count {
                    return Err(FixedOutcomeCircuitError::QubitOutOfRange {
                        operation_index,
                        qubit,
                        qubit_count,
                    });
                }
                // Loss is a classical event with state-dependent handling, not
                // a projection, so it has no single fixed-outcome operator.
                if outcome == MeasurementResult::Loss {
                    return Err(FixedOutcomeCircuitError::LossOutcome { operation_index });
                }
                // Each result has one fixed outcome, so a result id names one
                // projection.
                if !result_ids.insert(result_id) {
                    return Err(FixedOutcomeCircuitError::DuplicateResult {
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
    pub fn operations(&self) -> &[FixedOutcomeOperation] {
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
    ) -> Result<Self, FixedOutcomeCircuitError> {
        let mut operations = self.operations.clone();
        let fixed = operations
            .iter_mut()
            .find_map(|operation| match operation {
                FixedOutcomeOperation::Measure {
                    result_id: id,
                    outcome,
                    ..
                } if *id == result_id => Some(outcome),
                _ => None,
            })
            .ok_or(FixedOutcomeCircuitError::UnknownResult { result_id })?;
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
    /// amplitude; it is rejected with [`FixedOutcomeError::ResetOfLiveQubit`].
    ///
    /// A record that fails a selection check, such as a `REQUIRE` in a Stim
    /// `SELECT` block, makes the program restart the block and measure a
    /// result again; see [`FixedOutcomeError::ResultMeasuredAgain`] for an
    /// example. The restart usually resets qubits that are still in use before
    /// that second measurement. Those resets are a consequence of the failed
    /// check, not its cause, so [`FixedOutcomeError::ResetOfLiveQubit`] is
    /// reported only if the program completes, and the failed check is
    /// reported as [`FixedOutcomeError::ResultMeasuredAgain`].
    // TODO(selection-normalization): the circuit gives the probability of one
    // pass through the program. When a selection check can fail (noise, or a
    // non-deterministic REQUIRE), outputs are that probability divided by the
    // acceptance probability, which is not computed yet. See the example in
    // the execution README ("fixed-outcome circuit gives the probability of
    // one pass").
    pub fn from_prepared_program(
        prepared: &PreparedAdaptiveProgram<u64>,
        outcomes: &[bool],
    ) -> Result<Self, FixedOutcomeError> {
        let program = prepared.program();
        let result_count = usize::try_from(program.num_results)
            .expect("adaptive result count should fit in usize");
        if outcomes.len() != result_count {
            return Err(FixedOutcomeError::OutcomeCountMismatch {
                expected: result_count,
                actual: outcomes.len(),
            });
        }
        let qubit_count =
            usize::try_from(program.num_qubits).expect("adaptive qubit count should fit in usize");
        let mut consumer = FixedOutcomeConsumer::new(qubit_count, outcomes);
        drive_prepared_shot(prepared, &mut consumer).map_err(|error| match error {
            ShotExecutionError::Control(control)
            | ShotExecutionError::ControlAndClose { control, .. } => {
                FixedOutcomeError::Control(control)
            }
            ShotExecutionError::Consumer(error)
            | ShotExecutionError::Close(error)
            | ShotExecutionError::ConsumerAndClose {
                consumer: error, ..
            } => error,
        })?;
        Self::new(qubit_count, consumer.operations).map_err(FixedOutcomeError::Circuit)
    }
}

fn basis_outcome(one: bool) -> MeasurementResult {
    if one {
        MeasurementResult::One
    } else {
        MeasurementResult::Zero
    }
}

/// What the fixed-outcome consumer knows about one qubit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QubitState {
    /// Exactly |0⟩: untouched since allocation or since a folded reset.
    Fresh,
    /// A gate acted on the qubit since it was last in a known basis state.
    Live,
    /// Measured with no operation since; holds the index of that `Measure`.
    JustMeasured(usize),
}

struct FixedOutcomeConsumer<'outcomes> {
    outcomes: &'outcomes [bool],
    measured_results: Vec<bool>,
    qubits: Vec<QubitState>,
    operations: Vec<FixedOutcomeOperation>,
    /// The first live reset, reported only if the program completes.
    live_reset: Option<QubitID>,
}

impl<'outcomes> FixedOutcomeConsumer<'outcomes> {
    fn new(qubit_count: usize, outcomes: &'outcomes [bool]) -> Self {
        Self {
            outcomes,
            measured_results: vec![false; outcomes.len()],
            qubits: vec![QubitState::Fresh; qubit_count],
            operations: Vec::new(),
            live_reset: None,
        }
    }

    fn qubit_state(&mut self, qubit: QubitID) -> Result<&mut QubitState, FixedOutcomeError> {
        let qubit_count = self.qubits.len();
        self.qubits
            .get_mut(qubit)
            .ok_or(FixedOutcomeError::QubitOutOfRange { qubit, qubit_count })
    }
}

impl RegionConsumer for FixedOutcomeConsumer<'_> {
    type PreparedRegion<'region> = &'region QuantumEvolutionRegion;
    type RegionReport = ();
    type ExecutionReport = ();
    type Error = FixedOutcomeError;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error> {
        Ok(region)
    }

    fn execute_region(
        &mut self,
        region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error> {
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
                .push(FixedOutcomeOperation::Unitary(operation));
        }
        Ok(())
    }

    fn measure(&mut self, request: MeasurementRequest) -> Result<MeasurementResult, Self::Error> {
        self.qubit_state(request.qubit)?;
        let result_count = self.outcomes.len();
        let measured = self.measured_results.get_mut(request.result_id).ok_or(
            FixedOutcomeError::ResultOutOfRange {
                result_id: request.result_id,
                result_count,
            },
        )?;
        if *measured {
            return Err(FixedOutcomeError::ResultMeasuredAgain {
                result_id: request.result_id,
            });
        }
        *measured = true;
        let outcome = basis_outcome(self.outcomes[request.result_id]);
        let index = self.operations.len();
        self.operations.push(FixedOutcomeOperation::Measure {
            qubit: request.qubit,
            result_id: request.result_id,
            outcome,
            reset: request.kind == MeasurementKind::MeasureResetZ,
        });
        *self.qubit_state(request.qubit)? = QubitState::JustMeasured(index);
        Ok(outcome)
    }

    fn reset(&mut self, qubit: QubitID) -> Result<(), Self::Error> {
        let state = self.qubit_state(qubit)?;
        match *state {
            QubitState::Fresh => {}
            QubitState::JustMeasured(index) => {
                *state = QubitState::Fresh;
                if let FixedOutcomeOperation::Measure { reset, .. } = &mut self.operations[index] {
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

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        match self.live_reset {
            Some(qubit) => Err(FixedOutcomeError::ResetOfLiveQubit { qubit }),
            None => Ok(()),
        }
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedOutcomeCircuitError {
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

impl fmt::Display for FixedOutcomeCircuitError {
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

impl std::error::Error for FixedOutcomeCircuitError {}

/// Why a program and a fixed outcome record do not give a fixed-outcome circuit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedOutcomeError {
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
    ///    for example a loop that reuses a result id. Fixed-outcome evaluation
    ///    does not support such programs.
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
    Circuit(FixedOutcomeCircuitError),
}

impl fmt::Display for FixedOutcomeError {
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
                "result {result_id} would be measured a second time, so the outcome record does not describe a single pass through the program: either the record fails a selection check (for example, a REQUIRE in a SELECT block rejects it and the block restarts; only records that pass every check are program outputs), or the program measures result {result_id} more than once, which fixed-outcome evaluation does not support"
            ),
            Self::QubitOutOfRange { qubit, qubit_count } => write!(
                formatter,
                "the program uses qubit {qubit}, but it declares {qubit_count} qubits"
            ),
            Self::ResetOfLiveQubit { qubit } => write!(
                formatter,
                "qubit {qubit} is reset after a gate without an intervening measurement; only a reset of a qubit in a known basis state has a fixed-outcome form"
            ),
            Self::Control(error) => write!(formatter, "{error}"),
            Self::Circuit(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for FixedOutcomeError {
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
    use super::{FixedOutcomeCircuit, FixedOutcomeCircuitError, FixedOutcomeOperation};
    use crate::{MeasurementResult, execution::UnitaryOperation};

    fn measure(qubit: usize, outcome: MeasurementResult) -> FixedOutcomeOperation {
        FixedOutcomeOperation::Measure {
            qubit,
            result_id: 0,
            outcome,
            reset: false,
        }
    }

    #[test]
    fn keeps_operations_in_order() {
        let operations = vec![
            FixedOutcomeOperation::Unitary(UnitaryOperation::H { target: 0 }),
            measure(0, MeasurementResult::One),
        ];
        let circuit = FixedOutcomeCircuit::new(1, operations.clone()).expect("valid circuit");
        assert_eq!(circuit.qubit_count(), 1);
        assert_eq!(circuit.operations(), operations.as_slice());
    }

    #[test]
    fn rejects_measurement_outside_the_register() {
        assert_eq!(
            FixedOutcomeCircuit::new(1, vec![measure(1, MeasurementResult::Zero)]),
            Err(FixedOutcomeCircuitError::QubitOutOfRange {
                operation_index: 0,
                qubit: 1,
                qubit_count: 1,
            })
        );
    }

    #[test]
    fn rejects_a_result_fixed_twice() {
        assert_eq!(
            FixedOutcomeCircuit::new(
                2,
                vec![
                    measure(0, MeasurementResult::Zero),
                    measure(1, MeasurementResult::One)
                ]
            ),
            Err(FixedOutcomeCircuitError::DuplicateResult {
                operation_index: 1,
                result_id: 0,
            })
        );
    }

    #[test]
    fn rejects_loss_outcome() {
        assert_eq!(
            FixedOutcomeCircuit::new(1, vec![measure(0, MeasurementResult::Loss)]),
            Err(FixedOutcomeCircuitError::LossOutcome { operation_index: 0 })
        );
    }
}
