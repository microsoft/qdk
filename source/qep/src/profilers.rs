// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod counting;

use crate::QuantumBackend;
use std::collections::HashMap;

pub use counting::SequentialCountingProfiler;

/// Profiling data collected during quantum execution.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfilerResult {
    /// Number of logical qubits used by the program.
    pub logical_qubits: u64,

    /// Number of physical qubits used by program.
    pub physical_qubits: u64,

    /// Estimated runtime of the program.
    pub total_runtime_sec: f64,

    /// For each operation, number of calls.
    pub calls_per_operation: HashMap<String, u64>,

    /// For each operation, time spent in that operation.
    pub runtime_time_sec_per_operation: HashMap<String, f64>,

    /// Informational messages about assumptions made while profiling.
    pub infos: Vec<String>,
}

/// A quantum backend that reports profiling data after execution.
pub trait Profiler: QuantumBackend {
    /// Returns the profiling data collected so far.
    fn get_result(&self) -> ProfilerResult;
}
