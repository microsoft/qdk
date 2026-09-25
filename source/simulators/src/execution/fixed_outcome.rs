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

use super::UnitaryOperation;

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
        for (operation_index, operation) in operations.iter().enumerate() {
            if let FixedOutcomeOperation::Measure { qubit, outcome, .. } = *operation {
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
        }
    }
}

impl std::error::Error for FixedOutcomeCircuitError {}

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
    fn rejects_loss_outcome() {
        assert_eq!(
            FixedOutcomeCircuit::new(1, vec![measure(0, MeasurementResult::Loss)]),
            Err(FixedOutcomeCircuitError::LossOutcome { operation_index: 0 })
        );
    }
}
