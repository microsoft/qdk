// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult};
use crate::{
    InstructionId, QuantumBackendResult,
    instructions::{READ_RESULT, instruction_id_to_name, instruction_name_to_id},
    interface::{BatchResult, QuantumBackend, QuantumInstruction},
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Profiles instructions as if every operation executes sequentially.
pub struct SequentialCountingProfiler {
    /// The physical-qubit overhead applied to each observed logical qubit.
    physical_qubits_per_logical: u64,
    /// The duration of one call, keyed by the stable QIR instruction name.
    gate_time_sec: FxHashMap<String, f64>,
    /// Call counts indexed by IDs up to the highest configured timing.
    counts: Vec<u64>,
    /// Executed IDs beyond the configured counter range, reported as missing timings.
    unexpected_instructions: FxHashSet<InstructionId>,
    /// The largest logical qubit ID observed during execution.
    max_qubit_id: Option<usize>,
}

impl SequentialCountingProfiler {
    /// Creates a profiler, treating omitted instruction timings as zero.
    ///
    /// Returns an error for unknown names, invalid durations, or allocation failures.
    pub fn new(
        physical_qubits_per_logical: u64,
        gate_time_sec: FxHashMap<String, f64>,
    ) -> Result<Self, String> {
        let mut count_len = 0;
        for (name, duration) in &gate_time_sec {
            let instruction_id = instruction_name_to_id(name)
                .ok_or_else(|| format!("unknown QIR instruction name `{name}`"))?;
            if !duration.is_finite() || *duration < 0.0 {
                return Err(format!(
                    "gate timing for instruction `{name}` must be finite and non-negative"
                ));
            }
            count_len = count_len.max(
                instruction_id
                    .checked_add(1)
                    .ok_or("Instruction counter length overflow")?,
            );
        }
        let mut counts = Vec::new();
        counts
            .try_reserve_exact(count_len)
            .map_err(|error| format!("Cannot allocate instruction counters: {error}"))?;
        counts.resize(count_len, 0);
        Ok(Self {
            physical_qubits_per_logical,
            gate_time_sec,
            counts,
            unexpected_instructions: FxHashSet::default(),
            max_qubit_id: None,
        })
    }

    /// Updates the largest logical qubit ID referenced by an instruction.
    fn observe_qubits(&mut self, instruction: &QuantumInstruction) {
        let instruction_max = instruction.targets.iter().copied().max();

        if let Some(qubit) = instruction_max {
            self.max_qubit_id = Some(
                self.max_qubit_id
                    .map_or(qubit, |current| current.max(qubit)),
            );
        }
    }
}

impl Profiler for SequentialCountingProfiler {
    /// Computes counts, per-operation runtimes, and total resource estimates.
    fn get_result(&self) -> ProfilerResult {
        let mut calls_per_operation =
            FxHashMap::with_capacity_and_hasher(self.counts.len(), Default::default());
        let mut runtime_sec_per_operation =
            FxHashMap::with_capacity_and_hasher(self.counts.len(), Default::default());
        let mut missing_timings = Vec::new();
        let mut total_runtime_sec = 0.0;

        for (id, &calls) in self.counts.iter().enumerate() {
            if calls == 0 {
                continue;
            }
            let name = instruction_id_to_name(id);
            let gate_time_sec = self.gate_time_sec.get(name).copied().unwrap_or_else(|| {
                missing_timings.push(name);
                0.0
            });
            #[allow(clippy::cast_precision_loss)] // Profiling runtimes are represented as f64.
            let runtime = calls as f64 * gate_time_sec;
            total_runtime_sec += runtime;
            calls_per_operation.insert(name.to_string(), calls);
            runtime_sec_per_operation.insert(name.to_string(), runtime);
        }
        let mut unexpected: Vec<_> = self.unexpected_instructions.iter().copied().collect();
        unexpected.sort_unstable();
        missing_timings.extend(unexpected.into_iter().map(instruction_id_to_name));
        let infos = if missing_timings.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "These gates did not have time configured, assumed 0: [{}]",
                missing_timings.join(", ")
            )]
        };

        let logical_qubits = self.max_qubit_id.map_or(0, |id| {
            u64::try_from(id)
                .expect("logical qubit ID should fit in u64")
                .checked_add(1)
                .expect("logical qubit count overflow")
        });
        let physical_qubits = logical_qubits * self.physical_qubits_per_logical;

        ProfilerResult {
            logical_qubits,
            physical_qubits,
            total_runtime_sec,
            calls_per_operation,
            runtime_sec_per_operation,
            infos,
        }
    }
}

impl QuantumBackend for SequentialCountingProfiler {
    /// Counts named QIR instructions and returns one value per requested read.
    fn execute_batch(&mut self, instructions: &[QuantumInstruction]) -> BatchResult {
        let mut result = BatchResult::default();
        for instruction in instructions {
            self.observe_qubits(instruction);
            if let Some(count) = self.counts.get_mut(instruction.instruction_id) {
                *count += 1;
            } else {
                self.unexpected_instructions
                    .insert(instruction.instruction_id);
            }

            if instruction.instruction_id == READ_RESULT {
                result
                    .results
                    .push(QuantumBackendResult::from(rand::random::<bool>()));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::SequentialCountingProfiler;
    use crate::Profiler;
    use crate::tests::compile_qsharp_to_qir;
    use crate::{QuantumBackend, QuantumBackendResult};
    use qsc::target::Profile;
    use rustc_hash::FxHashMap;

    #[test]
    /// Returns configuration errors rather than panicking on invalid names or durations.
    fn rejects_invalid_timings() {
        for (name, duration, expected) in [
            ("unknown", 1.0, "unknown QIR instruction name"),
            ("h", -1.0, "finite and non-negative"),
            ("h", f64::NAN, "finite and non-negative"),
            ("h", f64::INFINITY, "finite and non-negative"),
            ("h", f64::NEG_INFINITY, "finite and non-negative"),
        ] {
            let error =
                SequentialCountingProfiler::new(1, FxHashMap::from_iter([(name.into(), duration)]))
                    .err()
                    .expect("invalid timings should fail");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    /// Counts sparse in-range IDs and deduplicates out-of-range warnings in ID order.
    fn sizes_counters_from_timings_and_reports_missing_ids() -> Result<(), String> {
        use crate::instructions::{H, T};
        let mut profiler = SequentialCountingProfiler::new(
            1,
            FxHashMap::from_iter([("h".into(), 0.25), ("t".into(), 0.5)]),
        )?;
        assert_eq!(profiler.counts.len(), T + 1);
        assert!(profiler.counts.iter().all(|&count| count == 0));
        profiler.execute_batch(&[
            crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::CX,
                targets: smallvec::smallvec![0, 1],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::CX,
                targets: smallvec::smallvec![0, 1],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(99)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(99)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(99)],
            },
        ]);
        assert_eq!(profiler.counts[H], 1);
        assert_eq!(profiler.unexpected_instructions.len(), 2);
        let result = profiler.get_result();
        assert_eq!(
            result.calls_per_operation,
            FxHashMap::from_iter([("cx".into(), 2), ("h".into(), 1)])
        );
        assert_eq!(
            result.runtime_sec_per_operation,
            FxHashMap::from_iter([("cx".into(), 0.0), ("h".into(), 0.25)])
        );
        assert_eq!(
            result.infos,
            ["These gates did not have time configured, assumed 0: [cx, mz, read_result]"]
        );
        assert!((result.total_runtime_sec - 0.25).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    /// Uses no counters for an empty configuration while retaining warnings and qubit accounting.
    fn empty_timings_track_unexpected_instructions() -> Result<(), String> {
        let mut profiler = SequentialCountingProfiler::new(1, FxHashMap::default())?;
        assert!(profiler.counts.is_empty());
        assert!(profiler.get_result().infos.is_empty());
        profiler.execute_batch(&[
            crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![4],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![4],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(profiler.unexpected_instructions.len(), 1);
        let result = profiler.get_result();
        assert!(result.calls_per_operation.is_empty());
        assert!(result.runtime_sec_per_operation.is_empty());
        assert_eq!(result.logical_qubits, 5);
        assert_eq!(
            result.infos,
            ["These gates did not have time configured, assumed 0: [h]"]
        );
        Ok(())
    }

    #[test]
    /// Counts named instructions without treating classical result IDs as qubits.
    fn counts_named_batches_and_keeps_result_ids_classical() {
        let mut profiler =
            SequentialCountingProfiler::new(1, FxHashMap::from_iter([("read_result".into(), 0.0)]))
                .expect("valid timing configuration");
        let batch = [
            crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![2],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![2],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                    1000
                )],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                    1000
                )],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MRESETZ,
                targets: smallvec::smallvec![2],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                    2000
                )],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                    2000
                )],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::RESET,
                targets: smallvec::smallvec![2],
                classical_parameters: smallvec::smallvec![],
            },
        ];

        let result = profiler.execute_batch(&batch);
        assert_eq!(result.results.len(), 2);
        assert!(result.results.iter().all(|value| matches!(
            value,
            QuantumBackendResult::Zero | QuantumBackendResult::One
        )));
        let counts = profiler.get_result();
        assert_eq!(counts.logical_qubits, 3);
        assert_eq!(counts.calls_per_operation["read_result"], 2);
        assert_eq!(counts.calls_per_operation["mz"], 1);
        assert_eq!(counts.calls_per_operation["mresetz"], 1);

        let result = profiler.execute_batch(&[crate::QuantumInstruction {
            instruction_id: crate::instructions::READ_RESULT,
            targets: smallvec::smallvec![],
            classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(2000)],
        }]);
        assert_eq!(result.results.len(), 1);
        assert!(!result.results.spilled());
        assert_eq!(profiler.counts[crate::instructions::READ_RESULT], 3);
    }

    #[test]
    /// Verifies instruction counts and sequential resource estimates.
    fn profiles_instructions_sequentially() -> Result<(), String> {
        let source = r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit[2];
                    H(q[0]);
                    H(q[1]);
                    [M(q[0]), M(q[1])]
                }
            }
        "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let timings = FxHashMap::from_iter([
            ("h".to_string(), 0.25),
            ("mz".to_string(), 0.5),
            ("read_result".to_string(), 0.125),
        ]);
        let mut profiler = SequentialCountingProfiler::new(10, timings)?;

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let result = unsafe { profiler.profile_qir(file.path()?) }?;

        assert_eq!(result.logical_qubits, 2);
        assert_eq!(result.physical_qubits, 20);
        assert_eq!(
            result.calls_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 2),
                ("mz".to_string(), 2),
                ("read_result".to_string(), 2),
            ])
        );
        assert_eq!(
            result.runtime_sec_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 0.5),
                ("mz".to_string(), 1.0),
                ("read_result".to_string(), 0.25),
            ])
        );
        assert!((result.total_runtime_sec - 1.75).abs() < f64::EPSILON);
        assert!(result.infos.is_empty());
        Ok(())
    }

    #[test]
    /// Reports one message listing executed gates whose timings were omitted.
    fn reports_missing_gate_timings() {
        let timings = FxHashMap::from_iter([("h".to_string(), 0.25)]);
        let mut profiler = SequentialCountingProfiler::new(10, timings).expect("valid timings");
        profiler.execute_batch(&[
            crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
        ]);

        let result = profiler.get_result();

        assert_eq!(
            result.infos,
            vec![
                "These gates did not have time configured, assumed 0: [mz, read_result]"
                    .to_string()
            ]
        );
        assert_eq!(
            result.calls_per_operation,
            FxHashMap::from_iter([("h".to_string(), 1),])
        );
        assert_eq!(
            result.runtime_sec_per_operation,
            FxHashMap::from_iter([("h".to_string(), 0.25),])
        );
    }
}
