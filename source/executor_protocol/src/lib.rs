// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol: parts that compose with `>>` into a chain that
//! runs shots.

mod batch_readouts;
mod instruction_set;
mod seed;
mod shot_set;

#[cfg(test)]
mod tests;

pub use batch_readouts::{
    BatchReadouts, ReadoutFailure, ReadoutIndex, ReadoutValue, ShapeFault, ShotConditions,
};
pub use instruction_set::{
    Angle, Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, FitFaults,
    Guard, Instruction, InstructionDef, InstructionFault, InstructionMismatch, InstructionSet,
    Meaning, MeaningFault, Observable, Opcode, OpcodeMap, Parameter, ParameterKind, Pauli,
    ReadoutParity, SelectPattern,
};
pub use seed::Seed;
pub use shot_set::{ShotId, ShotSet, ShotSetFault};
