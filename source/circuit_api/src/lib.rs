// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python-independent circuit documents, construction sessions and display.

mod angle;
mod builder;
mod circuit;
mod diagram;
mod view;

pub use angle::{AngleExpression, combine, negated};
pub use builder::{BodyBuilder, BodyKind, SwitchBuilder, SwitchCaseBuilder};
pub use circuit::{Circuit, CircuitError, Register, RegisterSpec};
pub use view::{ConditionView, InstructionView, StoredAngle};

#[cfg(test)]
mod tests;
