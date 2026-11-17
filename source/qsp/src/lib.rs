// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum instruction interfaces, stream serialization, and execution utilities.

#![forbid(unsafe_code)]

pub mod instructions;
pub mod interface;
pub mod serde;
pub mod utils;

pub use instructions::{
    InstructionId, OPERATION_COUNT, instruction_id_to_name, instruction_name_to_id,
    validate_instruction,
};
pub use interface::{
    BatchResult, Driver, Parameter, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
pub use serde::{StreamBackend, StreamDriver};
pub use utils::{
    adapters::{ResultAdapter, TranspilingAdapter},
    backends::{QsBackendShim, QuantumBackends},
    batcher::{Batcher, DEFAULT_MAX_BATCH_SIZE},
    runners::{
        OutputRecord, OutputValue, QsharpRunError, RunOutput, run_adaptive_program,
        run_qsharp_call, run_qsharp_expr,
    },
    transpilers::{
        CliffordTranspiler, MzResetTranspiler, RzQftTranspiler, RzTranspiler, SwapTransplier,
        Transpiler,
    },
};
