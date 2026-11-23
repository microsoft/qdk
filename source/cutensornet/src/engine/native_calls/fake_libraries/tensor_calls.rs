// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `TensorCalls` of the fake libraries: tensor descriptors, gate splits,
//! QRs and pairwise contractions, computed on dense tensors
//! (`dense_tensor.rs`) and factored by `dense_linalg`'s SVD and QR. An SVD
//! that fails its accuracy checks fails the gate split.
//!
//! It is as strict as cuTensorNet's documentation about what a call needs,
//! whether or not cuTensorNet itself refuses the misuse (the tests of the
//! fake say which cases a GPU run refuses), and, where the documentation
//! gives no bound, as cuTensorNet 2.13 was seen to be on an A100:
//!
//! - a tensor descriptor has one extent per mode, and every extent at
//!   least 1;
//! - a discarded-weight cutoff is in [0, 1): the documentation gives it no
//!   range, and cuTensorNet 2.13 on an A100 refuses a cutoff of 1;
//! - every object (a descriptor, an SVD config or info, a network, an
//!   optimizer info) belongs to the handle it was created with;
//! - a gate split, a QR or a contraction runs only in a workspace last
//!   sized for that same call (the same descriptors and config, or the
//!   same network and path), and lent what each of its slots asks for. A
//!   new path search for a network, even into the same optimizer info, may
//!   find another path, so it undoes the sizing for that network;
//! - an optimizer info serves only the network it was created for: its
//!   path is searched, and a workspace sized, for that network alone;
//! - a network is prepared only once optimized, in a workspace sized for
//!   its path and lent at least the device scratch it asks for, and
//!   contracted only once prepared, with at least the device scratch lent
//!   at its last preparation; it outlives its optimizer infos;
//! - a gate split, a QR or a contraction runs on a live stream;
//! - a gate split's shared extent does not exceed the smaller side of its
//!   matrix θ, and a QR's equals the smaller side of its matrix;
//! - a QR's input carries exactly Q's and R's other modes;
//! - a gate split takes A·B·G's topology, as cuTensorNet's gate split
//!   states it: A, B and the gate are pairwise connected (each pair shares
//!   a label); each input label is shared by two or three inputs and in
//!   neither output, or is in one input and one of U's or V's other modes,
//!   so no label is summed alone; and a label of A alone is kept in U, one
//!   of B alone in V, one of the gate alone in either (cuTensorNet 2.13 on
//!   an A100 also splits a label of A alone kept in V, or one of B alone
//!   kept in U, which its documentation does not provide for);
//! - U's and V's buffers hold their descriptors' extents before
//!   truncation, as cuTensorNet asks, since the reduced extent is known
//!   only once the split has run;
//! - the singular values' buffer holds as many doubles as the shared
//!   mode's extent (counted in the fake's 16-byte elements);
//! - what a call writes on its stream, its SVD info included, is read as
//!   the rules on streams in `fake_libraries.rs` say.
//!
//! The fake does not slice: it contracts a network's two inputs at once,
//! whatever the path. It need not model slicing, since the loaded libraries
//! disable it, and a path cuTensorNet sliced anyway is still contracted
//! whole (`TensorCalls::optimize_contraction`), so a caller sees the same
//! result either way. It ignores the path search's workspace limit, which
//! cuTensorNet's optimizer takes as a constraint the path may exceed.
//!
//! Beyond the documentation, it refuses what its dense tensors cannot
//! represent, which cuTensorNet 2.13 accepts when the descriptor or the
//! network is created: a tensor that names one mode twice, and a network
//! output label no input has, which has no extent to take.
//!
//! This file holds the calls, in the trait's one implementation, as
//! `loaded_libraries/tensor_calls.rs` does; what they keep and check is in
//! its folder: a descriptor's shape (`tensor_shape.rs`), a factorization's
//! two outputs (`sides.rs`), the gate split's SVD (`gate_split.rs`), the
//! call a workspace is sized for (`workspace_sizing.rs`), and a
//! contraction network's lookups (`contraction_network.rs`).

mod contraction_network;
pub(super) mod gate_split;
mod sides;
pub(super) mod tensor_shape;
pub(super) mod workspace_sizing;

use super::{
    FakeLibraries, FakeRule, Inner, Object,
    dense_tensor::{DenseTensor, contract},
    failed, id, refused, slot_index, to_usize,
};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::{
    OpaqueHandle, Stream,
    memory_calls::WorkspaceSlot,
    tensor_calls::{
        GateSplitDescriptors, GateSplitOperands, NativeContractionNetwork, SvdSetting,
        SvdTruncation, TensorCalls, TensorModes, TensorOperand,
    },
};
use canonical_mps::dense_linalg::{MatrixRef, Qr};
use gate_split::{Partition, Split, SvdConfig, split};
use sides::Sides;
use tensor_shape::TensorShape;
use workspace_sizing::SizedFor;

impl TensorCalls for FakeLibraries {
    fn create_tensor_descriptor(
        &self,
        handle: OpaqueHandle,
        tensor: TensorModes<'_>,
    ) -> Result<OpaqueHandle, CudaAlgebraError> {
        const OPERATION: &str = "create_tensor_descriptor";
        let mut inner = self.call(OPERATION)?;
        let shape = TensorShape::of(tensor, OPERATION)?;
        inner.create_on(handle, OPERATION, Object::TensorDescriptor(shape))
    }

    fn destroy_tensor_descriptor(&self, descriptor: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        self.call("destroy_tensor_descriptor")?.destroy(
            descriptor,
            "destroy_tensor_descriptor",
            |o| matches!(o, Object::TensorDescriptor(_)),
        )
    }

    fn create_svd_config(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.call("create_svd_config")?.create_on(
            handle,
            "create_svd_config",
            Object::SvdConfig(SvdConfig::default()),
        )
    }

    fn configure_svd(
        &self,
        handle: OpaqueHandle,
        config: OpaqueHandle,
        setting: SvdSetting,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "configure_svd";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, config, OPERATION)?;
        let Some(Object::SvdConfig(config)) = inner.objects.get_mut(&id(config)) else {
            return Err(failed(OPERATION, "no such SVD config"));
        };
        match setting {
            SvdSetting::AlgorithmGesvd => {}
            SvdSetting::NormalizationL2 => config.normalized = true,
            SvdSetting::PartitionUs => config.partition = Some(Partition::Us),
            SvdSetting::PartitionSv => config.partition = Some(Partition::Sv),
            SvdSetting::DiscardedWeightCutoff(cutoff) => {
                if !(0.0..1.0).contains(&cutoff) {
                    return Err(failed(OPERATION, "a cutoff outside [0, 1)"));
                }
                config.cutoff = Some(cutoff);
            }
        }
        Ok(())
    }

    fn destroy_svd_config(&self, config: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        self.call("destroy_svd_config")?
            .destroy(config, "destroy_svd_config", |o| {
                matches!(o, Object::SvdConfig(_))
            })
    }

    fn create_svd_info(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.call("create_svd_info")?
            .create_on(handle, "create_svd_info", Object::SvdInfo(None))
    }

    fn svd_truncation(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<SvdTruncation, CudaAlgebraError> {
        const OPERATION: &str = "svd_truncation";
        let inner = self.call(OPERATION)?;
        inner.owned(handle, info, OPERATION)?;
        if inner.write_pending(id(info)) {
            return Err(failed(
                OPERATION,
                "the stream writing the info is not synchronized",
            ));
        }
        match inner.objects.get(&id(info)) {
            Some(Object::SvdInfo(Some(truncation))) => Ok(*truncation),
            Some(Object::SvdInfo(None)) => Err(failed(OPERATION, "no gate split given this info")),
            _ => Err(failed(OPERATION, "no such SVD info")),
        }
    }

    fn destroy_svd_info(&self, info: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        self.call("destroy_svd_info")?
            .destroy(info, "destroy_svd_info", |o| {
                matches!(o, Object::SvdInfo(_))
            })
    }

    fn gate_split_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        descriptors: &GateSplitDescriptors,
        config: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "gate_split_workspace_sizes";
        let mut inner = self.call(OPERATION)?;
        let GateSplitDescriptors { a, b, gate, u, v } = *descriptors;
        for descriptor in [a, b, gate, u, v] {
            inner.descriptor(handle, descriptor, OPERATION)?;
        }
        inner.owned(handle, config, OPERATION)?;
        let call = SizedFor::GateSplit {
            descriptors: [a, b, gate, u, v].map(id),
            config: id(config),
        };
        inner.size_for(handle, workspace, call, OPERATION)
    }

    fn gate_split(
        &self,
        handle: OpaqueHandle,
        operands: &GateSplitOperands,
        config: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "gate_split";
        let mut inner = self.call(OPERATION)?;
        inner.require(stream, OPERATION, |o| matches!(o, Object::Stream))?;
        let GateSplitOperands {
            a,
            b,
            gate,
            u,
            v,
            singular_values,
        } = *operands;
        let call = SizedFor::GateSplit {
            descriptors: [a, b, gate, u, v].map(|operand| id(operand.descriptor)),
            config: id(config),
        };
        inner.sized_for(handle, workspace, call, OPERATION)?;
        inner.lent_scratch(workspace, OPERATION)?;
        // The workspace, this handle's, was sized for this config by
        // `gate_split_workspace_sizes`, which checked the config is the
        // handle's too.
        let Some(&Object::SvdConfig(settings)) = inner.objects.get(&id(config)) else {
            return Err(failed(OPERATION, "no such SVD config"));
        };
        inner.owned(handle, info, OPERATION)?;
        if !matches!(inner.objects.get(&id(info)), Some(Object::SvdInfo(_))) {
            return Err(failed(OPERATION, "no such SVD info"));
        }
        let (partition, cutoff) = settings.the_algebras(OPERATION)?;
        let inputs = [
            inner.operand(handle, a, OPERATION)?,
            inner.operand(handle, b, OPERATION)?,
            inner.operand(handle, gate, OPERATION)?,
        ];
        let u_shape = inner.descriptor(handle, u.descriptor, OPERATION)?.clone();
        let v_shape = inner.descriptor(handle, v.descriptor, OPERATION)?.clone();
        for (factor, shape) in [(u, &u_shape), (v, &v_shape)] {
            if inner.buffer(id(factor.data), OPERATION)?.len() < shape.elements()? {
                return Err(failed(
                    OPERATION,
                    "U's or V's buffer is smaller than its descriptor's extents",
                ));
            }
        }
        let sides = Sides::of(&u_shape, &v_shape, OPERATION)?;
        let doubles = 2 * inner.buffer(id(singular_values), OPERATION)?.len();
        if doubles < to_usize(sides.shared_extent)? {
            return Err(failed(
                OPERATION,
                "the singular values' buffer is smaller than the shared extent",
            ));
        }
        let inputs = [&inputs[0], &inputs[1], &inputs[2]];
        sides.check_gate_split_labels(&inputs, OPERATION)?;
        let (theta, rows, cols) = sides.matrix(&inputs, OPERATION)?;
        if to_usize(sides.shared_extent)? > rows.min(cols) {
            return Err(refused(OPERATION, FakeRule::SharedExtentExceedsSmallerSide));
        }
        let Split {
            left,
            right,
            reduced,
            discarded_weight,
        } = split(
            &theta,
            rows,
            cols,
            to_usize(sides.shared_extent)?,
            cutoff,
            partition,
        )
        .map_err(|message| failed(OPERATION, message))?;
        let reduced_extent = i64::try_from(reduced).map_err(|_| failed(OPERATION, "overflow"))?;
        let [u_tensor, v_tensor] = sides.factors(
            [left, right],
            reduced_extent,
            [&u_shape.modes, &v_shape.modes],
            OPERATION,
        )?;
        inner.write(u.data, &u_tensor, OPERATION)?;
        inner.write(v.data, &v_tensor, OPERATION)?;
        inner.set_extent(u.descriptor, sides.shared, reduced_extent);
        inner.set_extent(v.descriptor, sides.shared, reduced_extent);
        let full_extent =
            i64::try_from(rows.min(cols)).map_err(|_| failed(OPERATION, "overflow"))?;
        let reported = inner.misreported_truncation.unwrap_or(SvdTruncation {
            reduced_extent,
            full_extent,
            discarded_weight,
        });
        if let Some(Object::SvdInfo(truncation)) = inner.objects.get_mut(&id(info)) {
            *truncation = Some(reported);
        }
        let written = [id(u.data), id(v.data), id(singular_values), id(info)];
        inner.queue_on(stream, written);
        Ok(())
    }

    fn qr_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        input: OpaqueHandle,
        q: OpaqueHandle,
        r: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "qr_workspace_sizes";
        let mut inner = self.call(OPERATION)?;
        for descriptor in [input, q, r] {
            inner.descriptor(handle, descriptor, OPERATION)?;
        }
        let call = SizedFor::Qr {
            descriptors: [input, q, r].map(id),
        };
        inner.size_for(handle, workspace, call, OPERATION)
    }

    fn tensor_qr(
        &self,
        handle: OpaqueHandle,
        input: TensorOperand,
        q: TensorOperand,
        r: TensorOperand,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "tensor_qr";
        let mut inner = self.call(OPERATION)?;
        inner.require(stream, OPERATION, |o| matches!(o, Object::Stream))?;
        let call = SizedFor::Qr {
            descriptors: [input, q, r].map(|operand| id(operand.descriptor)),
        };
        inner.sized_for(handle, workspace, call, OPERATION)?;
        inner.lent_scratch(workspace, OPERATION)?;
        let tensor = inner.operand(handle, input, OPERATION)?;
        let q_shape = inner.descriptor(handle, q.descriptor, OPERATION)?.clone();
        let r_shape = inner.descriptor(handle, r.descriptor, OPERATION)?.clone();
        inner.buffer(id(q.data), OPERATION)?;
        inner.buffer(id(r.data), OPERATION)?;
        let sides = Sides::of(&q_shape, &r_shape, OPERATION)?;
        let free_modes = [sides.left.0.as_slice(), sides.right.0.as_slice()].concat();
        if tensor.modes.len() != free_modes.len()
            || !tensor.modes.iter().all(|mode| free_modes.contains(mode))
        {
            return Err(failed(
                OPERATION,
                "the input's modes are not Q's and R's other modes",
            ));
        }
        let (matrix, rows, cols) = sides.matrix(&[&tensor], OPERATION)?;
        if to_usize(sides.shared_extent)? != rows.min(cols) {
            return Err(refused(OPERATION, FakeRule::SharedExtentNotSmallerSide));
        }
        let Qr {
            q: q_factor,
            r: r_factor,
        } = Qr::of(MatrixRef::new(rows, cols, &matrix));
        let [q_tensor, r_tensor] = sides.factors(
            [q_factor.into_elements(), r_factor.into_elements()],
            sides.shared_extent,
            [&q_shape.modes, &r_shape.modes],
            OPERATION,
        )?;
        inner.write(q.data, &q_tensor, OPERATION)?;
        inner.write(r.data, &r_tensor, OPERATION)?;
        inner.queue_on(stream, [id(q.data), id(r.data)]);
        if let Some((slot, bytes)) = inner.slot_bytes_after_qr.take() {
            inner.slot_bytes[slot_index(slot)] = Some(bytes);
        }
        Ok(())
    }

    fn create_contraction_network(
        &self,
        handle: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        output_modes: &[i32],
    ) -> Result<NativeContractionNetwork, CudaAlgebraError> {
        const OPERATION: &str = "create_contraction_network";
        let mut inner = self.call(OPERATION)?;
        let inputs = [
            TensorShape::of(inputs[0], OPERATION)?,
            TensorShape::of(inputs[1], OPERATION)?,
        ];
        for mode in &inputs[0].modes {
            if let Some(extent) = inputs[1].extent(*mode)
                && inputs[0].extent(*mode) != Some(extent)
            {
                return Err(failed(OPERATION, "a label has two extents"));
            }
        }
        let output_extents = output_modes
            .iter()
            .map(|mode| {
                inputs
                    .iter()
                    .find_map(|input| input.extent(*mode))
                    .ok_or_else(|| failed(OPERATION, "an output label is in no input"))
            })
            .collect::<Result<Vec<i64>, _>>()?;
        let output = TensorShape::of(
            TensorModes {
                extents: &output_extents,
                modes: output_modes,
            },
            OPERATION,
        )?;
        let descriptor = inner.create_on(
            handle,
            OPERATION,
            Object::Network {
                inputs,
                output,
                path: None,
                prepared: None,
            },
        )?;
        Ok(NativeContractionNetwork {
            descriptor,
            input_ids: [0, 1],
        })
    }

    fn destroy_contraction_network(&self, network: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "destroy_contraction_network";
        let mut inner = self.call(OPERATION)?;
        let in_use = inner.has_part(id(network), |object, network| {
            matches!(object, Object::OptimizerInfo { network: of, .. } if *of == network)
        });
        if in_use {
            return Err(failed(
                OPERATION,
                "an optimizer info of the network is live",
            ));
        }
        inner.destroy(network, OPERATION, |o| matches!(o, Object::Network { .. }))
    }

    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, CudaAlgebraError> {
        const OPERATION: &str = "create_optimizer_info";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, network, OPERATION)?;
        inner.require(network, OPERATION, |o| matches!(o, Object::Network { .. }))?;
        inner.create_on(
            handle,
            OPERATION,
            Object::OptimizerInfo {
                network: id(network),
                optimized: false,
            },
        )
    }

    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        self.call("destroy_optimizer_info")?
            .destroy(info, "destroy_optimizer_info", |o| {
                matches!(o, Object::OptimizerInfo { .. })
            })
    }

    fn optimize_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        _workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "optimize_contraction";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, network, OPERATION)?;
        if inner.optimizer_info(handle, info, OPERATION)?.0 != id(network) {
            return Err(failed(
                OPERATION,
                "the optimizer info is for another network",
            ));
        }
        if let Some(Object::OptimizerInfo { optimized, .. }) = inner.objects.get_mut(&id(info)) {
            *optimized = true;
        }
        if let Some(Object::Network { path, .. }) = inner.objects.get_mut(&id(network)) {
            *path = Some(id(info));
        }
        // The search may find another path, even into the same info, and
        // a workspace sized for the old one may not hold the new one's.
        inner.forget_sizings(|_, call| {
            matches!(call, SizedFor::Contraction { network: of, .. } if of == id(network))
        });
        Ok(())
    }

    fn contraction_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "contraction_workspace_sizes";
        let mut inner = self.call(OPERATION)?;
        inner.path_of(handle, network, info, OPERATION)?;
        let call = SizedFor::Contraction {
            network: id(network),
            info: id(info),
        };
        inner.size_for(handle, workspace, call, OPERATION)
    }

    fn prepare_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "prepare_contraction";
        let mut inner = self.call(OPERATION)?;
        let (path, _) = inner.network(handle, network, OPERATION)?;
        // `sized_for` below refuses an unoptimized network too, since only
        // an optimized path can be sized for; this check names the cause.
        let info = path.ok_or_else(|| failed(OPERATION, "the network is not optimized"))?;
        let call = SizedFor::Contraction {
            network: id(network),
            info,
        };
        inner.sized_for(handle, workspace, call, OPERATION)?;
        // cuTensorNet validates the device scratch's size at preparation,
        // against the minimum the path needs, and `contract` then needs at
        // least the size set here (`cutensornetWorkspaceSetMemory`'s and
        // `cutensornetNetworkContract`'s notes, `generated/v2_13.rs`).
        let asked = inner.recommended_bytes(WorkspaceSlot::DeviceScratch);
        let scratch = inner.lent_scratch_bytes(workspace, OPERATION)?;
        if scratch < asked {
            return Err(failed(
                OPERATION,
                "the device scratch is lent less than it asks for",
            ));
        }
        if let Some(Object::Network { prepared, .. }) = inner.objects.get_mut(&id(network)) {
            *prepared = Some(scratch);
        }
        Ok(())
    }

    fn contract(
        &self,
        handle: OpaqueHandle,
        network: NativeContractionNetwork,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), CudaAlgebraError> {
        const OPERATION: &str = "contract";
        let mut inner = self.call(OPERATION)?;
        inner.require(stream, OPERATION, |o| matches!(o, Object::Stream))?;
        let network = network.descriptor;
        let (path, prepared) = inner.network(handle, network, OPERATION)?;
        let (Some(info), Some(prepared_scratch)) = (path, prepared) else {
            return Err(refused(OPERATION, FakeRule::NetworkNotPrepared));
        };
        inner.sized_for(
            handle,
            workspace,
            SizedFor::Contraction {
                network: id(network),
                info,
            },
            OPERATION,
        )?;
        inner.lent_scratch(workspace, OPERATION)?;
        if inner.lent_scratch_bytes(workspace, OPERATION)? < prepared_scratch {
            return Err(failed(
                OPERATION,
                "the device scratch is lent less than at the network's preparation",
            ));
        }
        let Some(Object::Network {
            inputs: shapes,
            output: output_shape,
            ..
        }) = inner.objects.get(&id(network))
        else {
            return Err(failed(OPERATION, "no such network"));
        };
        let mut tensors = Vec::with_capacity(2);
        for (shape, input) in shapes.iter().zip(inputs) {
            let data = inner
                .buffer(id(input), OPERATION)?
                .get(..shape.elements()?)
                .ok_or_else(|| failed(OPERATION, "a buffer is smaller than its tensor"))?
                .iter()
                .map(|&element| element.into())
                .collect();
            tensors.push(shape.dense(data)?);
        }
        let result = contract(&[&tensors[0], &tensors[1]], &output_shape.modes)
            .map_err(|message| failed(OPERATION, message))?;
        inner.write(output, &result, OPERATION)?;
        inner.queue_on(stream, [id(output)]);
        Ok(())
    }
}

impl Inner {
    /// The shape of `descriptor`, which must belong to `handle`.
    fn descriptor(
        &self,
        handle: OpaqueHandle,
        descriptor: OpaqueHandle,
        operation: &'static str,
    ) -> Result<&TensorShape, CudaAlgebraError> {
        self.owned(handle, descriptor, operation)?;
        match self.objects.get(&id(descriptor)) {
            Some(Object::TensorDescriptor(shape)) => Ok(shape),
            _ => Err(failed(operation, "no such tensor descriptor")),
        }
    }

    /// The tensor `operand`'s buffer holds, as its descriptor describes it.
    fn operand(
        &self,
        handle: OpaqueHandle,
        operand: TensorOperand,
        operation: &'static str,
    ) -> Result<DenseTensor, CudaAlgebraError> {
        let shape = self.descriptor(handle, operand.descriptor, operation)?;
        let data = self
            .buffer(id(operand.data), operation)?
            .get(..shape.elements()?)
            .ok_or_else(|| failed(operation, "a buffer is smaller than its tensor"))?
            .iter()
            .map(|&element| element.into())
            .collect();
        shape.dense(data)
    }

    /// Writes `tensor` to the start of buffer `data`.
    fn write(
        &mut self,
        data: OpaqueHandle,
        tensor: &DenseTensor,
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        let Some(Object::Buffer(buffer)) = self.objects.get_mut(&id(data)) else {
            return Err(failed(operation, "no such buffer"));
        };
        let written = buffer
            .get_mut(..tensor.data.len())
            .ok_or_else(|| failed(operation, "a buffer is smaller than its tensor"))?;
        for (element, &value) in written.iter_mut().zip(&tensor.data) {
            *element = value.into();
        }
        Ok(())
    }

    /// Sets the extent of `mode` in `descriptor`.
    fn set_extent(&mut self, descriptor: OpaqueHandle, mode: i32, extent: i64) {
        if let Some(Object::TensorDescriptor(shape)) = self.objects.get_mut(&id(descriptor))
            && let Some(at) = shape.modes.iter().position(|&m| m == mode)
        {
            shape.extents[at] = extent;
        }
    }
}
