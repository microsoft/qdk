// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod decompose_toffoli_adapter;
mod result_adapter;

pub use decompose_toffoli_adapter::DecomposeToffoliAdapter;
pub use result_adapter::ResultAdapter;

use crate::{Profiler, QuantumBackend};

/// Adapter is a QuantumBackend that wraps another QuantumBackend.
trait Adapter: QuantumBackend {
    type Backend: QuantumBackend;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &Self::Backend;
}

/// Adapter wrapping a Profiler is a Profiler that forwards ProfilerResult from wrapped Profiler.
impl<A: Adapter> Profiler for A
where
    A::Backend: Profiler,
{
    fn get_result(&self) -> crate::ProfilerResult {
        self.get_backend().get_result()
    }
}
