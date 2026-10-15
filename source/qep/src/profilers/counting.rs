// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult};
use crate::{
    interface::{MessageIn, MessageOut, QuantumBackend, QuantumBackendEndpoint},
    qir_isa::{OPERATION_COUNT, QirInstruction, instruction_index, instruction_names},
};
use rustc_hash::FxHashMap;

/// Profiles instructions as if every operation executes sequentially.
pub struct SequentialCountingProfiler {
    /// The physical-qubit overhead applied to each observed logical qubit.
    physical_qubits_per_logical: u64,
    /// The duration of one call, keyed by the stable QIR instruction name.
    gate_time_sec: FxHashMap<String, f64>,
    /// Call counts indexed by QIR instruction kind.
    counts: [u64; OPERATION_COUNT],
    /// The largest logical qubit ID observed during execution.
    max_qubit_id: Option<usize>,
}

impl SequentialCountingProfiler {
    /// Creates a profiler, treating omitted instruction timings as zero.
    ///
    /// # Panics
    ///
    /// Panics if a timing has an unknown name or is negative or non-finite.
    #[must_use]
    pub fn new(physical_qubits_per_logical: u64, gate_time_sec: FxHashMap<String, f64>) -> Self {
        for (name, duration) in &gate_time_sec {
            assert!(
                instruction_names().contains(&name.as_str()),
                "unknown QIR instruction name `{name}`"
            );
            assert!(
                duration.is_finite() && *duration >= 0.0,
                "gate timing for instruction `{name}` must be finite and non-negative"
            );
        }

        Self {
            physical_qubits_per_logical,
            gate_time_sec,
            counts: [0; OPERATION_COUNT],
            max_qubit_id: None,
        }
    }

    /// Updates the largest logical qubit ID referenced by an instruction.
    fn observe_qubits(&mut self, instruction: &QirInstruction) {
        let instruction_max = match instruction {
            QirInstruction::ccx(q0, q1, q2) => Some((*q0).max(*q1).max(*q2)),
            QirInstruction::cx(q0, q1)
            | QirInstruction::cy(q0, q1)
            | QirInstruction::cz(q0, q1)
            | QirInstruction::swap(q0, q1)
            | QirInstruction::rxx(_, q0, q1)
            | QirInstruction::ryy(_, q0, q1)
            | QirInstruction::rzz(_, q0, q1) => Some((*q0).max(*q1)),
            QirInstruction::rx(_, q)
            | QirInstruction::ry(_, q)
            | QirInstruction::rz(_, q)
            | QirInstruction::h(q)
            | QirInstruction::s(q)
            | QirInstruction::sadj(q)
            | QirInstruction::sx(q)
            | QirInstruction::t(q)
            | QirInstruction::tadj(q)
            | QirInstruction::x(q)
            | QirInstruction::y(q)
            | QirInstruction::z(q)
            | QirInstruction::reset(q)
            | QirInstruction::mz(q, _)
            | QirInstruction::mresetz(q, _) => Some(*q),
            QirInstruction::read_result(_) => None,
        };

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
            FxHashMap::with_capacity_and_hasher(OPERATION_COUNT, Default::default());
        let mut runtime_time_sec_per_operation =
            FxHashMap::with_capacity_and_hasher(OPERATION_COUNT, Default::default());
        let mut missing_timings = Vec::new();
        let mut total_runtime_sec = 0.0;

        for (index, name) in instruction_names().iter().enumerate() {
            let calls = self.counts[index];
            if calls == 0 {
                continue;
            }
            let gate_time_sec = self.gate_time_sec.get(*name).copied().unwrap_or_else(|| {
                missing_timings.push(*name);
                0.0
            });
            #[allow(clippy::cast_precision_loss)] // Profiling runtimes are represented as f64.
            let runtime = calls as f64 * gate_time_sec;
            total_runtime_sec += runtime;
            calls_per_operation.insert((*name).to_string(), calls);
            runtime_time_sec_per_operation.insert((*name).to_string(), runtime);
        }
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
            runtime_time_sec_per_operation,
            infos,
        }
    }
}

impl QuantumBackend for SequentialCountingProfiler {
    /// Counts streamed QIR instructions and returns random measurement results.
    async fn run(&mut self, mut endpoint: QuantumBackendEndpoint) {
        while let Some(message) = endpoint.instructions_receiver.recv().await {
            let MessageIn::QirInstruction(instruction) = message;
            let index = instruction_index(&instruction);

            self.observe_qubits(&instruction);
            self.counts[index] += 1;

            if matches!(instruction, QirInstruction::read_result(..))
                && endpoint
                    .results_sender
                    .send(MessageOut::MeasurementResult(rand::random()))
                    .await
                    .is_err()
            {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SequentialCountingProfiler;
    use crate::Profiler;
    use crate::qir_isa::{QirInstruction, instruction_index};
    use crate::tests::compile_qsharp_to_qir;
    use qsc::target::Profile;
    use rustc_hash::FxHashMap;

    #[tokio::test]
    /// Verifies instruction counts and sequential resource estimates.
    async fn profiles_instructions_sequentially() -> Result<(), String> {
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
        let mut profiler = SequentialCountingProfiler::new(10, timings);

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let result = unsafe { profiler.profile_qir(file.path()?).await }?;

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
            result.runtime_time_sec_per_operation,
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
        let mut profiler = SequentialCountingProfiler::new(10, timings);
        profiler.counts[instruction_index(&QirInstruction::h(0))] = 1;
        profiler.counts[instruction_index(&QirInstruction::mz(0, 0))] = 2;
        profiler.counts[instruction_index(&QirInstruction::read_result(0))] = 2;

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
            FxHashMap::from_iter([
                ("h".to_string(), 1),
                ("mz".to_string(), 2),
                ("read_result".to_string(), 2),
            ])
        );
        assert_eq!(
            result.runtime_time_sec_per_operation,
            FxHashMap::from_iter([
                ("h".to_string(), 0.25),
                ("mz".to_string(), 0.0),
                ("read_result".to_string(), 0.0),
            ])
        );
    }
}
