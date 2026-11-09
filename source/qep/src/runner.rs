// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod batcher;
mod qir_bytecode_runner;
mod qs_runner;
pub use batcher::{Batcher, DEFAULT_MAX_BATCH_SIZE};
pub use qir_bytecode_runner::run_adaptive_program;
pub use qs_runner::{QsharpRunError, run_qsharp_call, run_qsharp_expr};

/// A value emitted by a program's output-recording calls.
///
/// Array and tuple records introduce the given number of elements. The bytecode
/// runner currently emits only leaf values.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputValue {
    Array(u64),
    Tuple(u64),
    Result(bool),
    Bool(bool),
    Int(i64),
    Double(f64),
}

/// An output value and its optional label.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputRecord {
    pub value: OutputValue,
    pub label: Option<String>,
}

/// The output from one program execution.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub records: Vec<OutputRecord>,
    /// The entry-point return value, if available; bytecode execution leaves this unset.
    pub return_value: Option<i64>,
}
