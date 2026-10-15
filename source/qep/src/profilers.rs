// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod counting;

use crate::{QuantumBackend, run_qir};
use rustc_hash::FxHashMap;

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
    pub calls_per_operation: FxHashMap<String, u64>,

    /// For each operation, time spent in that operation.
    pub runtime_sec_per_operation: FxHashMap<String, f64>,

    /// Informational messages about assumptions made while profiling.
    pub infos: Vec<String>,
}

/// A quantum backend that reports profiling data after execution.
#[allow(async_fn_in_trait)]
pub trait Profiler: QuantumBackend {
    /// Returns the profiling data collected so far.
    fn get_result(&self) -> ProfilerResult;

    /// Executes a QIR file and returns the profiling data collected afterward.
    ///
    /// # Safety
    ///
    /// The caller must satisfy the safety requirements of [`run_qir`].
    async unsafe fn profile_qir(&mut self, qir_file_name: &str) -> Result<ProfilerResult, String>
    where
        Self: Sized,
    {
        // SAFETY: The caller accepts the safety contract inherited from `run_qir`.
        unsafe { run_qir(qir_file_name, self).await? };
        Ok(self.get_result())
    }
}
