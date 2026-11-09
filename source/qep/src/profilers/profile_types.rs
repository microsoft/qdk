// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Parameter, QuantumBackendInstruction};
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::{borrow::Cow, fmt};

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

    /// Error - probability of getting incorrect result.
    pub error: f64,
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

#[derive(Clone, PartialEq, Serialize)]
pub enum LoggedInstructionParameter {
    Float(f64),
    ResultId(usize),
    QubitId(usize),
    FactoryId(usize),
}

#[derive(Clone, PartialEq, Serialize)]
pub struct LoggedInstruction {
    pub instruction_name: Cow<'static, str>,
    pub parameters: Vec<LoggedInstructionParameter>,
}

/// Provides an operation name for metrics and a detailed representation for traces.
pub trait LoggableOperation {
    /// Returns the operation name without constructing a trace entry.
    fn get_name(&self) -> &str;

    /// Builds the instruction representation recorded in execution traces.
    fn to_logged_instruction(&self) -> LoggedInstruction;
}

impl From<&QuantumBackendInstruction> for LoggedInstruction {
    /// Copies instruction data into the trace representation without cloning the backend instruction.
    fn from(instruction: &QuantumBackendInstruction) -> Self {
        Self {
            instruction_name: Cow::Borrowed(instruction.instruction_name),
            parameters: instruction
                .parameters
                .iter()
                .map(|parameter| match parameter {
                    Parameter::Float(value) => LoggedInstructionParameter::Float(*value),
                    Parameter::ResultId(value) => LoggedInstructionParameter::ResultId(*value),
                    Parameter::QubitId(value) => LoggedInstructionParameter::QubitId(*value),
                    Parameter::FactoryId(value) => LoggedInstructionParameter::FactoryId(*value),
                })
                .collect(),
        }
    }
}

impl From<QuantumBackendInstruction> for LoggedInstruction {
    /// Converts an owned backend instruction using the same trace mapping as borrowed instructions.
    fn from(instruction: QuantumBackendInstruction) -> Self {
        Self::from(&instruction)
    }
}

impl fmt::Debug for LoggedInstructionParameter {
    /// Formats the contained value without its parameter variant name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Float(value) => write!(f, "{value}"),
            Self::ResultId(value) | Self::QubitId(value) | Self::FactoryId(value) => {
                write!(f, "{value}")
            }
        }
    }
}

impl fmt::Debug for LoggedInstruction {
    /// Formats the instruction name followed by space-separated parameter values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.instruction_name)?;
        for parameter in &self.parameters {
            write!(f, " {parameter:?}")?;
        }
        Ok(())
    }
}

/// For every instruction, records start and end time of that instruction.
#[derive(Clone, PartialEq, Serialize)]
pub struct InstructionTrace {
    pub start_time_nanos: usize,
    pub end_time_nanos: usize,
    pub instruction: LoggedInstruction,
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

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    /// Keeps JSON and debug output identical for both conversions and every parameter variant.
    #[test]
    fn logged_instruction_preserves_backend_representation() {
        for parameters in [
            smallvec![],
            smallvec![
                Parameter::Float(-0.25),
                Parameter::ResultId(2),
                Parameter::QubitId(3),
                Parameter::FactoryId(4),
            ],
        ] {
            let instruction = QuantumBackendInstruction {
                instruction_name: "test",
                parameters,
            };
            let expected_json =
                serde_json::to_string(&instruction).expect("instruction serializes");
            let expected_debug = format!("{instruction:?}");
            let borrowed = LoggedInstruction::from(&instruction);
            let owned = LoggedInstruction::from(instruction);
            assert_eq!(borrowed, owned);
            for logged in [borrowed, owned] {
                assert!(matches!(logged.instruction_name, Cow::Borrowed("test")));
                assert_eq!(
                    serde_json::to_string(&logged).expect("logged instruction serializes"),
                    expected_json
                );
                assert_eq!(format!("{logged:?}"), expected_debug);
            }
        }
    }

    /// Directly constructed trace entries retain the original JSON shape and accept owned names.
    #[test]
    fn direct_logged_instruction_preserves_trace_json() {
        let trace = InstructionTrace {
            start_time_nanos: 0,
            end_time_nanos: 55_000,
            instruction: LoggedInstruction {
                instruction_name: Cow::Owned(String::from("round")),
                parameters: vec![LoggedInstructionParameter::FactoryId(7)],
            },
        };
        assert_eq!(
            serde_json::to_string(&trace).expect("trace serializes"),
            r#"{"start_time_nanos":0,"end_time_nanos":55000,"instruction":{"instruction_name":"round","parameters":[{"FactoryId":7}]}}"#
        );
        assert_eq!(format!("{trace:?}"), "0-55000: round 7");
    }
}
