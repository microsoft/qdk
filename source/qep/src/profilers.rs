// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod aggregators;
mod counting;
mod factories;
mod temporal;

use crate::{QuantumBackend, QuantumBackendInstruction, run_qir, run_qsharp_call, run_qsharp_expr};
use qsc::interpret::{Interpreter, Value};
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::{
    fmt,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub use aggregators::TumblingWindowAggregator;
pub use counting::SequentialCountingProfiler;
pub use temporal::TemporalProfiler;

#[derive(Clone, Copy)]
pub struct ProfilerConfig {
    pub aggregation_window_nanos: usize,
    pub t_factories_count: usize,
    /// Seed for temporal-profiler distillation and measurement randomness.
    pub seed: Option<u64>,
    /// Records temporal instruction and factory-round intervals when true.
    /// Disabling this leaves metrics unchanged and returns no execution trace.
    pub record_trace: bool,

    /// Distanceof the error-coorecting code.
    /// Assumes that all logical qubits have the same error correcting code.
    /// TODO: change it to specify different ECCs for different qubits.
    pub ecc_distance: usize,
}

impl Default for ProfilerConfig {
    /// Uses ten factories, one-second windows, unseeded randomness, and no trace recording.
    fn default() -> Self {
        Self {
            aggregation_window_nanos: 1_000_000_000,
            t_factories_count: 10,
            seed: None,
            record_trace: false,
            ecc_distance: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize)]
pub struct TotalCounts {
    /// Number of logical qubits used by the program.
    pub logical_qubits: usize,

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
    instructions: Vec<InstructionTrace>,
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

/// Profiling data collected during quantum execution.
#[derive(Clone, PartialEq, Serialize)]
pub struct ProfilerResult {
    /// Total counts.
    pub totals: TotalCounts,

    /// Temporal profiles.
    pub profiles: TemporalProfiles,

    /// Full execution trace.
    /// This can be huge, so can be omitted.
    pub trace: Option<ExecutionTrace>,

    /// Informational messages about assumptions made while profiling.
    pub infos: Vec<String>,
}

impl fmt::Debug for ProfilerResult {
    /// Summarizes profiling data without dumping trace instructions or time-series points.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = f.debug_struct("ProfilerResult");
        result
            .field("totals", &self.totals)
            .field("time_series_count", &self.profiles.profiles.len())
            .field(
                "aggregation_window_sec",
                &self.profiles.aggregation_window_sec,
            );
        if let Some(trace) = &self.trace
            && !trace.instructions.is_empty()
        {
            result.field("trace_operations_count", &trace.instructions.len());
        }
        result.field("infos", &self.infos).finish()
    }
}

impl ProfilerResult {
    /// Writes all result fields as pretty-printed JSON, creating or replacing the file.
    ///
    /// Instruction parameters retain their variant names, and absent traces are
    /// serialized as `null`. Parent directories must already exist.
    /// Returns a path-qualified error if creation, serialization, or flushing fails.
    pub fn to_json(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let file = File::create(path)
            .map_err(|error| format!("Failed to create {}: {error}", path.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, self).map_err(|error| {
            format!("Failed to serialize result to {}: {error}", path.display())
        })?;
        writer
            .flush()
            .map_err(|error| format!("Failed to flush {}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Boxed trait objects execute instructions and return each profiler's collected data.
    #[test]
    fn boxed_profilers() -> Result<(), String> {
        let profilers: [Box<dyn Profiler + Send + Sync>; 2] = [
            Box::new(SequentialCountingProfiler::new(
                10,
                FxHashMap::from_iter([("x".into(), 1.0)]),
            )?),
            Box::new(TemporalProfiler::new(ProfilerConfig::default())),
        ];
        for mut profiler in profilers {
            assert_eq!(profiler.get_result().totals.logical_qubits, 0);
            profiler.execute_batch(&[QuantumBackendInstruction {
                instruction_name: "x",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
            }])?;
            let result = profiler.get_result();
            assert_eq!(result.totals.logical_qubits, 1);
            assert!(result.totals.total_operations_count > 0);
            assert!(result.totals.total_runtime_sec > 0.0);
        }
        Ok(())
    }

    /// Omits the trace count for both absent and empty traces.
    #[test]
    fn profiler_result_debug_without_trace_operations() {
        let mut result = ProfilerResult {
            totals: TotalCounts::default(),
            profiles: TemporalProfiles::default(),
            trace: None,
            infos: Vec::new(),
        };
        let expected = format!(
            "ProfilerResult {{ totals: {:?}, time_series_count: 0, aggregation_window_sec: None, infos: [] }}",
            result.totals
        );
        assert_eq!(format!("{result:?}"), expected);
        result.trace = Some(ExecutionTrace {
            instructions: Vec::new(),
        });
        assert_eq!(format!("{result:?}"), expected);
    }

    /// Counts trace entries and metrics while preserving totals and metadata.
    #[test]
    fn profiler_result_debug_with_trace_and_profiles() {
        let result = ProfilerResult {
            totals: TotalCounts {
                logical_qubits: 2,
                physical_qubits: 20,
                total_runtime_sec: 0.5,
                total_operations_count: 3,
                ..TotalCounts::default()
            },
            profiles: TemporalProfiles {
                profiles: vec![
                    TimeSeries {
                        metric_name: "hidden metric".into(),
                        metric_units: "events/s".into(),
                        series: vec![(0.0, 123.0), (1.0, 456.0)],
                    };
                    2
                ],
                aggregation_window_sec: Some(1.0),
            },
            trace: Some(ExecutionTrace {
                instructions: vec![
                    InstructionTrace {
                        start_time_nanos: 0,
                        end_time_nanos: 5,
                        instruction: QuantumBackendInstruction {
                            instruction_name: "hidden_instruction",
                            parameters: smallvec::smallvec![],
                        },
                    };
                    4
                ],
            }),
            infos: vec!["profiling information".into()],
        };
        assert_eq!(
            format!("{result:?}"),
            format!(
                "ProfilerResult {{ totals: {:?}, time_series_count: 2, aggregation_window_sec: Some(1.0), trace_operations_count: 4, infos: [\"profiling information\"] }}",
                result.totals
            )
        );
        let pretty = format!("{result:#?}");
        assert!(pretty.contains("trace_operations_count: 4"));
        assert!(!pretty.contains("hidden metric"));
        assert!(!pretty.contains("hidden_instruction"));
    }
}

/// A quantum backend that reports profiling data after execution.
///
/// Boxed profilers, including trait objects, support the execution helpers below.
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

    /// Invokes a Q# callable and returns the profiling data collected afterward.
    fn profile_qsharp(
        &mut self,
        interpreter: &mut Interpreter,
        callable: Value,
        args: Value,
    ) -> Result<ProfilerResult, String>
    where
        Self: Sized,
    {
        run_qsharp_call(interpreter, callable, args, self).map_err(|error| error.to_string())?;
        Ok(self.get_result())
    }

    /// Runs a Q# expression and returns the profiling data collected afterward.
    fn profile_qsharp_expr(
        &mut self,
        interpreter: &mut Interpreter,
        expr: &str,
    ) -> Result<ProfilerResult, String>
    where
        Self: Sized,
    {
        run_qsharp_expr(interpreter, expr, self).map_err(|error| error.to_string())?;
        Ok(self.get_result())
    }
}
