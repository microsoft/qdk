// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `TensorCalls` of the fake libraries: tensor descriptors, gate splits,
//! QRs and pairwise contractions, computed on dense tensors
//! (`dense_tensor.rs`) and factored by `dense_linalg`'s SVD and QR. An SVD
//! that fails its accuracy checks fails the gate split.
//!
//! It is as strict as the state calls about what cuTensorNet needs:
//!
//! - every object (a descriptor, an SVD config or info, a network, an
//!   optimizer info, a plan) belongs to the handle it was created with;
//! - a gate split, a QR or a contraction runs only in a workspace last
//!   sized for that same call (the same descriptors and config, or the
//!   same network and path), and lent what each of its slots asks for;
//! - a plan needs an optimized path of its network and a workspace sized
//!   for both; a network outlives its optimizer infos and plans, and an
//!   optimizer info its plans;
//! - a gate split, a QR or a contraction runs on a live stream;
//! - a gate split takes A·B·G's topology: each input label is shared by
//!   two inputs and in neither output, or is in one input and one of U's
//!   or V's other modes, so no label is summed alone;
//! - the singular values' buffer must be live, and an SVD info is read
//!   only once the stream is synchronized.
//!
//! The optimizer finds an unsliced path unless told to report slices
//! ([`FakeLibraries::set_contraction_slices`]), to test the refusal of a
//! sliced one.
//!
//! A gate split's report and outputs follow cuTensorNet's: the reduced
//! extent is the lowest of the shared mode's extent in U's descriptor, the
//! count of singular values (the SVD leaves out those of rounding's size),
//! and the discarded-weight cutoff's rank; the full extent is the smaller
//! side of the split, whatever the SVD left out; U's and V's descriptors
//! are given the reduced extent. The fake absorbs the singular values into
//! U or V only, writes none to their buffer, and refuses a product of A, B
//! and the gate that is zero, whose singular vectors are arbitrary.

use super::{
    FakeLibraries, Inner, Object,
    dense_tensor::{DenseTensor, contract},
    failed, id, to_usize,
};
use crate::native_calls::error::NativeError;
use crate::native_calls::{
    OpaqueHandle, Stream,
    tensor_calls::{
        GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation, TensorCalls,
        TensorModes, TensorOperand,
    },
};
use dense_linalg::{MatrixRef, Qr, Svd};
use num_complex::Complex64;

impl TensorCalls for FakeLibraries {
    fn check_tensor_functions(&self) -> Result<(), NativeError> {
        self.call("check_tensor_functions").map(drop)
    }

    fn create_tensor_descriptor(
        &self,
        handle: OpaqueHandle,
        tensor: TensorModes<'_>,
    ) -> Result<OpaqueHandle, NativeError> {
        const OPERATION: &str = "create_tensor_descriptor";
        let mut inner = self.call(OPERATION)?;
        let shape = TensorShape::of(tensor, OPERATION)?;
        inner.create_on(handle, OPERATION, Object::TensorDescriptor(shape))
    }

    fn destroy_tensor_descriptor(&self, descriptor: OpaqueHandle) -> Result<(), NativeError> {
        self.call("destroy_tensor_descriptor")?.destroy(
            descriptor,
            "destroy_tensor_descriptor",
            |o| matches!(o, Object::TensorDescriptor(_)),
        )
    }

    fn create_svd_config(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError> {
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
    ) -> Result<(), NativeError> {
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
                if !(0.0..=1.0).contains(&cutoff) {
                    return Err(failed(OPERATION, "a cutoff outside [0, 1]"));
                }
                config.cutoff = cutoff;
            }
        }
        Ok(())
    }

    fn destroy_svd_config(&self, config: OpaqueHandle) -> Result<(), NativeError> {
        self.call("destroy_svd_config")?
            .destroy(config, "destroy_svd_config", |o| {
                matches!(o, Object::SvdConfig(_))
            })
    }

    fn create_svd_info(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError> {
        self.call("create_svd_info")?
            .create_on(handle, "create_svd_info", Object::SvdInfo(None))
    }

    fn svd_truncation(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<SvdTruncation, NativeError> {
        const OPERATION: &str = "svd_truncation";
        let inner = self.call(OPERATION)?;
        inner.owned(handle, info, OPERATION)?;
        if inner.stream_busy {
            return Err(failed(
                OPERATION,
                "the stream may still be writing the info",
            ));
        }
        match inner.objects.get(&id(info)) {
            Some(Object::SvdInfo(Some(truncation))) => Ok(*truncation),
            Some(Object::SvdInfo(None)) => Err(failed(OPERATION, "no gate split given this info")),
            _ => Err(failed(OPERATION, "no such SVD info")),
        }
    }

    fn destroy_svd_info(&self, info: OpaqueHandle) -> Result<(), NativeError> {
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
    ) -> Result<(), NativeError> {
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
    ) -> Result<(), NativeError> {
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
        inner.owned(handle, config, OPERATION)?;
        let Some(&Object::SvdConfig(settings)) = inner.objects.get(&id(config)) else {
            return Err(failed(OPERATION, "no such SVD config"));
        };
        inner.owned(handle, info, OPERATION)?;
        if !matches!(inner.objects.get(&id(info)), Some(Object::SvdInfo(_))) {
            return Err(failed(OPERATION, "no such SVD info"));
        }
        inner.buffer(id(singular_values), OPERATION)?;
        let Some(partition) = settings.partition else {
            return Err(failed(
                OPERATION,
                "the fake absorbs the singular values into U or V only",
            ));
        };
        let inputs = [
            inner.operand(handle, a, OPERATION)?,
            inner.operand(handle, b, OPERATION)?,
            inner.operand(handle, gate, OPERATION)?,
        ];
        let u_shape = inner.descriptor(handle, u.descriptor, OPERATION)?.clone();
        let v_shape = inner.descriptor(handle, v.descriptor, OPERATION)?.clone();
        inner.buffer(id(u.data), OPERATION)?;
        inner.buffer(id(v.data), OPERATION)?;
        let sides = Sides::of(&u_shape, &v_shape, OPERATION)?;
        let inputs = [&inputs[0], &inputs[1], &inputs[2]];
        sides.check_gate_split_labels(&inputs, OPERATION)?;
        let (theta, rows, cols) = sides.matrix(&inputs, OPERATION)?;
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
            settings,
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
        if let Some(Object::SvdInfo(truncation)) = inner.objects.get_mut(&id(info)) {
            *truncation = Some(SvdTruncation {
                reduced_extent,
                full_extent,
                discarded_weight,
            });
        }
        inner.stream_busy = true;
        Ok(())
    }

    fn qr_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        input: OpaqueHandle,
        q: OpaqueHandle,
        r: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
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
    ) -> Result<(), NativeError> {
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
            return Err(failed(
                OPERATION,
                "the shared extent is not the smaller side",
            ));
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
        inner.stream_busy = true;
        Ok(())
    }

    fn create_contraction_network(
        &self,
        handle: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        output: TensorModes<'_>,
    ) -> Result<OpaqueHandle, NativeError> {
        const OPERATION: &str = "create_contraction_network";
        let mut inner = self.call(OPERATION)?;
        let inputs = [
            TensorShape::of(inputs[0], OPERATION)?,
            TensorShape::of(inputs[1], OPERATION)?,
        ];
        let output = TensorShape::of(output, OPERATION)?;
        for (mode, extent) in output.modes.iter().zip(&output.extents) {
            let found: Vec<i64> = inputs
                .iter()
                .filter_map(|input| input.extent(*mode))
                .collect();
            if found.is_empty() {
                return Err(failed(OPERATION, "an output label is in no input"));
            }
            if found.iter().any(|found| found != extent) {
                return Err(failed(OPERATION, "a label has two extents"));
            }
        }
        for mode in &inputs[0].modes {
            if let Some(extent) = inputs[1].extent(*mode)
                && inputs[0].extent(*mode) != Some(extent)
            {
                return Err(failed(OPERATION, "a label has two extents"));
            }
        }
        inner.create_on(handle, OPERATION, Object::Network { inputs, output })
    }

    fn destroy_contraction_network(&self, network: OpaqueHandle) -> Result<(), NativeError> {
        const OPERATION: &str = "destroy_contraction_network";
        let mut inner = self.call(OPERATION)?;
        let in_use = inner.has_part(id(network), |object, network| {
            matches!(object, Object::OptimizerInfo { network: of, .. } | Object::Plan { network: of, .. } if *of == network)
        });
        if in_use {
            return Err(failed(
                OPERATION,
                "an optimizer info or a plan of the network is live",
            ));
        }
        inner.destroy(network, OPERATION, |o| matches!(o, Object::Network { .. }))
    }

    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError> {
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

    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), NativeError> {
        const OPERATION: &str = "destroy_optimizer_info";
        let mut inner = self.call(OPERATION)?;
        if inner.has_part(
            id(info),
            |object, info| matches!(object, Object::Plan { info: of, .. } if *of == info),
        ) {
            return Err(failed(OPERATION, "a plan of the optimizer info is live"));
        }
        inner.destroy(info, OPERATION, |o| {
            matches!(o, Object::OptimizerInfo { .. })
        })
    }

    fn find_contraction_path(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        _workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), NativeError> {
        const OPERATION: &str = "find_contraction_path";
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
        Ok(())
    }

    fn contraction_slice_count(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<i64, NativeError> {
        const OPERATION: &str = "contraction_slice_count";
        let inner = self.call(OPERATION)?;
        inner.optimizer_info(handle, info, OPERATION)?;
        Ok(inner.contraction_slices.unwrap_or(1))
    }

    fn contraction_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        const OPERATION: &str = "contraction_workspace_sizes";
        let mut inner = self.call(OPERATION)?;
        inner.path_of(handle, network, info, OPERATION)?;
        let call = SizedFor::Contraction {
            network: id(network),
            info: id(info),
        };
        inner.size_for(handle, workspace, call, OPERATION)
    }

    fn create_contraction_plan(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError> {
        const OPERATION: &str = "create_contraction_plan";
        let mut inner = self.call(OPERATION)?;
        inner.path_of(handle, network, info, OPERATION)?;
        let call = SizedFor::Contraction {
            network: id(network),
            info: id(info),
        };
        inner.sized_for(handle, workspace, call, OPERATION)?;
        inner.create_on(
            handle,
            OPERATION,
            Object::Plan {
                network: id(network),
                info: id(info),
            },
        )
    }

    fn destroy_contraction_plan(&self, plan: OpaqueHandle) -> Result<(), NativeError> {
        self.call("destroy_contraction_plan")?
            .destroy(plan, "destroy_contraction_plan", |o| {
                matches!(o, Object::Plan { .. })
            })
    }

    fn contract(
        &self,
        handle: OpaqueHandle,
        plan: OpaqueHandle,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError> {
        const OPERATION: &str = "contract";
        let mut inner = self.call(OPERATION)?;
        inner.require(stream, OPERATION, |o| matches!(o, Object::Stream))?;
        inner.owned(handle, plan, OPERATION)?;
        let Some(&Object::Plan { network, info }) = inner.objects.get(&id(plan)) else {
            return Err(failed(OPERATION, "no such plan"));
        };
        inner.sized_for(
            handle,
            workspace,
            SizedFor::Contraction { network, info },
            OPERATION,
        )?;
        inner.lent_scratch(workspace, OPERATION)?;
        let Some(Object::Network {
            inputs: shapes,
            output: output_shape,
        }) = inner.objects.get(&network)
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
        inner.stream_busy = true;
        Ok(())
    }
}

/// A tensor descriptor's modes: the extent and the label of each.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TensorShape {
    extents: Vec<i64>,
    modes: Vec<i32>,
}

/// An SVD configuration's settings the fake acts on.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SvdConfig {
    /// Which factor absorbs the singular values; none returns them apart.
    partition: Option<Partition>,
    normalized: bool,
    /// The discarded-weight cutoff; 0 truncates nothing.
    cutoff: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Partition {
    Us,
    Sv,
}

/// The call a workspace was last sized for, by its objects' ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SizedFor {
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

impl TensorShape {
    fn of(tensor: TensorModes<'_>, operation: &'static str) -> Result<Self, NativeError> {
        if tensor.extents.len() != tensor.modes.len() {
            return Err(failed(operation, "extents and modes of different lengths"));
        }
        if tensor.extents.iter().any(|&extent| extent < 1) {
            return Err(failed(operation, "an extent below 1"));
        }
        for (at, mode) in tensor.modes.iter().enumerate() {
            if tensor.modes[..at].contains(mode) {
                return Err(failed(operation, "a repeated mode"));
            }
        }
        Ok(Self {
            extents: tensor.extents.to_vec(),
            modes: tensor.modes.to_vec(),
        })
    }

    fn extent(&self, mode: i32) -> Option<i64> {
        self.modes
            .iter()
            .position(|&m| m == mode)
            .map(|at| self.extents[at])
    }

    /// The modes other than `shared`, and their extents.
    fn free_modes(&self, shared: i32) -> (Vec<i32>, Vec<i64>) {
        self.modes
            .iter()
            .zip(&self.extents)
            .filter(|&(&mode, _)| mode != shared)
            .map(|(&mode, &extent)| (mode, extent))
            .unzip()
    }

    fn elements(&self) -> Result<usize, NativeError> {
        product(&self.extents)
    }

    fn dense(&self, data: Vec<Complex64>) -> Result<DenseTensor, NativeError> {
        Ok(DenseTensor {
            extents: self
                .extents
                .iter()
                .map(|&extent| to_usize(extent))
                .collect::<Result<_, _>>()?,
            modes: self.modes.clone(),
            data,
        })
    }
}

/// The two outputs of a split (U and V, or Q and R): the one mode they
/// share and its extent, and each one's other modes and their extents, the
/// rows and the columns of the split's matrix.
struct Sides {
    shared: i32,
    shared_extent: i64,
    left: (Vec<i32>, Vec<i64>),
    right: (Vec<i32>, Vec<i64>),
}

impl Sides {
    fn of(
        left: &TensorShape,
        right: &TensorShape,
        operation: &'static str,
    ) -> Result<Self, NativeError> {
        let mut shared = left.modes.iter().filter(|mode| right.modes.contains(mode));
        let (Some(&shared), None) = (shared.next(), shared.next()) else {
            return Err(failed(operation, "the outputs share other than one mode"));
        };
        let shared_extent = left.extent(shared).unwrap_or(1);
        if right.extent(shared) != Some(shared_extent) {
            return Err(failed(operation, "the outputs' shared extents differ"));
        }
        Ok(Self {
            shared,
            shared_extent,
            left: left.free_modes(shared),
            right: right.free_modes(shared),
        })
    }

    /// Fails unless every label of a gate split's `inputs` (A, B and the
    /// gate) is either shared by two of them and in neither output, or in
    /// one of them and one output's other modes: no label is summed alone,
    /// and none is both contracted and kept.
    fn check_gate_split_labels(
        &self,
        inputs: &[&DenseTensor],
        operation: &'static str,
    ) -> Result<(), NativeError> {
        let kept = |mode: &i32| self.left.0.contains(mode) || self.right.0.contains(mode);
        for input in inputs {
            for mode in &input.modes {
                let count = inputs
                    .iter()
                    .filter(|other| other.modes.contains(mode))
                    .count();
                let fits = match count {
                    1 => kept(mode),
                    2 => !kept(mode) && *mode != self.shared,
                    _ => false,
                };
                if !fits {
                    return Err(failed(
                        operation,
                        "an input label is outside a gate split's topology",
                    ));
                }
            }
        }
        Ok(())
    }

    /// The contraction of `inputs` as a column-major matrix, its rows the
    /// left output's other modes and its columns the right's; and its rows
    /// and columns.
    fn matrix(
        &self,
        inputs: &[&DenseTensor],
        operation: &'static str,
    ) -> Result<(Vec<Complex64>, usize, usize), NativeError> {
        let modes = [self.left.0.as_slice(), self.right.0.as_slice()].concat();
        let matrix = contract(inputs, &modes).map_err(|message| failed(operation, message))?;
        let extents: Vec<usize> = [self.left.1.as_slice(), self.right.1.as_slice()]
            .concat()
            .into_iter()
            .map(to_usize)
            .collect::<Result<_, _>>()?;
        if matrix.extents != extents {
            return Err(failed(
                operation,
                "an output's extent differs from the inputs'",
            ));
        }
        Ok((matrix.data, product(&self.left.1)?, product(&self.right.1)?))
    }

    /// The factors `matrices`, left (rows × `extent`) and right (`extent` ×
    /// columns), both column-major, laid out in their descriptors' mode
    /// orders `layouts`.
    fn factors(
        &self,
        matrices: [Vec<Complex64>; 2],
        extent: i64,
        layouts: [&[i32]; 2],
        operation: &'static str,
    ) -> Result<[DenseTensor; 2], NativeError> {
        let [left, right] = matrices;
        let left = TensorShape {
            extents: [self.left.1.as_slice(), &[extent]].concat(),
            modes: [self.left.0.as_slice(), &[self.shared]].concat(),
        }
        .dense(left)?;
        let right = TensorShape {
            extents: [&[extent], self.right.1.as_slice()].concat(),
            modes: [&[self.shared], self.right.0.as_slice()].concat(),
        }
        .dense(right)?;
        let laid_out = |matrix: &DenseTensor, layout: &[i32]| {
            contract(&[matrix], layout).map_err(|message| failed(operation, message))
        };
        Ok([laid_out(&left, layouts[0])?, laid_out(&right, layouts[1])?])
    }
}

/// A gate split's factors: U (rows×reduced) and V (reduced×cols), both
/// column-major, one of them holding the kept singular values; and the
/// weight the truncation dropped.
struct Split {
    left: Vec<Complex64>,
    right: Vec<Complex64>,
    reduced: usize,
    discarded_weight: f64,
}

/// Factors the `rows`×`cols` column-major `theta` as `settings` say, into
/// at most `max_extent` singular values: drops the smallest whose squared
/// sum is at most the cutoff's fraction of the total, normalizes the kept
/// ones if asked, and absorbs them into U or V. Fails if `theta` is zero.
fn split(
    theta: &[Complex64],
    rows: usize,
    cols: usize,
    max_extent: usize,
    settings: SvdConfig,
    partition: Partition,
) -> Result<Split, &'static str> {
    let Svd {
        u,
        singular_values: values,
        v_adjoint,
    } = Svd::of(MatrixRef::new(rows, cols, theta)).map_err(|_| "the SVD failed")?;
    let (left, right) = (u.elements(), v_adjoint.elements());
    let squares: Vec<f64> = values.iter().map(|value| value * value).collect();
    let total: f64 = squares.iter().sum();
    if total <= 0.0 {
        return Err("the gate split's product is zero");
    }
    let rank = values.len();
    let mut reduced = rank.min(max_extent);
    if settings.cutoff > 0.0 {
        let mut discarded = 0.0;
        let dropped = squares
            .iter()
            .rev()
            .take_while(|&&square| {
                discarded += square;
                discarded <= settings.cutoff * total
            })
            .count();
        reduced = reduced.min((rank - dropped).max(1));
    }
    let discarded_weight = squares[reduced..].iter().sum::<f64>() / total;
    let mut kept = values[..reduced].to_vec();
    if settings.normalized {
        let norm = kept.iter().map(|value| value * value).sum::<f64>().sqrt();
        if norm > 0.0 {
            for value in &mut kept {
                *value /= norm;
            }
        }
    }
    let mut u_matrix = left[..rows * reduced].to_vec();
    let mut v_matrix: Vec<Complex64> = (0..cols)
        .flat_map(|col| (0..reduced).map(move |row| (row, col)))
        .map(|(row, col)| right[row + col * rank])
        .collect();
    match partition {
        Partition::Us => {
            for (at, element) in u_matrix.iter_mut().enumerate() {
                *element *= kept[at / rows];
            }
        }
        Partition::Sv => {
            for (at, element) in v_matrix.iter_mut().enumerate() {
                *element *= kept[at % reduced];
            }
        }
    }
    Ok(Split {
        left: u_matrix,
        right: v_matrix,
        reduced,
        discarded_weight,
    })
}

fn product(extents: &[i64]) -> Result<usize, NativeError> {
    extents
        .iter()
        .try_fold(1, |product: usize, &extent| Ok(product * to_usize(extent)?))
}

impl Inner {
    /// The shape of `descriptor`, which must belong to `handle`.
    fn descriptor(
        &self,
        handle: OpaqueHandle,
        descriptor: OpaqueHandle,
        operation: &'static str,
    ) -> Result<&TensorShape, NativeError> {
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
    ) -> Result<DenseTensor, NativeError> {
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
    ) -> Result<(), NativeError> {
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

    /// Records that `workspace`, which must belong to `handle`, is sized for
    /// `call`.
    fn size_for(
        &mut self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        call: SizedFor,
        operation: &'static str,
    ) -> Result<(), NativeError> {
        self.owned(handle, workspace, operation)?;
        match self.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { sized_for, .. }) => {
                *sized_for = Some(call);
                Ok(())
            }
            _ => Err(failed(operation, "no such workspace")),
        }
    }

    /// Fails unless `workspace`, which must belong to `handle`, was last
    /// sized for `call`.
    fn sized_for(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        call: SizedFor,
        operation: &'static str,
    ) -> Result<(), NativeError> {
        self.owned(handle, workspace, operation)?;
        match self.objects.get(&id(workspace)) {
            Some(Object::Workspace { sized_for, .. }) if *sized_for == Some(call) => Ok(()),
            Some(Object::Workspace { .. }) => Err(failed(
                operation,
                "the workspace is not sized for this call",
            )),
            _ => Err(failed(operation, "no such workspace")),
        }
    }

    /// The network an optimizer info, which must belong to `handle`, is
    /// for, and whether it holds a path.
    fn optimizer_info(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        operation: &'static str,
    ) -> Result<(usize, bool), NativeError> {
        self.owned(handle, info, operation)?;
        match self.objects.get(&id(info)) {
            Some(&Object::OptimizerInfo { network, optimized }) => Ok((network, optimized)),
            _ => Err(failed(operation, "no such optimizer info")),
        }
    }

    /// Fails unless `info` holds an optimized path of `network`.
    fn path_of(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        operation: &'static str,
    ) -> Result<(), NativeError> {
        self.owned(handle, network, operation)?;
        match self.optimizer_info(handle, info, operation)? {
            (of, _) if of != id(network) => Err(failed(
                operation,
                "the optimizer info is for another network",
            )),
            (_, false) => Err(failed(operation, "the path is not optimized")),
            _ => Ok(()),
        }
    }

    /// Whether a live object is `is_part` of `whole`.
    fn has_part(&self, whole: usize, is_part: impl Fn(&Object, usize) -> bool) -> bool {
        self.objects.values().any(|object| is_part(object, whole))
    }
}
