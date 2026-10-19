// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod counting;
mod temporal;

use crate::{QuantumBackend, run_qir};
use rustc_hash::FxHashMap;

pub use counting::SequentialCountingProfiler;
pub use temporal::TemporalProfiler;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TotalCounts {
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
}

/// Examples:
///   - "X gate rate", "gate/s"
///   - "T state generation rate", "state/s"
///   - "T state consumption rate", "state/s"
///   - "compute qubits used", "qubits" (how many qubits were used in this time window)
///   - "max qubit usage", "qubits" - max qubit usage at any point from execution start till now.
/// It's responsibility of the profiler to compute these and provide correct units.
/// UI will display them as is without any post-processing.
/// If profiler uses window (sliding or tumbling), rates should be computed as
///   "number events per window / window size".
/// Profiler may also report gate counts per aggregations window, e.g. "X gates per window", but
///    that metruic will depend on window size, so it's not recommended.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSeries {
    pub metric_name: String,
    pub metric_units: String,
    /// Pairs (time in seconds, metric value).
    pub series: Vec<(f64, f64)>,
}

/// Time series reporting metrics per time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TemporalProfiles {
    pub profiles: Vec<TimeSeries>,

    /// Agregation window used for temporal profiles.
    pub aggregation_window_sec: Option<f64>,
}

/// Profiling data collected during quantum execution.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfilerResult {
    /// Total counts.
    pub totals: TotalCounts,

    /// Temporal profiles.
    pub profiles: TemporalProfiles,

    /// Informational messages about assumptions made while profiling.
    pub infos: Vec<String>,
}

/// A quantum backend that reports profiling data after execution.
pub trait Profiler: QuantumBackend {
    /// Returns the profiling data collected so far.
    fn get_result(&self) -> ProfilerResult;

    /// Executes a QIR file and returns the profiling data collected afterward.
    ///
    /// # Safety
    ///
    /// The caller must satisfy the safety requirements of [`run_qir`].
    unsafe fn profile_qir(&mut self, qir_file_name: &str) -> Result<ProfilerResult, String>
    where
        Self: Sized,
    {
        // SAFETY: The caller accepts the safety contract inherited from `run_qir`.
        unsafe { run_qir(qir_file_name, self)? };
        Ok(self.get_result())
    }
}
