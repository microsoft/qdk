// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The call a workspace was last sized for ([`SizedFor`]), which a tensor
//! call checks before it runs, and which a new path search for a network
//! makes stale.

use super::super::{FakeRule, Inner, Object, failed, id, refused};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::memory_calls::WorkspaceDescriptor;
use crate::engine::native_calls::session_calls::CuTensorNetHandle;

/// The call a workspace was last sized for, by its objects' ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SizedFor {
    /// A gate split: A, B, the gate, U and V, and the SVD config.
    GateSplit {
        descriptors: [usize; 5],
        config: usize,
    },
    /// A QR: the input, Q and R.
    Qr { descriptors: [usize; 3] },
    /// A contraction of a network along an optimizer info's path.
    Contraction { network: usize, info: usize },
}

impl Inner {
    /// Records that `workspace`, which must belong to `handle`, is sized for
    /// `call`.
    pub(super) fn size_for(
        &mut self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        call: SizedFor,
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        self.owned(handle, workspace, operation)?;
        match self.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { sized_for, .. }) => {
                *sized_for = Some(call);
                Ok(())
            }
            _ => Err(failed(operation, "no such workspace")),
        }
    }

    /// Forgets the tensor call a workspace was sized for wherever `stale`,
    /// given the workspace's id and that call, says the sizing no longer
    /// holds, so the call is refused until the workspace is sized again.
    pub(super) fn forget_sizings(&mut self, stale: impl Fn(usize, SizedFor) -> bool) {
        for (&object, value) in &mut self.objects {
            if let Object::Workspace { sized_for, .. } = value
                && sized_for.is_some_and(|call| stale(object, call))
            {
                *sized_for = None;
            }
        }
    }

    /// Fails unless `workspace`, which must belong to `handle`, was last
    /// sized for `call`.
    pub(super) fn sized_for(
        &self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        call: SizedFor,
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        self.owned(handle, workspace, operation)?;
        match self.objects.get(&id(workspace)) {
            Some(Object::Workspace { sized_for, .. }) if *sized_for == Some(call) => Ok(()),
            Some(Object::Workspace { .. }) => {
                Err(refused(operation, FakeRule::WorkspaceNotSizedForTheCall))
            }
            _ => Err(failed(operation, "no such workspace")),
        }
    }
}
