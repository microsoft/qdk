// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod qs_backend_shim;
pub use qs_backend_shim::QsBackendShim;
use qsc_eval::backend::{CliffordSim, SparseSim};

/// Constructors for built-in quantum backends.
pub struct QuantumBackends;

impl QuantumBackends {
    /// Creates a sparse simulator wrapped for streamed QIR execution.
    #[must_use]
    pub fn sparse_sim() -> QsBackendShim<SparseSim> {
        QsBackendShim::new(SparseSim::new())
    }

    /// Creates a Clifford simulator with the requested number of qubits.
    #[must_use]
    pub fn clifford_sim(num_qubits: usize) -> QsBackendShim<CliffordSim> {
        QsBackendShim::new(CliffordSim::new(num_qubits))
    }
}
