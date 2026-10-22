// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum Execution Profiler: backend interfaces for quantum instructions.
//!
//! QIR execution requires LLVM 20 development libraries
//! (`LLVM_SYS_201_PREFIX` can specify their installation).
//!
//! The runner supports static qubit/result IDs, adaptive control flow, and
//! structured output recording. Dynamic allocation and legacy runtime calls
//! are rejected. It does not depend on or modify QIR Runner.
//!
//! Run the test suite with `cargo test -p qep`.

#[cfg(test)]
mod tests;

mod adapters;
mod backends;
pub mod instructions;
pub mod interface;
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
pub use profilers::{
    Profiler, ProfilerConfig, ProfilerResult, SequentialCountingProfiler, TemporalProfiler,
    TemporalProfiles, TimeSeries, TotalCounts, TumblingWindowAggregator,
};
pub use runner::{
    DEFAULT_MAX_BATCH_SIZE, OutputRecord, OutputValue, QsharpRunError, RunOutput, run_qir,
    run_qir_with_batch_size, run_qsharp_call, run_qsharp_expr,
};
pub use transpilers::{RzQftTranspiler, SwapTransplier, Transpiler};
