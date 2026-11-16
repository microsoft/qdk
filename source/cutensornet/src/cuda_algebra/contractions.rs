// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Contractions`]: the pairwise contractions of a move of the centre (a
//! one-site gate into a site, a QR's small factor into the new centre),
//! each through a contraction network prepared once per shape and kept.
//!
//! A network costs a path search and a preparation, and a run's moves of
//! the centre repeat a few hundred shapes, so the algebra keeps one
//! prepared network per shape (its inputs' extents and the three tensors'
//! labels) and gives it new buffers at each contraction. Beyond
//! `policy::NETWORKS_KEPT` networks, all are released and the cache starts
//! again.

use super::call_scope::CallScope;
use super::policy;
use crate::native_calls::error::{NativeError, combine_execution_and_cleanup, first_failure};
use crate::native_calls::tensor_calls::{ContractionNetwork, TensorModes};
use crate::native_calls::{NativeCalls, OpaqueHandle};
use std::collections::BTreeMap;

/// The prepared networks the algebra keeps, by shape.
#[derive(Default)]
pub(super) struct Contractions {
    networks: BTreeMap<NetworkShape, PreparedNetwork>,
}

/// One input of a contraction: its buffer, extents and labels.
pub(super) struct Input<'a> {
    pub(super) data: OpaqueHandle,
    pub(super) extents: &'a [usize],
    pub(super) modes: &'a [i32],
}

/// The shape a network is prepared for: its inputs' extents, and the labels
/// of its inputs and output. The output's extents follow from them.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NetworkShape {
    extents: [Vec<i64>; 2],
    modes: [Vec<i32>; 3],
}

/// A network, prepared along the path in its optimizer info; the info is
/// destroyed before the network.
#[derive(Clone, Copy)]
struct PreparedNetwork {
    network: ContractionNetwork,
    info: OpaqueHandle,
}

impl Contractions {
    /// Releases every network; attempts every release and returns the first
    /// failure.
    pub(super) fn release(&mut self, calls: &dyn NativeCalls) -> Result<(), NativeError> {
        let networks = std::mem::take(&mut self.networks);
        first_failure(
            networks
                .into_values()
                .map(|prepared| prepared.release(calls)),
        )
    }
}

impl PreparedNetwork {
    fn release(self, calls: &dyn NativeCalls) -> Result<(), NativeError> {
        first_failure([
            calls.destroy_optimizer_info(self.info),
            calls.destroy_contraction_network(self.network.descriptor),
        ])
    }
}

/// Contracts `inputs` into `output`, of labels `output_modes`, summing
/// every label absent from them, and waits for it; prepares the shape's
/// network first if none is kept.
pub(super) fn contract(
    scope: &mut CallScope<'_>,
    inputs: &[Input<'_>; 2],
    output_modes: &[i32],
    output: OpaqueHandle,
) -> Result<(), NativeError> {
    let [first, second] = inputs;
    let shape = NetworkShape {
        extents: [
            super::call_scope::native_extents(first.extents)?,
            super::call_scope::native_extents(second.extents)?,
        ],
        modes: [
            first.modes.to_vec(),
            second.modes.to_vec(),
            output_modes.to_vec(),
        ],
    };
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    let network = if let Some(&PreparedNetwork { network, info }) =
        scope.resources.contractions.networks.get(&shape)
    {
        calls.contraction_workspace_sizes(handle, network.descriptor, info, workspace)?;
        scope.lend_workspace()?;
        network
    } else {
        if scope.resources.contractions.networks.len() >= policy::NETWORKS_KEPT {
            scope.resources.contractions.release(calls)?;
        }
        let prepared = prepare(scope, &shape)?;
        scope
            .resources
            .contractions
            .networks
            .insert(shape, prepared);
        prepared.network
    };
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

/// A network of `shape`, its path found, its workspace sized and lent, and
/// prepared; on a failure, what was created is released and the failure
/// returned.
fn prepare(
    scope: &mut CallScope<'_>,
    shape: &NetworkShape,
) -> Result<PreparedNetwork, NativeError> {
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
    let prepared = PreparedNetwork { network, info };
    let ready = calls
        .optimize_contraction(
            handle,
            network.descriptor,
            policy::PATH_WORKSPACE_LIMIT,
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
