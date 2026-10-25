// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An Adaptive program from the JSON of `AdaptiveProgram.as_dict()`, for the
//! tests that run a user's program, such as H₂'s, which stays out of the
//! repository.
//!
//! qdk builds the program from that dict in Python only
//! (`adaptive_program_from_pydict` in `qdk_package`, behind pyo3), so the
//! Rust tests read its JSON. Two crates' tests read it: `qdk_simulators`'s
//! `h2_qpe` and `tensornet_simulator`'s A100 qualification, which includes
//! this file with `#[path]`. It is test support, not API: it panics with
//! the reason on input it cannot read.

use qdk_simulators::bytecode::{
    AdaptiveProgram, Block, Function, Instruction, Op, PhiNodeEntry, SwitchCase,
};
use serde_json::Value;

/// The rows of table `key`, each of `N` words.
fn rows<const N: usize>(program: &Value, key: &str) -> Vec<[u64; N]> {
    program[key]
        .as_array()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` list"))
        .iter()
        .map(|row| {
            let words: Vec<u64> = row
                .as_array()
                .unwrap_or_else(|| panic!("each row of `{key}` should be a list"))
                .iter()
                .map(|word| word.as_u64().expect("each word should be a u64"))
                .collect();
            words
                .try_into()
                .unwrap_or_else(|_| panic!("each row of `{key}` should have {N} words"))
        })
        .collect()
}

/// The words of the flat list `key`.
fn words(program: &Value, key: &str) -> Vec<u64> {
    program[key]
        .as_array()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` list"))
        .iter()
        .map(|word| word.as_u64().expect("each word should be a u64"))
        .collect()
}

fn word(program: &Value, key: &str) -> u64 {
    program[key]
        .as_u64()
        .unwrap_or_else(|| panic!("the bytecode should have a `{key}` number"))
}

/// `table`, or one default entry if it is empty, as
/// `adaptive_program_from_pydict` pads it (WebGPU needs non-empty arrays).
fn non_empty<T: Default>(mut table: Vec<T>) -> Vec<T> {
    if table.is_empty() {
        table.push(T::default());
    }
    table
}

/// The program from the JSON of `AdaptiveProgram.as_dict()`, built as
/// `adaptive_program_from_pydict` in `qdk_package` builds it.
///
/// # Panics
///
/// When `json` is not such a dict, naming the key it cannot read.
pub fn adaptive_program_from_json(json: &str) -> AdaptiveProgram<u64> {
    let program: Value = serde_json::from_str(json).expect("the bytecode should be JSON");
    let count = |key| u32::try_from(word(&program, key)).expect("a count should fit u32");
    AdaptiveProgram {
        num_qubits: count("num_qubits"),
        num_results: count("num_results"),
        num_registers: count("num_registers"),
        entry_block: word(&program, "entry_block"),
        instructions: rows::<8>(&program, "instructions")
            .into_iter()
            .map(|[a, b, c, d, e, f, g, h]| Instruction::from_tuple((a, b, c, d, e, f, g, h)))
            .collect(),
        quantum_ops: rows::<5>(&program, "quantum_ops")
            .into_iter()
            .map(|[a, b, c, d, e]| Op::from_tuple((a, b, c, d, e)))
            .collect(),
        block_table: non_empty(
            rows::<3>(&program, "blocks")
                .into_iter()
                .map(|[_, offset, count]| Block::from_tuple((offset, count)))
                .collect(),
        ),
        function_table: non_empty(
            rows::<3>(&program, "functions")
                .into_iter()
                .map(|[a, b, c]| Function::from_tuple((a, b, c)))
                .collect(),
        ),
        phi_entries: non_empty(
            rows::<2>(&program, "phi_entries")
                .into_iter()
                .map(|[a, b]| PhiNodeEntry::from_tuple((a, b)))
                .collect(),
        ),
        switch_cases: non_empty(
            rows::<2>(&program, "switch_cases")
                .into_iter()
                .map(|[a, b]| SwitchCase::from_tuple((a, b)))
                .collect(),
        ),
        call_args: non_empty(words(&program, "call_args")),
        constant_data: non_empty(words(&program, "constant_data")),
    }
}
