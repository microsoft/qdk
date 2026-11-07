// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::QuantumBackendInstruction;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Default, Serialize)]
pub struct TotalCounts {
    /// Number of logical qubits, including factory qubits.
    pub logical_qubits: usize,

    /// Number of logical compute qubits, excluding factory qubits.
    pub logical_compute_qubits: usize,

    /// Number of physical qubits used by program.
    pub physical_qubits: usize,

    /// Estimated runtime of the program.
    pub total_runtime_sec: f64,

    /// Total number of operations.
    pub total_operations_count: usize,

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
///
/// It's responsibility of the profiler to compute these and provide correct units.
/// UI will display them as is without any post-processing.
/// If profiler uses window (sliding or tumbling), rates should be computed as
///   "number events per window / window size".
/// Profiler may also report gate counts per aggregations window, e.g. "X gates per window", but
///    that metric will depend on window size, so it's not recommended.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimeSeries {
    pub metric_name: String,
    pub metric_units: String,
    /// Display group for related metrics.
    pub group_name: String,
    /// Pairs (time in seconds, metric value).
    pub series: Vec<(f64, f64)>,
}

/// Time series reporting metrics per time.
#[derive(Clone, Debug, PartialEq, Default, Serialize)]
pub struct TemporalProfiles {
    pub profiles: Vec<TimeSeries>,

    /// Agregation window used for temporal profiles.
    pub aggregation_window_sec: Option<f64>,
}

/// For every instruction, records start and end time of that instruction.
#[derive(Clone, PartialEq, Serialize)]
pub struct InstructionTrace {
    pub start_time_nanos: usize,
    pub end_time_nanos: usize,
    pub instruction: QuantumBackendInstruction,
}

impl fmt::Debug for InstructionTrace {
    /// Formats the execution interval followed by the instruction.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}: {:?}",
            self.start_time_nanos, self.end_time_nanos, self.instruction
        )
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub struct ExecutionTrace {
    pub(super) instructions: Vec<InstructionTrace>,
}

impl fmt::Debug for ExecutionTrace {
    /// Formats instructions in trace order, separated by newlines.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, instruction) in self.instructions.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{instruction:?}")?;
        }
        Ok(())
    }
}
