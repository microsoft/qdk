// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod counting;
mod sparse_sim;
#[cfg(test)]
mod tests;

pub use counting::{CountingBackend, CountingBackendCounts};
pub use sparse_sim::SparseSimBackend;
