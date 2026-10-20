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
    use crate::Parameter::ResultId;
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
            parameters: target
                .into_iter()
                .map(crate::Parameter::QubitId)
                .chain(result.map(ResultId))
                .collect(),
        },
    )
    .collect();

    let mut sparse_sim = SparseSimBackend::default();
    let mut shim = QsBackendShim::new(SparseSim::new());
    for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
        let batch = backend
            .execute_batch(&instructions)
            .expect("batch should succeed");
        assert_eq!(
            batch.results.as_slice(),
            &[One, One, Zero, One, Zero, Zero, Zero, One]
        );
    }
}

#[test]
/// Allocates fresh measurement targets without creating a stored result.
fn immediate_measurements_do_not_initialize_result_slots() {
    use crate::Parameter::ResultId;
    use smallvec::smallvec;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for name in ["mz", "mz_reset"] {
        let mut sparse_sim = SparseSimBackend::default();
        let mut shim = QsBackendShim::new(SparseSim::new());
        for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
            let batch = backend
                .execute_batch(&[QuantumBackendInstruction {
                    instruction_name: name,
                    parameters: smallvec![crate::Parameter::QubitId(3)],
                }])
                .expect("batch should succeed");
            assert_eq!(batch.results.as_slice(), &[QuantumBackendResult::Zero]);
            assert!(!batch.results.spilled());
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    backend
                        .execute_batch(&[QuantumBackendInstruction {
                            instruction_name: "read_result",
                            parameters: smallvec![ResultId(3)],
                        }])
                        .expect("batch should succeed")
                }))
                .is_err()
            );
        }
    }
}

#[test]
/// Rejects unknown names, invalid target counts, and incorrect classical parameter types.
fn rejects_invalid_named_instructions() {
    use crate::Parameter::{Float, QubitId, ResultId};
    use smallvec::smallvec;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for (name, classical_parameters) in [
        ("unknown", smallvec![QubitId(0)]),
        ("h", smallvec![]),
        ("h", smallvec![QubitId(0), Float(0.5)]),
        ("rx", smallvec![QubitId(0), ResultId(0)]),
        ("mz_store", smallvec![QubitId(0), QubitId(1)]),
        ("mz_store_reset", smallvec![QubitId(0), QubitId(1)]),
        ("mz", smallvec![QubitId(0), ResultId(0)]),
        ("mz", smallvec![]),
        ("mz", smallvec![QubitId(0), QubitId(1)]),
        ("mz_reset", smallvec![QubitId(0), ResultId(0)]),
        ("mz_reset", smallvec![]),
        ("mz_reset", smallvec![QubitId(0), QubitId(1)]),
        ("mresetz", smallvec![QubitId(0), ResultId(0)]),
        ("read_result", smallvec![QubitId(0), ResultId(0)]),
        ("read_result", smallvec![Float(0.5)]),
        ("h", smallvec![ResultId(0)]),
        ("cx", smallvec![QubitId(0), ResultId(1)]),
        ("rx", smallvec![Float(0.5), QubitId(0)]),
        ("mz_store", smallvec![ResultId(0), QubitId(1)]),
    ] {
        let instruction = QuantumBackendInstruction {
            instruction_name: name,
            parameters: classical_parameters,
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
                catch_unwind(AssertUnwindSafe(|| backend
                    .execute_batch(std::slice::from_ref(&instruction))
                    .expect("batch should succeed")))
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
        let empty = backend.execute_batch(&[]).expect("batch should succeed");
        assert!(empty.results.is_empty());
        assert!(!empty.results.spilled());

        let no_reads = backend
            .execute_batch(&[crate::QuantumBackendInstruction {
                instruction_name: "mz_store",
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::ResultId(0)
                ],
            }])
            .expect("batch should succeed");
        assert!(no_reads.results.is_empty());
        assert!(!no_reads.results.spilled());

        let single = backend
            .execute_batch(&[
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "x",
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                },
            ])
            .expect("batch should succeed");
        assert_eq!(single.results.as_slice(), &[Zero]);
        assert!(!single.results.spilled());

        let multiple = backend
            .execute_batch(&[
                crate::QuantumBackendInstruction {
                    instruction_name: "mz_store",
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(1)
                    ],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(1)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(1)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "reset",
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "mz_store",
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(1)
                    ],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(1)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "x",
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                },
            ])
            .expect("batch should succeed");
        assert_eq!(multiple.results.as_slice(), &[One, Zero, One, Zero]);

        let after_last_read = backend
            .execute_batch(&[
                crate::QuantumBackendInstruction {
                    instruction_name: "mz_store",
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(0)
                    ],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "reset",
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                },
            ])
            .expect("batch should succeed");
        assert_eq!(after_last_read.results.as_slice(), &[One]);
        assert!(!after_last_read.results.spilled());
    }
}
