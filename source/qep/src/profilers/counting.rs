// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult, TemporalProfiles, TotalCounts};
use crate::{
    QuantumBackendResult,
    instructions::{instruction_id_to_name, instruction_name_to_id, produces_result},
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
};
use rustc_hash::FxHashMap;

/// Profiles instructions as if every operation executes sequentially.
pub struct SequentialCountingProfiler {
    /// The physical-qubit overhead applied to each observed logical qubit.
    physical_qubits_per_logical: usize,
    /// The duration of one call, keyed by the stable QIR instruction name.
    gate_time_sec: FxHashMap<String, f64>,
    /// Call counts for every supported instruction, indexed by ID.
    counts: Vec<u64>,
    /// The largest logical qubit ID observed during execution.
    max_qubit_id: Option<usize>,
}

impl SequentialCountingProfiler {
    /// Creates a profiler, treating omitted instruction timings as zero.
    ///
    /// Returns an error for unknown names or invalid durations.
    pub fn new(
        physical_qubits_per_logical: usize,
        gate_time_sec: FxHashMap<String, f64>,
    ) -> Result<Self, String> {
        for (name, duration) in &gate_time_sec {
            instruction_name_to_id(name)
                .ok_or_else(|| format!("unknown QIR instruction name `{name}`"))?;
            if !duration.is_finite() || *duration < 0.0 {
                return Err(format!(
                    "gate timing for instruction `{name}` must be finite and non-negative"
                ));
            }
        }
        Ok(Self {
            physical_qubits_per_logical,
            gate_time_sec,
            counts: vec![0; usize::from(crate::OPERATION_COUNT)],
            max_qubit_id: None,
        })
    }

    /// Updates the largest logical qubit ID referenced by an instruction.
    fn observe_qubits(&mut self, instruction: &QuantumBackendInstruction) {
        let instruction_max = instruction
            .parameters
            .iter()
            .filter_map(|parameter| match parameter {
                crate::Parameter::QubitId(qubit) => Some(*qubit),
                _ => None,
            })
            .max();

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

        for (id, &calls) in (0..crate::OPERATION_COUNT).zip(&self.counts) {
            if calls == 0 {
                continue;
            }
            let name = instruction_id_to_name(id);
            let gate_time_sec = if let Some(&duration) = self.gate_time_sec.get(name) {
                calls_per_operation.insert(name.to_string(), calls);
                duration
            } else {
                missing_timings.push(name);
                0.0
            };
            #[allow(clippy::cast_precision_loss)] // Profiling runtimes are represented as f64.
            let runtime = calls as f64 * gate_time_sec;
            total_runtime_sec += runtime;
            runtime_sec_per_operation.insert(name.to_string(), runtime);
        }
        let infos = if missing_timings.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "These gates did not have time configured, assumed 0: [{}]",
                missing_timings.join(", ")
            )]
        };

        let logical_qubits = self.max_qubit_id.map_or(0, |id| id + 1);
        let physical_qubits = logical_qubits * self.physical_qubits_per_logical;
        let total_operations_count = usize::try_from(calls_per_operation.values().sum::<u64>())
            .expect("total operation count should fit in usize");

        ProfilerResult {
            totals: TotalCounts {
                logical_qubits,
                logical_compute_qubits: logical_qubits,
                physical_qubits,
                total_runtime_sec,
                total_operations_count,
                calls_per_operation,
                runtime_sec_per_operation,
                error: 0.0,
            },
            profiles: TemporalProfiles::default(),
            trace: None,
            infos,
        }
    }
}

impl QuantumBackend for SequentialCountingProfiler {
    /// Counts instructions and returns random bits for reads and immediate measurements.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut result = BatchResult::default();
        for instruction in instructions {
            if instruction.instruction_id >= crate::OPERATION_COUNT {
                return Err(format!(
                    "Unknown instruction ID {}",
                    instruction.instruction_id
                ));
            }
            self.observe_qubits(instruction);
            self.counts[usize::from(instruction.instruction_id)] += 1;

            if produces_result(instruction) {
                result
                    .results
                    .push(QuantumBackendResult::from(rand::random::<bool>()));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::SequentialCountingProfiler;
    use crate::tests::{create_qsharp_interpreter, qsharp_sources_with_arithmetic};
    use crate::{InstructionId, Profiler, instructions};
    use crate::{QuantumBackend, QuantumBackendResult};
    use rustc_hash::FxHashMap;
    use std::time::Instant;

    /// Configures one-microsecond timings for all instructions used by the profiling workloads.
    fn create_profiler() -> SequentialCountingProfiler {
        SequentialCountingProfiler::new(
            1,
            [
                "ccx",
                "cx",
                "cz",
                "rz",
                "h",
                "x",
                "swap",
                "reset",
                "mz",
                "mz_reset",
                "mz_store",
                "mz_store_reset",
                "read_result",
            ]
            .into_iter()
            .map(|name| (name.to_string(), 1e-6))
            .collect(),
        )
        .expect("profiling workload timings should be valid")
    }

    const MOD_MUL_SOURCE: &str = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let n = 500;
                let modulus = (1L <<< n) - 1L;
                use xs = Qubit[n];
                use ys = Qubit[n];
                use ans = Qubit[n];
                ModMul.ModMul(xs, ys, ans, modulus);
            }
        }
    "#;

    #[test]
    /// Profiles modular multiplication directly through Q# and reports the execution time.
    fn profiles_mod_mul_qsharp() -> Result<(), String> {
        if cfg!(debug_assertions) {
            return Ok(()); // Skip in debug mode
        }

        let mut interpreter =
            create_qsharp_interpreter(qsharp_sources_with_arithmetic(MOD_MUL_SOURCE))?;
        let mut profiler = create_profiler();
        let start = Instant::now();

        let result = profiler.profile_qsharp_expr(&mut interpreter, "Test.Main()")?;
        let elapsed = start.elapsed();
        assert!(result.infos.is_empty(), "{:?}", result.infos);
        let operation_count: u64 = result.totals.calls_per_operation.values().sum();

        assert!(operation_count > 0);
        let ns_per_operation = elapsed.as_nanos() / u128::from(operation_count);
        println!(
            "Profiled Q# modular multiplication without QIR: {operation_count} operations in {elapsed:?} \
             ({ns_per_operation} ns/op)"
        );
        // Other gate counts vary with the profiler's random measurement results.
        for (name, expected) in [
            ("mz_store", 500),
            ("mz_store_reset", 996_003),
            ("read_result", 996_503),
            ("swap", 249_500),
        ] {
            assert_eq!(
                result.totals.calls_per_operation.get(name),
                Some(&expected),
                "unexpected instruction count for {name}"
            );
        }
        Ok(())
    }

    #[test]
    /// Counts and times immediate measurements while preserving stored-measurement output counts.
    fn profiles_immediate_and_stored_measurements() -> Result<(), String> {
        use crate::{Parameter::ResultId, QuantumBackendInstruction};

        let instructions: Vec<_> = [
            (instructions::MZ, Some(4), None),
            (instructions::MZ_RESET, Some(4), None),
            (instructions::MZ_STORE, Some(4), Some(1000)),
            (instructions::MZ_STORE_RESET, Some(4), Some(1001)),
            (instructions::READ_RESULT, None, Some(1000)),
            (instructions::READ_RESULT, None, Some(1001)),
        ]
        .into_iter()
        .map(
            |(instruction_id, target, result)| QuantumBackendInstruction {
                instruction_id,
                parameters: target
                    .into_iter()
                    .map(crate::Parameter::QubitId)
                    .chain(result.map(ResultId))
                    .collect(),
            },
        )
        .collect();
        let timings = [
            "mz",
            "mz_reset",
            "mz_store",
            "mz_store_reset",
            "read_result",
        ]
        .into_iter()
        .map(|name| (name.to_string(), 0.25))
        .collect();
        let mut profiler = SequentialCountingProfiler::new(2, timings)?;
        let batch = profiler.execute_batch(&instructions)?;
        assert_eq!(batch.results.len(), 4);
        assert!(batch.results.iter().all(|value| matches!(
            value,
            QuantumBackendResult::Zero | QuantumBackendResult::One
        )));

        let result = profiler.get_result();
        assert_eq!(result.totals.logical_qubits, 5);
        assert_eq!(result.totals.logical_compute_qubits, 5);
        assert_eq!(result.totals.physical_qubits, 10);
        assert!((result.totals.total_runtime_sec - 1.5).abs() < f64::EPSILON);
        assert!(result.infos.is_empty());
        assert_eq!(
            result.totals.calls_per_operation,
            FxHashMap::from_iter([
                ("mz".into(), 1),
                ("mz_reset".into(), 1),
                ("mz_store".into(), 1),
                ("mz_store_reset".into(), 1),
                ("read_result".into(), 2),
            ])
        );
        assert_eq!(
            result.totals.runtime_sec_per_operation,
            FxHashMap::from_iter([
                ("mz".into(), 0.25),
                ("mz_reset".into(), 0.25),
                ("mz_store".into(), 0.25),
                ("mz_store_reset".into(), 0.25),
                ("read_result".into(), 0.5),
            ])
        );

        let mut profiler = SequentialCountingProfiler::new(2, FxHashMap::default())?;
        assert_eq!(profiler.execute_batch(&instructions)?.results.len(), 4);
        let result = profiler.get_result();
        assert!(result.totals.calls_per_operation.is_empty());
        assert_eq!(
            result.infos,
            [
                "These gates did not have time configured, assumed 0: [mz_store, mz_store_reset, read_result, mz, mz_reset]"
            ]
        );
        Ok(())
    }

    #[test]
    /// Returns configuration errors rather than panicking on invalid names or durations.
    fn rejects_invalid_timings() {
        for (name, duration, expected) in [
            ("unknown", 1.0, "unknown QIR instruction name"),
            ("mresetz", 1.0, "unknown QIR instruction name"),
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
    /// Counts all instructions regardless of configured timings and warns once per untimed ID.
    fn sizes_counters_from_timings_and_reports_missing_ids() -> Result<(), String> {
        use crate::instructions::H;
        let mut profiler = SequentialCountingProfiler::new(
            1,
            FxHashMap::from_iter([("h".into(), 0.25), ("t".into(), 0.5)]),
        )?;
        assert_eq!(profiler.counts.len(), usize::from(crate::OPERATION_COUNT));
        assert!(profiler.counts.iter().all(|&count| count == 0));
        profiler.execute_batch(&[
            crate::QuantumBackendInstruction {
                instruction_id: instructions::H,
                parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::CX,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::QubitId(1)
                ],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::CX,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::QubitId(1)
                ],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::READ_RESULT,
                parameters: smallvec::smallvec![crate::Parameter::ResultId(99)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::MZ_STORE,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::ResultId(99)
                ],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::MZ_STORE,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::ResultId(99)
                ],
            },
        ])?;
        assert_eq!(profiler.counts[usize::from(H)], 1);
        let result = profiler.get_result();
        assert_eq!(
            result.totals.calls_per_operation,
            FxHashMap::from_iter([("h".into(), 1)])
        );
        assert_eq!(
            result.totals.runtime_sec_per_operation,
            FxHashMap::from_iter([
                ("cx".into(), 0.0),
                ("h".into(), 0.25),
                ("mz_store".into(), 0.0),
                ("read_result".into(), 0.0),
            ])
        );
        assert_eq!(
            result.infos,
            ["These gates did not have time configured, assumed 0: [cx, mz_store, read_result]"]
        );
        assert!((result.totals.total_runtime_sec - 0.25).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    /// Retains counts, warnings, and qubit accounting even without configured timings.
    fn empty_timings_track_unexpected_instructions() -> Result<(), String> {
        let mut profiler = SequentialCountingProfiler::new(1, FxHashMap::default())?;
        assert_eq!(profiler.counts.len(), usize::from(crate::OPERATION_COUNT));
        assert!(profiler.get_result().infos.is_empty());
        profiler.execute_batch(&[
            crate::QuantumBackendInstruction {
                instruction_id: instructions::H,
                parameters: smallvec::smallvec![crate::Parameter::QubitId(4)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::H,
                parameters: smallvec::smallvec![crate::Parameter::QubitId(4)],
            },
        ])?;
        let result = profiler.get_result();
        assert!(result.totals.calls_per_operation.is_empty());
        assert_eq!(
            result.totals.runtime_sec_per_operation,
            FxHashMap::from_iter([("h".into(), 0.0)])
        );
        assert_eq!(result.totals.logical_qubits, 5);
        assert_eq!(
            result.infos,
            ["These gates did not have time configured, assumed 0: [h]"]
        );
        Ok(())
    }

    #[test]
    /// Rejects unknown IDs before they can reach name conversion in the profile report.
    fn rejects_unknown_instruction_ids() -> Result<(), String> {
        let mut profiler = SequentialCountingProfiler::new(1, FxHashMap::default())?;
        for instruction_id in [crate::OPERATION_COUNT, InstructionId::MAX] {
            let error = profiler
                .execute_batch(&[crate::QuantumBackendInstruction {
                    instruction_id,
                    parameters: smallvec::smallvec![],
                }])
                .expect_err("unknown instruction ID");
            assert_eq!(error, format!("Unknown instruction ID {instruction_id}"));
        }
        assert!(profiler.get_result().infos.is_empty());
        Ok(())
    }

    #[test]
    /// Counts instructions by ID without treating classical result IDs as qubits.
    fn counts_batches_and_keeps_result_ids_classical() {
        let mut profiler =
            SequentialCountingProfiler::new(1, FxHashMap::from_iter([("read_result".into(), 0.0)]))
                .expect("valid timing configuration");
        let batch = [
            crate::QuantumBackendInstruction {
                instruction_id: instructions::H,
                parameters: smallvec::smallvec![crate::Parameter::QubitId(2)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::MZ_STORE,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(2),
                    crate::Parameter::ResultId(1000)
                ],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::READ_RESULT,
                parameters: smallvec::smallvec![crate::Parameter::ResultId(1000)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::MZ_STORE_RESET,
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(2),
                    crate::Parameter::ResultId(2000)
                ],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::READ_RESULT,
                parameters: smallvec::smallvec![crate::Parameter::ResultId(2000)],
            },
            crate::QuantumBackendInstruction {
                instruction_id: instructions::RESET,
                parameters: smallvec::smallvec![crate::Parameter::QubitId(2)],
            },
        ];

        let result = profiler
            .execute_batch(&batch)
            .expect("batch should succeed");
        assert_eq!(result.results.len(), 2);
        assert!(result.results.iter().all(|value| matches!(
            value,
            QuantumBackendResult::Zero | QuantumBackendResult::One
        )));
        let counts = profiler.get_result();
        assert_eq!(counts.totals.logical_qubits, 3);
        assert_eq!(counts.totals.calls_per_operation["read_result"], 2);
        assert!(!counts.totals.calls_per_operation.contains_key("mz_store"));
        assert!(
            !counts
                .totals
                .calls_per_operation
                .contains_key("mz_store_reset")
        );

        let result = profiler
            .execute_batch(&[crate::QuantumBackendInstruction {
                instruction_id: instructions::READ_RESULT,
                parameters: smallvec::smallvec![crate::Parameter::ResultId(2000)],
            }])
            .expect("batch should succeed");
        assert_eq!(result.results.len(), 1);
        assert!(!result.results.spilled());
        assert_eq!(
            profiler.counts[usize::from(crate::instructions::READ_RESULT)],
            3
        );
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
        let mut interpreter = create_qsharp_interpreter(vec![("test.qs".into(), source.into())])?;
        let timings = FxHashMap::from_iter([
            ("h".to_string(), 0.25),
            ("mz_store".to_string(), 0.5),
            ("read_result".to_string(), 0.125),
        ]);
        let mut profiler = SequentialCountingProfiler::new(10, timings)?;

        let result = profiler.profile_qsharp_expr(&mut interpreter, "Test.Main()")?;

        assert_eq!(result.totals.logical_qubits, 2);
        assert_eq!(result.totals.physical_qubits, 20);
        assert_eq!(
            result.totals.calls_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 2),
                ("mz_store".to_string(), 2),
                ("read_result".to_string(), 2),
            ])
        );
        assert_eq!(
            result.totals.runtime_sec_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 0.5),
                ("mz_store".to_string(), 1.0),
                ("read_result".to_string(), 0.25),
            ])
        );
        assert!((result.totals.total_runtime_sec - 1.75).abs() < f64::EPSILON);
        assert!(result.infos.is_empty());
        Ok(())
    }

    #[test]
    /// Reports one message listing executed gates whose timings were omitted.
    fn reports_missing_gate_timings() {
        let timings = FxHashMap::from_iter([("h".to_string(), 0.25)]);
        let mut profiler = SequentialCountingProfiler::new(10, timings).expect("valid timings");
        profiler
            .execute_batch(&[
                crate::QuantumBackendInstruction {
                    instruction_id: instructions::H,
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_id: instructions::MZ_STORE,
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(0)
                    ],
                },
                crate::QuantumBackendInstruction {
                    instruction_id: instructions::MZ_STORE,
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(0)
                    ],
                },
                crate::QuantumBackendInstruction {
                    instruction_id: instructions::READ_RESULT,
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_id: instructions::READ_RESULT,
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
            ])
            .expect("batch should succeed");

        let result = profiler.get_result();

        assert_eq!(
            result.infos,
            vec![
                "These gates did not have time configured, assumed 0: [mz_store, read_result]"
                    .to_string()
            ]
        );
        assert_eq!(
            result.totals.calls_per_operation,
            FxHashMap::from_iter([("h".to_string(), 1)])
        );
        assert_eq!(
            result.totals.runtime_sec_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 0.25),
                ("mz_store".to_string(), 0.0),
                ("read_result".to_string(), 0.0),
            ])
        );
    }
}
