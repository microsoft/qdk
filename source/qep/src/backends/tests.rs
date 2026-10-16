// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod qs_backend_shim;
mod sparse_sim;

use crate::{
    QsBackendShim, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
    SparseSimBackend,
};
use qsc_eval::backend::SparseSim;

#[test]
/// Returns immediate measurements in order without overwriting stored results.
fn immediate_measurements_preserve_state_and_result_slots() {
    use crate::ClassicalParameter::ResultId;
    use QuantumBackendResult::{One, Zero};

    let instructions: Vec<_> = [
        ("mz_store", Some(4), Some(0)),
        ("x", Some(4), None),
        ("mz", Some(4), None),
        ("mz", Some(4), None),
        ("read_result", None, Some(0)),
        ("mz_reset", Some(4), None),
        ("mz", Some(4), None),
        ("read_result", None, Some(0)),
        ("x", Some(4), None),
        ("mz_store_reset", Some(4), Some(7)),
        ("mz", Some(4), None),
        ("read_result", None, Some(7)),
    ]
    .into_iter()
    .map(
        |(instruction_name, target, result)| QuantumBackendInstruction {
            instruction_name,
            targets: target.into_iter().collect(),
            classical_parameters: result.map(ResultId).into_iter().collect(),
        },
    )
    .collect();

    let mut sparse_sim = SparseSimBackend::default();
    let mut shim = QsBackendShim::new(SparseSim::new());
    for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
        let batch = backend.execute_batch(&instructions);
        assert_eq!(
            batch.results.as_slice(),
            &[One, One, Zero, One, Zero, Zero, Zero, One]
        );
    }
}

#[test]
/// Allocates fresh measurement targets without creating a stored result.
fn immediate_measurements_do_not_initialize_result_slots() {
    use crate::ClassicalParameter::ResultId;
    use smallvec::smallvec;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for name in ["mz", "mz_reset"] {
        let mut sparse_sim = SparseSimBackend::default();
        let mut shim = QsBackendShim::new(SparseSim::new());
        for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
            let batch = backend.execute_batch(&[QuantumBackendInstruction {
                instruction_name: name,
                targets: smallvec![3],
                classical_parameters: smallvec![],
            }]);
            assert_eq!(batch.results.as_slice(), &[QuantumBackendResult::Zero]);
            assert!(!batch.results.spilled());
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    backend.execute_batch(&[QuantumBackendInstruction {
                        instruction_name: "read_result",
                        targets: smallvec![],
                        classical_parameters: smallvec![ResultId(3)],
                    }])
                }))
                .is_err()
            );
        }
    }
}

#[test]
/// Rejects unknown names, invalid target counts, and incorrect classical parameter types.
fn rejects_invalid_named_instructions() {
    use crate::ClassicalParameter::{Float, ResultId};
    use smallvec::smallvec;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for (name, targets, classical_parameters) in [
        ("unknown", smallvec![0], smallvec![]),
        ("h", smallvec![], smallvec![]),
        ("h", smallvec![0], smallvec![Float(0.5)]),
        ("rx", smallvec![0], smallvec![ResultId(0)]),
        ("mz_store", smallvec![0, 1], smallvec![]),
        ("mz_store_reset", smallvec![0, 1], smallvec![]),
        ("mz", smallvec![0], smallvec![ResultId(0)]),
        ("mz", smallvec![], smallvec![]),
        ("mz", smallvec![0, 1], smallvec![]),
        ("mz_reset", smallvec![0], smallvec![ResultId(0)]),
        ("mz_reset", smallvec![], smallvec![]),
        ("mz_reset", smallvec![0, 1], smallvec![]),
        ("mresetz", smallvec![0], smallvec![ResultId(0)]),
        ("read_result", smallvec![0], smallvec![ResultId(0)]),
        ("read_result", smallvec![], smallvec![Float(0.5)]),
    ] {
        let instruction = QuantumBackendInstruction {
            instruction_name: name,
            targets,
            classical_parameters,
        };
        let mut sparse_sim = SparseSimBackend::default();
        let mut shim = QsBackendShim::new(SparseSim::new());
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                crate::instructions::validate_instruction(&instruction);
            }))
            .is_err()
        );
        for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
            assert!(
                catch_unwind(AssertUnwindSafe(
                    || backend.execute_batch(std::slice::from_ref(&instruction))
                ))
                .is_err(),
                "{name} should be rejected"
            );
        }
    }
}

#[test]
/// Preserves read order across interleaved gates, repeated reads, and result writes.
fn batches_return_one_result_per_read() {
    use QuantumBackendResult::{One, Zero};

    let mut sparse_sim = SparseSimBackend::default();
    let mut shim = QsBackendShim::new(SparseSim::new());
    for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
        let empty = backend.execute_batch(&[]);
        assert!(empty.results.is_empty());
        assert!(!empty.results.spilled());

        let no_reads = backend.execute_batch(&[crate::QuantumBackendInstruction {
            instruction_name: "mz_store",
            targets: smallvec::smallvec![0],
            classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
        }]);
        assert!(no_reads.results.is_empty());
        assert!(!no_reads.results.spilled());

        let single = backend.execute_batch(&[
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "x",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(single.results.as_slice(), &[Zero]);
        assert!(!single.results.spilled());

        let multiple = backend.execute_batch(&[
            crate::QuantumBackendInstruction {
                instruction_name: "mz_store",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "reset",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "mz_store",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "x",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(multiple.results.as_slice(), &[One, Zero, One, Zero]);

        let after_last_read = backend.execute_batch(&[
            crate::QuantumBackendInstruction {
                instruction_name: "mz_store",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "read_result",
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumBackendInstruction {
                instruction_name: "reset",
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(after_last_read.results.as_slice(), &[One]);
        assert!(!after_last_read.results.spilled());
    }
}
