// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol: parts that compose with `>>` into a chain that
//! runs shots.

mod admission;
mod batch;
mod batch_readouts;
mod checked;
mod composition;
mod instruction_set;
mod panics;
mod roles;
mod runner;
mod seed;
mod shot_set;

#[cfg(test)]
mod tests;

pub use admission::{AdmissionFaults, Capabilities, Needs, ShortOffer, UnmetNeed};
pub use batch::Batch;
pub use batch_readouts::{
    BatchReadouts, ReadoutFailure, ReadoutIndex, ReadoutValue, ShapeFault, ShotConditions,
};
pub use composition::{Adapter, Compose, Composed, CompositionFaults, Driver, Executor};
pub use instruction_set::{
    Angle, Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, FitFaults,
    Guard, Instruction, InstructionDef, InstructionFault, InstructionMismatch, InstructionSet,
    Meaning, MeaningFault, Observable, Opcode, OpcodeMap, Parameter, ParameterKind, Pauli,
    ReadoutParity, SelectPattern,
};
pub use roles::{
    Adapt, Drive, DriveError, Execute, ExecuteError, ExecuteRefusal, Failure, RunRefusal,
};
pub use runner::{RunError, Runner};
pub use seed::Seed;
pub use shot_set::{ShotId, ShotSet, ShotSetFault};
