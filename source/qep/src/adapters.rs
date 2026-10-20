// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod decompose_toffoli_adapter;
mod result_adapter;

pub use decompose_toffoli_adapter::DecomposeToffoliAdapter;
pub use result_adapter::ResultAdapter;

use crate::{Profiler, QuantumBackend, interface::Driver};

/// Adapter is something that is both Driver and QuantumBackend.
trait Adapter: Driver + QuantumBackend {}

impl<T: Driver + QuantumBackend> Adapter for T {}

/// Any adapter on top of Profiler produces a Profiler.
impl<A: Adapter> Profiler for A
where
    A::Backend: Profiler,
{
    /// Returns profiling data collected by the wrapped backend.
    fn get_result(&self) -> crate::ProfilerResult {
        self.get_backend().get_result()
    }
}
