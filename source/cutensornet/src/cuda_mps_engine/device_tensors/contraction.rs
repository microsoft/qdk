// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): the pairwise
//! contractions of [`DeviceTensors`]'s QR moves (a one-site gate into a
//! site, a QR's small factor into the new centre), on the device.
//!
//! They go through cuTensorNet's network-descriptor API (deprecated in
//! cuTensorNet 2.x, still in 2.13, and the shortest path to one contraction
//! on the device): a network of two inputs, its path optimised, and a
//! contraction plan, all made once per shape and kept, since a run's bonds
//! repeat a few hundred shapes; beyond [`PLANS_KEPT`] the cache starts
//! again.

use super::{DeviceTensors, failed};
use crate::cuda_mps_engine::CudaMpsError;
use crate::dev_gate_split::{Status, check_tensor};
use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::cutensornet::v2_13;
use crate::libraries::loader::function_table;
use std::collections::BTreeMap;
use std::ffi::c_void;

/// The plans kept before the cache is emptied.
const PLANS_KEPT: usize = 512;

/// The largest device workspace a contraction's path may need: a pairwise
/// contraction of MPS sites needs far less.
const WORKSPACE_LIMIT: u64 = 1 << 30;

/// One input of a contraction: its device data, extents and modes, the
/// first mode fastest.
pub(super) struct Operand<'a> {
    pub(super) data: *const c_void,
    pub(super) extents: &'a [usize],
    pub(super) modes: &'a [u8],
}

/// The output of a contraction: its device data, extents and modes; the
/// modes of the inputs it lacks are summed.
pub(super) struct Output<'a> {
    pub(super) data: *mut c_void,
    pub(super) extents: &'a [usize],
    pub(super) modes: &'a [u8],
}

impl DeviceTensors {
    /// `output = Σ a·b`, waited for.
    pub(super) fn contract(
        &self,
        a: &Operand<'_>,
        b: &Operand<'_>,
        output: &Output<'_>,
    ) -> Result<(), CudaMpsError> {
        let key: ShapeKey = (
            [a.modes.to_vec(), b.modes.to_vec(), output.modes.to_vec()],
            [
                a.extents.to_vec(),
                b.extents.to_vec(),
                output.extents.to_vec(),
            ],
        );
        let mut plans = self.plans.borrow_mut();
        if !plans.contains_key(&key) {
            if plans.len() >= PLANS_KEPT {
                plans.clear();
            }
            let network = self.network(&key)?;
            plans.insert(key.clone(), network);
        }
        let plan = plans
            .get_mut(&key)
            .expect("the shape's network was just made");
        let functions = &self.contraction;
        let workspace =
            self.prepare_workspace("cutensornetWorkspaceComputeContractionSizes", |workspace| {
                // SAFETY: the network, its optimizer info and the workspace
                // descriptor are live.
                unsafe {
                    (functions.workspace_contraction_sizes)(
                        self.context.handle,
                        plan.descriptor,
                        plan.info,
                        workspace,
                    )
                }
            })?;
        if plan.plan.is_null() {
            // SAFETY: the network and its info are live; the workspace
            // descriptor holds the sizes just computed.
            let status = unsafe {
                (functions.create_plan)(
                    self.context.handle,
                    plan.descriptor,
                    plan.info,
                    workspace,
                    &raw mut plan.plan,
                )
            };
            check_tensor("cutensornetCreateContractionPlan", status).map_err(failed)?;
        }
        let inputs = [a.data, b.data];
        // SAFETY: the inputs and the output hold the extents of the
        // network's tensors; the plan and the workspace are live, and the
        // workspace holds the sizes this plan needs.
        let status = unsafe {
            (functions.contraction)(
                self.context.handle,
                plan.plan,
                inputs.as_ptr(),
                output.data,
                workspace,
                0,
                self.context.stream,
            )
        };
        check_tensor("cutensornetContraction", status).map_err(failed)?;
        // The next call may resize the workspace this one uses.
        self.synchronize()
    }

    /// The network of `key`, its path optimised; its plan is made at its
    /// first contraction, once the workspace holds its sizes.
    fn network(&self, key: &ShapeKey) -> Result<Network, CudaMpsError> {
        let functions = &self.contraction;
        let (modes, extents) = key;
        let extents: Vec<Vec<i64>> = extents
            .iter()
            .map(|tensor| {
                tensor
                    .iter()
                    .map(|&extent| i64::try_from(extent).expect("an extent fits in i64"))
                    .collect()
            })
            .collect();
        let modes: Vec<Vec<i32>> = modes
            .iter()
            .map(|tensor| tensor.iter().map(|&mode| i32::from(mode)).collect())
            .collect();
        let count = |modes: &[i32]| i32::try_from(modes.len()).expect("a tensor has few modes");
        let modes_in = [count(&modes[0]), count(&modes[1])];
        let extents_in = [extents[0].as_ptr(), extents[1].as_ptr()];
        let labels_in = [modes[0].as_ptr(), modes[1].as_ptr()];
        let mut network = Network {
            descriptor: std::ptr::null_mut(),
            info: std::ptr::null_mut(),
            plan: std::ptr::null_mut(),
            destroy_descriptor: functions.destroy_network,
            destroy_info: functions.destroy_info,
            destroy_plan: functions.destroy_plan,
        };
        // SAFETY: two inputs, each with as many extents and modes as
        // `modes_in` says, and the output's; null strides and qualifiers
        // are the defaults (the first mode fastest); the handle is live.
        let status = unsafe {
            (functions.create_network)(
                self.context.handle,
                2,
                modes_in.as_ptr(),
                extents_in.as_ptr(),
                std::ptr::null(),
                labels_in.as_ptr(),
                std::ptr::null(),
                count(&modes[2]),
                extents[2].as_ptr(),
                std::ptr::null(),
                modes[2].as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                &raw mut network.descriptor,
            )
        };
        check_tensor("cutensornetCreateNetworkDescriptor", status).map_err(failed)?;
        let mut config = OptimizerConfig {
            config: std::ptr::null_mut(),
            destroy: functions.destroy_config,
        };
        // SAFETY: `config` is a valid out-pointer; the handle is live.
        let status =
            unsafe { (functions.create_config)(self.context.handle, &raw mut config.config) };
        check_tensor("cutensornetCreateContractionOptimizerConfig", status).map_err(failed)?;
        // SAFETY: the network is live; `info` is a valid out-pointer.
        let status = unsafe {
            (functions.create_info)(
                self.context.handle,
                network.descriptor,
                &raw mut network.info,
            )
        };
        check_tensor("cutensornetCreateContractionOptimizerInfo", status).map_err(failed)?;
        // SAFETY: the network, the config and the info are live.
        let status = unsafe {
            (functions.optimize)(
                self.context.handle,
                network.descriptor,
                config.config,
                WORKSPACE_LIMIT,
                network.info,
            )
        };
        check_tensor("cutensornetContractionOptimize", status).map_err(failed)?;
        Ok(network)
    }
}

/// The modes and the extents of a contraction's two inputs and output.
type ShapeKey = ([Vec<u8>; 3], [Vec<usize>; 3]);

/// The cached networks, by shape.
pub(super) type Plans = BTreeMap<ShapeKey, Network>;

/// A network, its optimised path and, once made, its plan; destroyed in
/// the reverse order.
pub(super) struct Network {
    descriptor: v2_13::cutensornetNetworkDescriptor_t,
    info: v2_13::cutensornetContractionOptimizerInfo_t,
    plan: v2_13::cutensornetContractionPlan_t,
    destroy_descriptor: DestroyNetworkFn,
    destroy_info: DestroyInfoFn,
    destroy_plan: DestroyPlanFn,
}

impl Drop for Network {
    fn drop(&mut self) {
        // SAFETY: each object, when made, is destroyed once, the plan
        // before the info and the info before the network.
        unsafe {
            if !self.plan.is_null() {
                let _ = (self.destroy_plan)(self.plan);
            }
            if !self.info.is_null() {
                let _ = (self.destroy_info)(self.info);
            }
            if !self.descriptor.is_null() {
                let _ = (self.destroy_descriptor)(self.descriptor);
            }
        }
    }
}

/// An optimizer config, used once per network and destroyed after.
struct OptimizerConfig {
    config: v2_13::cutensornetContractionOptimizerConfig_t,
    destroy: DestroyConfigFn,
}

impl Drop for OptimizerConfig {
    fn drop(&mut self) {
        if !self.config.is_null() {
            // SAFETY: created by
            // `cutensornetCreateContractionOptimizerConfig`, destroyed once.
            let _ = unsafe { (self.destroy)(self.config) };
        }
    }
}

type CreateNetworkFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    i32,
    *const i32,
    *const *const i64,
    *const *const i64,
    *const *const i32,
    *const v2_13::cutensornetTensorQualifiers_t,
    i32,
    *const i64,
    *const i64,
    *const i32,
    v2_13::cudaDataType_t,
    v2_13::cutensornetComputeType_t,
    *mut v2_13::cutensornetNetworkDescriptor_t,
) -> Status;
type DestroyNetworkFn = unsafe extern "C" fn(v2_13::cutensornetNetworkDescriptor_t) -> Status;
type CreateConfigFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetContractionOptimizerConfig_t,
) -> Status;
type DestroyConfigFn =
    unsafe extern "C" fn(v2_13::cutensornetContractionOptimizerConfig_t) -> Status;
type CreateInfoFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    *mut v2_13::cutensornetContractionOptimizerInfo_t,
) -> Status;
type DestroyInfoFn = unsafe extern "C" fn(v2_13::cutensornetContractionOptimizerInfo_t) -> Status;
type OptimizeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerConfig_t,
    u64,
    v2_13::cutensornetContractionOptimizerInfo_t,
) -> Status;
type WorkspaceContractionSizesFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerInfo_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> Status;
type CreatePlanFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerInfo_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
    *mut v2_13::cutensornetContractionPlan_t,
) -> Status;
type DestroyPlanFn = unsafe extern "C" fn(v2_13::cutensornetContractionPlan_t) -> Status;
type ContractionFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetContractionPlan_t,
    *const *const c_void,
    *mut c_void,
    v2_13::cutensornetWorkspaceDescriptor_t,
    i64,
    cudart_12::cudaStream_t,
) -> Status;

function_table! {
    /// The cuTensorNet functions of [`DeviceTensors::contract`].
    struct ContractionFunctions, declared in v2_13 {
        required create_network: CreateNetworkFn = cutensornetCreateNetworkDescriptor,
        required destroy_network: DestroyNetworkFn = cutensornetDestroyNetworkDescriptor,
        required create_config: CreateConfigFn = cutensornetCreateContractionOptimizerConfig,
        required destroy_config: DestroyConfigFn = cutensornetDestroyContractionOptimizerConfig,
        required create_info: CreateInfoFn = cutensornetCreateContractionOptimizerInfo,
        required destroy_info: DestroyInfoFn = cutensornetDestroyContractionOptimizerInfo,
        required optimize: OptimizeFn = cutensornetContractionOptimize,
        required workspace_contraction_sizes: WorkspaceContractionSizesFn =
            cutensornetWorkspaceComputeContractionSizes,
        required create_plan: CreatePlanFn = cutensornetCreateContractionPlan,
        required destroy_plan: DestroyPlanFn = cutensornetDestroyContractionPlan,
        required contraction: ContractionFn = cutensornetContraction,
    }
}
