// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod dynamic_memory_compute;
mod lattice_surgery;
mod psspc;
mod unmemory;

pub use dynamic_memory_compute::{ComputeCapacity, DynamicMemoryCompute, EvictionStrategy};
pub use lattice_surgery::LatticeSurgery;
pub use psspc::PSSPC;
pub use unmemory::Unmemory;

use crate::{Error, Trace};

pub trait TraceTransform {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error>;
}
