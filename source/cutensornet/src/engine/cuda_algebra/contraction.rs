// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PreparedContractionNetworks`]: the pairwise contractions of a move of
//! the centre (a one-site gate into a site, a QR's small factor into the
//! new centre), each through a contraction network prepared once per shape
//! and kept.
//!
//! A network costs a path search and a preparation, and a run's moves of
//! the centre repeat a few hundred shapes. So the algebra keeps one
//! prepared network per shape (its inputs' extents and the three tensors'
//! labels), and gives it new buffers at each contraction. Beyond
//! `settings::NETWORKS_KEPT` networks, all are released and the cache starts
//! again.

use super::call_scope::CallScope;
use super::extents::native_extents;
use super::settings;
use super::tensor_layout::TensorLayout;
use crate::engine::cuda_algebra_error::{
    CudaAlgebraError, combine_execution_and_cleanup, first_failure,
};
use crate::engine::native_calls::NativeCalls;
use crate::engine::native_calls::memory_calls::DeviceBuffer;
use crate::engine::native_calls::tensor_calls::{
    ContractionOptimizerInfo, NativeContractionNetwork, TensorModes,
};
use std::collections::BTreeMap;

/// The prepared networks the algebra keeps, by shape.
#[derive(Default)]
pub(super) struct PreparedContractionNetworks {
    networks: BTreeMap<NetworkShape, PreparedContractionNetwork>,
}

/// One input of a contraction: its buffer, extents and labels.
pub(super) struct ContractionInput<'a> {
    pub(super) data: DeviceBuffer,
    pub(super) extents: &'a [usize],
    pub(super) modes: &'a [i32],
}

impl<'a> ContractionInput<'a> {
    /// The input `data`, a tensor of `layout`.
    #[inline]
    pub(super) fn new<const RANK: usize>(
        data: DeviceBuffer,
        layout: &'a TensorLayout<RANK>,
    ) -> Self {
        Self {
            data,
            extents: &layout.extents,
            modes: &layout.modes,
        }
    }
}

/// The shape a network is prepared for: its inputs' extents, and the labels
/// of its inputs and output. The output's extents follow from them.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NetworkShape {
    /// The two inputs' extents.
    extents: [Vec<i64>; 2],
    /// The two inputs' labels, then the output's.
    modes: [Vec<i32>; 3],
}

/// A network, prepared along the path in its optimizer info; the info is
/// destroyed before the network.
#[derive(Clone, Copy)]
struct PreparedContractionNetwork {
    network: NativeContractionNetwork,
    info: ContractionOptimizerInfo,
}

impl PreparedContractionNetworks {
    /// Releases every network. It attempts every release and returns the
    /// first failure. A network whose destroy fails is leaked, never retried.
    pub(super) fn release(&mut self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        let networks = std::mem::take(&mut self.networks);
        first_failure(
            networks
                .into_values()
                .map(|prepared| prepared.release(calls)),
        )
    }
}

impl PreparedContractionNetwork {
    /// Destroys the optimizer info, then the network; attempts both and
    /// returns the first failure.
    fn release(self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        first_failure([
            calls.destroy_optimizer_info(self.info),
            calls.destroy_contraction_network(self.network.descriptor),
        ])
    }
}

/// Contracts `inputs` into `output`, of labels `output_modes`, summing
/// every label absent from them, and waits for it. It first prepares the
/// shape's network if none is kept.
pub(super) fn contract(
    scope: &mut CallScope<'_>,
    inputs: &[ContractionInput<'_>; 2],
    output_modes: &[i32],
    output: DeviceBuffer,
) -> Result<(), CudaAlgebraError> {
    let shape = NetworkShape::of(inputs, output_modes)?;
    let network = ready_network(scope, shape)?;
    let [first, second] = inputs;
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    calls.contract(
        handle,
        network,
        [first.data, second.data],
        output,
        workspace,
        scope.stream(),
    )?;
    scope.synchronize()
}

impl NetworkShape {
    /// The shape of a contraction of `inputs` into `output_modes`.
    fn of(
        [first, second]: &[ContractionInput<'_>; 2],
        output_modes: &[i32],
    ) -> Result<Self, CudaAlgebraError> {
        Ok(Self {
            extents: [
                native_extents(first.extents)?,
                native_extents(second.extents)?,
            ],
            modes: [
                first.modes.to_vec(),
                second.modes.to_vec(),
                output_modes.to_vec(),
            ],
        })
    }
}

/// The network of `shape`, its workspace sized and lent, ready to
/// contract: the one kept if any, otherwise one prepared now and kept.
/// The cache is full at `settings::NETWORKS_KEPT`, when all its networks
/// are released before the new one is kept.
///
/// ```text
///  `shape` kept:        size and lend the workspace for it
///  not kept, room:      prepare it, keep it
///  not kept, full:      release every kept network, prepare it, keep it
/// ```
fn ready_network(
    scope: &mut CallScope<'_>,
    shape: NetworkShape,
) -> Result<NativeContractionNetwork, CudaAlgebraError> {
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    if let Some(&PreparedContractionNetwork { network, info }) =
        scope.resources.prepared_networks.networks.get(&shape)
    {
        calls.contraction_workspace_sizes(handle, network.descriptor, info, workspace)?;
        scope.lend_workspace()?;
        return Ok(network);
    }
    if scope.resources.prepared_networks.networks.len() >= settings::NETWORKS_KEPT {
        scope.resources.prepared_networks.release(calls)?;
    }
    let prepared = prepare(scope, &shape)?;
    scope
        .resources
        .prepared_networks
        .networks
        .insert(shape, prepared);
    Ok(prepared.network)
}

/// A network of `shape`, its path found, its workspace sized and lent, and
/// prepared. On a failure, what was created is released and the failure
/// returned.
fn prepare(
    scope: &mut CallScope<'_>,
    shape: &NetworkShape,
) -> Result<PreparedContractionNetwork, CudaAlgebraError> {
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    let inputs = [0, 1].map(|at| TensorModes {
        extents: &shape.extents[at],
        modes: &shape.modes[at],
    });
    let network = calls.create_contraction_network(handle, inputs, &shape.modes[2])?;
    let info = match calls.create_optimizer_info(handle, network.descriptor) {
        Ok(info) => info,
        Err(error) => {
            let destroyed = calls.destroy_contraction_network(network.descriptor);
            return combine_execution_and_cleanup(Err(error), destroyed);
        }
    };
    let prepared = PreparedContractionNetwork { network, info };
    let ready = calls
        .optimize_contraction(
            handle,
            network.descriptor,
            settings::PATH_WORKSPACE_LIMIT,
            info,
        )
        .and_then(|()| {
            calls.contraction_workspace_sizes(handle, network.descriptor, info, workspace)
        })
        .and_then(|()| scope.lend_workspace())
        .and_then(|()| calls.prepare_contraction(handle, network.descriptor, workspace));
    match ready {
        Ok(()) => Ok(prepared),
        Err(error) => combine_execution_and_cleanup(Err(error), prepared.release(calls)),
    }
}

#[cfg(test)]
mod tests;
