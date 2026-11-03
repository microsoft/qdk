// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dense storage for quantum circuits.
//!
//! The store keeps a flat vector of instructions. A standard gate is encoded
//! inline, so appending one costs no heap allocation beyond amortized vector
//! growth. Operands are referenced by handle rather than owned per instruction.
//!
//! Instruction identity, operand addressing, encoding, and properties follow
//! the circuit representation contract that ships with the Python package.

pub mod angle;
pub mod classical;
pub mod control_flow;
pub mod gate;
pub mod operands;
pub mod operation;
pub mod snapshot;
mod snapshot_binary;
mod snapshot_text;
pub mod store;

pub use angle::{AngleArena, AngleExpr, AngleId, AngleOp};
pub use classical::{
    BinaryOp, Expr, ExprArena, ExprId, Literal, ParamValue, Parameter, ParameterId, Var, VarId,
    VarType,
};
pub use control_flow::{
    BlockId, BoxBody, ControlFlow, ForLoop, IfElse, LoopIndex, Switch, WhileLoop,
};
pub use gate::{STANDARD_GATE_COUNT, StandardGate};
pub use operands::{Clbit, EMPTY_OPERANDS, OperandTable, OperandsId, Qubit};
pub use operation::{CustomOperation, PackedOperation, TimeUnit, UnitaryOperation};
pub use snapshot::{SNAPSHOT_SCHEMA_VERSION, Snapshot, SnapshotError, SnapshotInstruction};
pub use store::{Block, CircuitStore, Instruction, StoreError};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod snapshot_tests;
