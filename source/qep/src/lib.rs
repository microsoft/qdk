// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum Execution Profiler: backend interfaces for quantum instructions.
//!
//! Programs execute through the Q# interpreter or adaptive bytecode runner.
//! Neither runner requires LLVM development libraries.
//!
//! The bytecode runner supports static qubit/result IDs, adaptive control flow,
//! and leaf output records. The Q# runner delegates quantum instructions through
//! a batched backend interface.
//!
//! Run the test suite with `cargo test -p qep`.

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

mod adapters;
mod backends;
pub mod instructions;
pub mod interface;
mod models;
mod profilers;
mod runner;
mod transpilers;
pub use adapters::{ResultAdapter, TranspilingAdapter};
pub use backends::{QsBackendShim, QuantumBackends};
pub use instructions::{
    InstructionId, OPERATION_COUNT, instruction_id_to_name, instruction_name_to_id,
    validate_instruction,
};
pub use interface::{
    BatchResult, Parameter, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
pub use models::DemoModel1;
pub use profilers::{
    AggregationType, Profiler, ProfilerResult, SequentialCountingProfiler, TemporalProfiler,
    TemporalProfilerConfig, TemporalProfiles, TimeSeries, TotalCounts, TumblingWindowAggregator,
    make_profiler_for_demo_model,
};
pub use runner::{
    Batcher, DEFAULT_MAX_BATCH_SIZE, OutputRecord, OutputValue, QsharpRunError, RunOutput,
    run_adaptive_program, run_qsharp_call, run_qsharp_expr,
};
pub use transpilers::{
    CliffordTranspiler, MzResetTranspiler, RzQftTranspiler, RzTranspiler, SwapTransplier,
    Transpiler,
};
