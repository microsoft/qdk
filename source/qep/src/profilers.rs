// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod aggregators;
mod counting;
mod event_recorder;
mod factories;
mod models;
mod profile_types;
mod temporal;
mod temporal2;

use crate::{QuantumBackend, run_adaptive_program, run_qir, run_qsharp_call, run_qsharp_expr};
use qdk_simulators::bytecode::AdaptiveProgram;
use qsc::interpret::{Interpreter, Value};
use serde::Serialize;
use std::{
    fmt,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub use aggregators::{AggregationType, TumblingWindowAggregator};
pub use counting::SequentialCountingProfiler;
pub use profile_types::{
    ExecutionTrace, InstructionTrace, TemporalProfiles, TimeSeries, TotalCounts,
};
pub use temporal::{TemporalProfiler, TemporalProfilerConfig};

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
    use crate::QuantumBackendInstruction;
    use rustc_hash::FxHashMap;

    /// Boxed trait objects execute instructions and return each profiler's collected data.
    #[test]
    fn boxed_profilers() -> Result<(), String> {
        let profilers: [Box<dyn Profiler + Send + Sync>; 2] = [
            Box::new(SequentialCountingProfiler::new(
                10,
                FxHashMap::from_iter([("x".into(), 1.0)]),
            )?),
            Box::new(TemporalProfiler::new(TemporalProfilerConfig::default())),
        ];
        for (mut profiler, expected_runtime) in profilers.into_iter().zip([1.0, 0.0]) {
            assert_eq!(profiler.get_result().totals.logical_compute_qubits, 0);
            profiler.execute_batch(&[QuantumBackendInstruction {
                instruction_name: "x",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
            }])?;
            let result = profiler.get_result();
            assert_eq!(result.totals.logical_compute_qubits, 1);
            let json = serde_json::to_value(&result).expect("profile should serialize");
            assert_eq!(json["totals"]["logical_compute_qubits"], 1);
            assert!(result.totals.total_operations_count > 0);
            assert_eq!(result.totals.total_runtime_sec, expected_runtime);
        }
        Ok(())
    }

    /// Profiles lowered bytecode through both boxed profiler implementations.
    #[test]
    fn boxed_profilers_profile_adaptive_program() -> Result<(), String> {
        use qdk_simulators::bytecode::{Block, Instruction, Op};

        let program = AdaptiveProgram {
            num_qubits: 1,
            num_results: 0,
            num_registers: 0,
            entry_block: 0,
            instructions: vec![
                Instruction {
                    opcode: 0x10 | (1 << 16) | (1 << 20) | (1 << 21),
                    ..Default::default()
                },
                Instruction {
                    opcode: 0x02 | (1 << 18),
                    ..Default::default()
                },
            ],
            block_table: vec![Block::from_tuple((0, 2))],
            function_table: vec![],
            phi_entries: vec![],
            switch_cases: vec![],
            call_args: vec![],
            constant_data: vec![],
            quantum_ops: vec![Op::from_tuple((2, 0, 0, 0, 0))],
        };
        let profilers: [Box<dyn Profiler + Send + Sync>; 2] = [
            Box::new(SequentialCountingProfiler::new(
                10,
                FxHashMap::from_iter([("x".into(), 0.25)]),
            )?),
            Box::new(TemporalProfiler::new(TemporalProfilerConfig::default())),
        ];
        for (mut profiler, expected_runtime) in profilers.into_iter().zip([0.25, 0.0]) {
            let result = profiler.profile_adaptive_program(&program)?;
            assert_eq!(result.totals.logical_compute_qubits, 1);
            assert!(result.totals.total_operations_count > 0);
            assert_eq!(result.totals.total_runtime_sec, expected_runtime);
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
                logical_compute_qubits: 2,
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
                        group_name: "Event rates".into(),
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
                        instruction: profile_types::LoggedInstruction {
                            instruction_name: "hidden_instruction".into(),
                            parameters: vec![],
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

    /// Executes one shot of lowered adaptive QIR and returns accumulated profiling data.
    ///
    /// Inherits the well-formed bytecode requirements and error behavior of
    /// [`run_adaptive_program`]. Program output is discarded; prior profile data
    /// is retained, and backend changes are not rolled back after an error.
    fn profile_adaptive_program(
        &mut self,
        program: &AdaptiveProgram<u64>,
    ) -> Result<ProfilerResult, String>
    where
        Self: Sized,
    {
        run_adaptive_program(program, self)?;
        Ok(self.get_result())
    }
}
