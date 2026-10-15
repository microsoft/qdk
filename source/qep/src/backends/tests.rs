// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod qs_backend_shim;
mod sparse_sim;

use crate::{
    QirInstruction, QsBackendShim, QuantumBackend, QuantumBackendResult, SparseSimBackend,
};
use qsc_eval::backend::SparseSim;

#[test]
/// Preserves read order across interleaved gates, repeated reads, and result writes.
fn batches_return_one_result_per_read() {
    use QirInstruction::{mz, read_result, reset, x};
    use QuantumBackendResult::{One, Zero};

    let mut sparse_sim = SparseSimBackend::default();
    let mut shim = QsBackendShim::new(SparseSim::new());
    for backend in [&mut sparse_sim as &mut dyn QuantumBackend, &mut shim] {
        let empty = backend.execute_batch(&[]);
        assert!(empty.results.is_empty());
        assert!(!empty.results.spilled());

        let no_reads = backend.execute_batch(&[mz(0, 0)]);
        assert!(no_reads.results.is_empty());
        assert!(!no_reads.results.spilled());

        let single = backend.execute_batch(&[read_result(0), x(0)]);
        assert_eq!(single.results.as_slice(), &[Zero]);
        assert!(!single.results.spilled());

        let multiple = backend.execute_batch(&[
            mz(0, 1),
            read_result(1),
            read_result(0),
            read_result(1),
            reset(0),
            mz(0, 1),
            read_result(1),
            x(0),
        ]);
        assert_eq!(multiple.results.as_slice(), &[One, Zero, One, Zero]);

        let after_last_read = backend.execute_batch(&[mz(0, 0), read_result(0), reset(0)]);
        assert_eq!(after_last_read.results.as_slice(), &[One]);
        assert!(!after_last_read.results.spilled());
    }
}
