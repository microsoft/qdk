// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol: parts that compose with `>>` into a chain that
//! runs shots.

mod instruction_set;

pub use instruction_set::{
    Angle, Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, FitFaults,
    Guard, Instruction, InstructionDef, InstructionFault, InstructionMismatch, InstructionSet,
    Meaning, MeaningFault, Observable, Opcode, OpcodeMap, Parameter, ParameterKind, Pauli,
    ReadoutParity, SelectPattern,
};

#[cfg(test)]
mod tests;
