// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A bytecode program in the JSON form of `AdaptiveProgram.as_dict()`, which
//! the corpus and the Python pass's dumps hold. The benchmark includes this
//! file too, so it names the bytecode types by their paths.

use qdk_simulators::bytecode::{
    AdaptiveProgram, Block, Function, Instruction, Op, PhiNodeEntry, SwitchCase,
};
use serde::Deserialize;

/// An instruction as a tuple, in the order of `Instruction::from_tuple`.
type InstructionTuple = (u64, u64, u64, u64, u64, u64, u64, u64);

/// A bytecode program as `AdaptiveProgram.as_dict()` gives it, with the keys
/// `adaptive_program_from_pydict` reads.
#[derive(Deserialize)]
pub struct Bytecode {
    num_qubits: u32,
    num_results: u32,
    num_registers: u32,
    entry_block: u64,
    blocks: Vec<(u64, u64, u64)>,
    instructions: Vec<InstructionTuple>,
    quantum_ops: Vec<(u64, u64, u64, u64, u64)>,
    functions: Vec<(u64, u64, u64)>,
    phi_entries: Vec<(u64, u64)>,
    switch_cases: Vec<(u64, u64)>,
    call_args: Vec<u64>,
    constant_data: Vec<u64>,
}

impl Bytecode {
    /// The program, as `adaptive_program_from_pydict` builds it: block ids
    /// dropped, and a default entry in each empty table.
    pub fn program(&self) -> AdaptiveProgram<u64> {
        fn non_empty<T: Default>(mut table: Vec<T>) -> Vec<T> {
            if table.is_empty() {
                table.push(T::default());
            }
            table
        }
        AdaptiveProgram {
            num_qubits: self.num_qubits,
            num_results: self.num_results,
            num_registers: self.num_registers,
            entry_block: self.entry_block,
            instructions: self
                .instructions
                .iter()
                .map(|&t| Instruction::from_tuple(t))
                .collect(),
            block_table: non_empty(
                self.blocks
                    .iter()
                    .map(|&(_, offset, count)| Block::from_tuple((offset, count)))
                    .collect(),
            ),
            function_table: non_empty(
                self.functions
                    .iter()
                    .map(|&t| Function::from_tuple(t))
                    .collect(),
            ),
            phi_entries: non_empty(
                self.phi_entries
                    .iter()
                    .map(|&t| PhiNodeEntry::from_tuple(t))
                    .collect(),
            ),
            switch_cases: non_empty(
                self.switch_cases
                    .iter()
                    .map(|&t| SwitchCase::from_tuple(t))
                    .collect(),
            ),
            call_args: non_empty(self.call_args.clone()),
            constant_data: non_empty(self.constant_data.clone()),
            quantum_ops: self
                .quantum_ops
                .iter()
                .map(|&t| Op::from_tuple(t))
                .collect(),
        }
    }
}
