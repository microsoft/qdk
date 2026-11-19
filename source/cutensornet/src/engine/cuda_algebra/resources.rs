// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`AlgebraResources`]: what a [`CudaAlgebra`](super::CudaAlgebra) keeps
//! across its calls, so that a call reuses its descriptor, its memory and
//! its prepared networks instead of making them again, and how they are
//! released.

use super::contraction::PreparedContractionNetworks;
use super::scratch::Scratch;
use crate::engine::cuda_algebra_error::{CudaAlgebraError, first_failure};
use crate::engine::native_calls::{NativeCalls, OpaqueHandle};

/// What an algebra keeps across its calls.
#[derive(Default)]
pub(super) struct AlgebraResources {
    /// The one workspace descriptor every call is sized for in turn.
    pub(super) workspace: Option<OpaqueHandle>,
    /// The memory lent to the workspace.
    pub(super) scratch: Scratch,
    /// A device buffer of 16 complex doubles: each call's gate.
    pub(super) gate: Option<OpaqueHandle>,
    /// A device buffer for a gate split's singular values, and the complex
    /// doubles it holds.
    pub(super) singular_values: Option<(OpaqueHandle, usize)>,
    /// The contraction networks prepared for the shapes seen.
    pub(super) prepared_networks: PreparedContractionNetworks,
}

impl AlgebraResources {
    /// Releases everything, the workspace's memory once the descriptor is
    /// destroyed; attempts every release and returns the first failure.
    pub(super) fn release(mut self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        let mut released = vec![self.prepared_networks.release(calls)];
        if let Some(workspace) = self.workspace.take() {
            released.push(calls.destroy_workspace(workspace));
        }
        released.push(self.scratch.release(calls));
        let buffers = [
            self.singular_values.take().map(|(buffer, _)| buffer),
            self.gate.take(),
        ];
        released.extend(
            buffers
                .into_iter()
                .flatten()
                .map(|buffer| calls.free(buffer)),
        );
        first_failure(released)
    }
}
