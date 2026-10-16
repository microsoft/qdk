// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod qs_backend_shim;
mod sparse_sim;

use crate::{
    QsBackendShim, QuantumBackend, QuantumBackendResult, QuantumInstruction, SparseSimBackend,
};
use qsc_eval::backend::SparseSim;

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
        ("mz", smallvec![0, 1], smallvec![]),
        ("read_result", smallvec![0], smallvec![ResultId(0)]),
        ("read_result", smallvec![], smallvec![Float(0.5)]),
    ] {
        let instruction = QuantumInstruction {
            instruction_id: if name == "unknown" {
                usize::MAX
            } else {
                crate::instructions::instruction_name_to_id(name).expect("known test instruction")
            },
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

        let no_reads = backend.execute_batch(&[crate::QuantumInstruction {
            instruction_id: crate::instructions::MZ,
            targets: smallvec::smallvec![0],
            classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
        }]);
        assert!(no_reads.results.is_empty());
        assert!(!no_reads.results.spilled());

        let single = backend.execute_batch(&[
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::X,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(single.results.as_slice(), &[Zero]);
        assert!(!single.results.spilled());

        let multiple = backend.execute_batch(&[
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(0)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::RESET,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::READ_RESULT,
                targets: smallvec::smallvec![],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            },
            crate::QuantumInstruction {
                instruction_id: crate::instructions::X,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(multiple.results.as_slice(), &[One, Zero, One, Zero]);

        let after_last_read = backend.execute_batch(&[
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
                instruction_id: crate::instructions::RESET,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            },
        ]);
        assert_eq!(after_last_read.results.as_slice(), &[One]);
        assert!(!after_last_read.results.spilled());
    }
}
