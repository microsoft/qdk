// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operation stored in each instruction.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::control_flow::ControlFlow;
use crate::gate::StandardGate;

/// Estimator identifier for an operation with no standard-gate counterpart.
const GENERIC_INSTRUCTION_ID: u16 = 65535;
/// Estimator identifier for an arbitrary one-qubit unitary.
const ONE_QUBIT_UNITARY_ID: u16 = 1280;
/// Estimator identifier for an arbitrary two-qubit unitary.
const TWO_QUBIT_UNITARY_ID: u16 = 1281;

/// The unit a duration is measured in.
///
/// Spelled the way OpenQASM spells it, since that is where a duration has to
/// survive to. `Dt` is the target's own time step, which is what a number with
/// no physical unit attached actually means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    Dt,
    Ns,
    Us,
    Ms,
    S,
}

impl TimeUnit {
    /// Every unit a duration may be written in.
    pub const ALL: [Self; 5] = [Self::Dt, Self::Ns, Self::Us, Self::Ms, Self::S];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dt => "dt",
            Self::Ns => "ns",
            Self::Us => "us",
            Self::Ms => "ms",
            Self::S => "s",
        }
    }
}

impl Display for TimeUnit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for TimeUnit {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|unit| unit.name() == text)
            .ok_or(())
    }
}

/// An operation carrying data that cannot live inline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomOperation {
    pub name: String,
    pub qubit_arity: u8,
    pub param_arity: u8,
}

/// An explicit unitary given as a flattened row-major complex matrix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitaryOperation {
    pub qubit_arity: u8,
    pub matrix: Vec<(f64, f64)>,
}

/// What an instruction does.
///
/// The standard-gate case carries no pointer and costs no allocation, which is
/// the property the construction path depends on. Every other case boxes its
/// payload, so the size of this type does not grow with the largest variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackedOperation {
    Standard(StandardGate),
    Custom(Box<CustomOperation>),
    Unitary(Box<UnitaryOperation>),
    Barrier {
        qubit_arity: u8,
    },
    Reset,
    Delay {
        duration: f64,
        unit: TimeUnit,
    },
    /// Transfers control into one or more blocks owned by the circuit.
    ControlFlow(Box<ControlFlow>),
}

impl PackedOperation {
    /// Estimator instruction identifier for this operation.
    ///
    /// Operations with no standard-gate counterpart map onto the estimator's
    /// generic identifiers rather than inventing parallel names. Custom
    /// operations map to the generic identifier rather than being silently
    /// treated as any specific gate.
    #[must_use]
    pub fn instruction_id(&self) -> u16 {
        match self {
            Self::Standard(gate) => gate.instruction_id(),
            Self::Unitary(unitary) => match unitary.qubit_arity {
                1 => ONE_QUBIT_UNITARY_ID,
                2 => TWO_QUBIT_UNITARY_ID,
                _ => GENERIC_INSTRUCTION_ID,
            },
            Self::Custom(_) | Self::Barrier { .. } | Self::Reset | Self::Delay { .. } => {
                GENERIC_INSTRUCTION_ID
            }
            Self::ControlFlow(_) => GENERIC_INSTRUCTION_ID,
        }
    }

    /// Number of qubits this operation acts on.
    #[must_use]
    pub fn qubit_arity(&self) -> u8 {
        match self {
            Self::Standard(gate) => gate.qubit_arity(),
            Self::Custom(custom) => custom.qubit_arity,
            Self::Unitary(unitary) => unitary.qubit_arity,
            Self::Barrier { qubit_arity } => *qubit_arity,
            Self::Reset | Self::Delay { .. } => 1,
            // Operands live on the instructions inside the referenced blocks.
            Self::ControlFlow(_) => 0,
        }
    }

    /// Name for this operation.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Standard(gate) => gate.name(),
            Self::Custom(custom) => &custom.name,
            Self::Unitary(_) => "unitary",
            Self::Barrier { .. } => "barrier",
            Self::Reset => "reset",
            Self::Delay { .. } => "delay",
            Self::ControlFlow(control) => control.name(),
        }
    }

    /// Whether this operation is stored inline with no heap payload.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(
            self,
            Self::Standard(_) | Self::Barrier { .. } | Self::Reset | Self::Delay { .. }
        )
    }
}
