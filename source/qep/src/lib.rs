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
pub use adapters::{DecomposeToffoliAdapter, ResultAdapter};
pub use backends::{QsBackendShim, SparseSimBackend};
pub use instructions::{
    InstructionId, OPERATION_COUNT, instruction_id_to_name, instruction_name_to_id,
    validate_instruction,
};
pub use interface::{
    BatchResult, Parameter, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
pub use profilers::{
    Profiler, ProfilerResult, SequentialCountingProfiler, TemporalProfiler, TemporalProfiles,
    TimeSeries, TotalCounts,
};
pub use runner::{
    DEFAULT_MAX_BATCH_SIZE, OutputRecord, OutputValue, RunOutput, run_qir, run_qir_with_batch_size,
};
