// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the native calls: the rules of cuTensorNet and the CUDA
//! Runtime that the fake libraries hold their callers to, one file per
//! trait (`memory_calls.rs`, `session_calls.rs`, `tensor_calls.rs`), and
//! the native calls' own code (`complex.rs`). Each rule's test pins it with
//! the cases it accepts and those it refuses.
//!
//! The CUDA algebra's tests (`src/tests/engine/cuda_algebra.rs`) make no
//! misuse, so they reach only the accepted side of each rule; only a
//! direct caller of the native calls can make the misuse a rule refuses,
//! so these tests call them directly. A table whose cases a GPU can run
//! takes the device its cases run on, and runs on the fake here and on
//! cuTensorNet in `on_the_gpu` (ignored, run by hand on a CUDA host), so
//! that the fake is checked to refuse what the library refuses. The other
//! cases run on the fake alone, each table saying why: on a GPU their
//! misuse is undefined behaviour or goes undetected, or the fake refuses a
//! tensor its dense tensors cannot represent.
//!
//! This file holds what the tables share: the `Device` a table makes its
//! objects through, which releases them when dropped, so that a leak check
//! of the GPU run reports none of the tables' own, and the calls the
//! tables misuse (a gate split, a QR, a contraction).

mod complex;
mod memory_calls;
mod session_calls;
mod tensor_calls;

use crate::engine::{
    CudaAlgebraError,
    native_calls::{
        NativeCalls, OpaqueHandle, Stream,
        complex::Complex64Abi,
        fake_libraries::{FakeLibraries, FakeRule},
        memory_calls::{MemoryCalls, WorkspaceSlot},
        session_calls::SessionCalls,
        tensor_calls::{
            GateSplitDescriptors, GateSplitOperands, NativeContractionNetwork, SvdSetting,
            TensorCalls, TensorModes, TensorOperand,
        },
    },
};
use memory_calls::a_workspace_slot_is_lent_what_it_asks_on;
use num_complex::Complex64;
use std::{cell::RefCell, ptr::NonNull, sync::Arc};
use tensor_calls::{
    a_contraction_follows_its_steps_in_order_on, a_contractions_labels_are_consistent_on,
    a_factorizations_bond_is_at_most_its_smaller_side_on,
    a_gate_split_takes_only_its_topologys_labels_on, a_malformed_descriptor_is_refused_on,
    a_qr_splits_its_input_modes_between_its_two_factors_on,
    an_svd_cutoff_outside_zero_to_one_is_refused_on,
};

/// The tables in `on_the_gpu` run under a leak check on a CUDA host: each
/// case releases every object its devices make, so that a leak reported
/// there is not the case's own.
#[test]
fn every_object_a_contract_case_makes_is_released() {
    let tables: [fn(&dyn Fn() -> Device); 8] = [
        a_workspace_slot_is_lent_what_it_asks_on,
        a_malformed_descriptor_is_refused_on,
        a_factorizations_bond_is_at_most_its_smaller_side_on,
        a_gate_split_takes_only_its_topologys_labels_on,
        a_qr_splits_its_input_modes_between_its_two_factors_on,
        a_contraction_follows_its_steps_in_order_on,
        a_contractions_labels_are_consistent_on,
        an_svd_cutoff_outside_zero_to_one_is_refused_on,
    ];
    for (at, table) in tables.into_iter().enumerate() {
        let fake = Arc::new(FakeLibraries::new());
        table(&|| Device::on(Arc::clone(&fake)));
        assert_eq!(fake.live_objects(), 0, "the table at {at} leaves objects");
    }
}

/// The tables that run on a GPU, on cuTensorNet: a case the fake refuses
/// and the library accepts shows a rule of the fake wider than the
/// library's. Their refusals are not matched by reason, which the
/// library's statuses do not name.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod on_the_gpu {
    use super::{
        Device, TestedCalls, a_contraction_follows_its_steps_in_order_on,
        a_contractions_labels_are_consistent_on,
        a_factorizations_bond_is_at_most_its_smaller_side_on,
        a_gate_split_takes_only_its_topologys_labels_on, a_malformed_descriptor_is_refused_on,
        a_qr_splits_its_input_modes_between_its_two_factors_on,
        a_workspace_slot_is_lent_what_it_asks_on, an_svd_cutoff_outside_zero_to_one_is_refused_on,
    };
    use crate::{engine::native_calls::session_calls::SessionCalls, libraries::LoadedLibraries};
    use std::sync::Arc;

    impl TestedCalls for LoadedLibraries {
        const NAMES_ITS_RULE: bool = false;
    }

    /// Runs `table` on devices of the libraries discovery opens, on CUDA
    /// device 0.
    fn on_the_gpu(table: fn(&dyn Fn() -> Device<LoadedLibraries>)) {
        let libraries = Arc::clone(
            crate::discover()
                .expect("this host has a CUDA device and the libraries")
                .libraries(),
        );
        table(&|| {
            libraries.set_device(0).expect("the host has device 0");
            Device::on(Arc::clone(&libraries))
        });
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_workspace_slot_is_lent_what_it_asks() {
        on_the_gpu(a_workspace_slot_is_lent_what_it_asks_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_malformed_descriptor_is_refused() {
        on_the_gpu(a_malformed_descriptor_is_refused_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_factorizations_bond_is_at_most_its_smaller_side() {
        on_the_gpu(a_factorizations_bond_is_at_most_its_smaller_side_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_gate_split_takes_only_its_topologys_labels() {
        on_the_gpu(a_gate_split_takes_only_its_topologys_labels_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_qr_splits_its_input_modes_between_its_two_factors() {
        on_the_gpu(a_qr_splits_its_input_modes_between_its_two_factors_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_contraction_follows_its_steps_in_order() {
        on_the_gpu(a_contraction_follows_its_steps_in_order_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_contractions_labels_are_consistent() {
        on_the_gpu(a_contractions_labels_are_consistent_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn an_svd_cutoff_outside_zero_to_one_is_refused() {
        on_the_gpu(an_svd_cutoff_outside_zero_to_one_is_refused_on);
    }
}

/// The native calls a table runs on: the fake libraries in every host test,
/// the loaded libraries in the GPU tests (`on_the_gpu`).
trait TestedCalls: NativeCalls {
    /// Whether a refusal's status names the rule it enforces, as the
    /// fake's do ([`FakeRule`]), so that a case checks it was refused by
    /// that rule; cuTensorNet's statuses name a class of error, not the
    /// rule.
    const NAMES_ITS_RULE: bool;
}

impl TestedCalls for FakeLibraries {
    const NAMES_ITS_RULE: bool = true;
}

/// Whether `refused` is a refusal by `rule`, on calls that name their
/// rule; any refusal, on calls that do not.
fn refused_for<C: TestedCalls>(refused: &CudaAlgebraError, rule: FakeRule) -> bool {
    !C::NAMES_ITS_RULE
        || matches!(
            refused,
            CudaAlgebraError::NativeCallFailed { status, .. } if *status == rule.status()
        )
}

/// What every tensor call needs: a handle, a stream and a workspace, made
/// through `calls`. The device owns every object it makes, and releases
/// them when dropped, once its streams are synchronized, so that a table
/// leaves nothing live for the GPU run's leak check to report.
struct Device<C: TestedCalls = FakeLibraries> {
    calls: Arc<C>,
    handle: OpaqueHandle,
    stream: Stream,
    workspace: OpaqueHandle,
    /// Every object the device made and the test did not release itself.
    owned: RefCell<Vec<(NativeObject, OpaqueHandle)>>,
    /// The host memory lent to host slots, kept as long as the device.
    host_memory: RefCell<Vec<Vec<u8>>>,
}

/// The kinds of native object a [`Device`] owns, in the order it releases
/// them: what refers to another object before it (an optimizer info
/// before its network), the objects of a handle before the handle, and a
/// workspace before the memory lent to it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NativeObject {
    SvdInfo,
    SvdConfig,
    OptimizerInfo,
    Network,
    TensorDescriptor,
    Workspace,
    Buffer,
    Stream,
    Handle,
}

impl NativeObject {
    /// Releases `object`, of this kind, through `calls`.
    fn release(
        self,
        calls: &impl NativeCalls,
        object: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        match self {
            Self::SvdInfo => calls.destroy_svd_info(object),
            Self::SvdConfig => calls.destroy_svd_config(object),
            Self::OptimizerInfo => calls.destroy_optimizer_info(object),
            Self::Network => calls.destroy_contraction_network(object),
            Self::TensorDescriptor => calls.destroy_tensor_descriptor(object),
            Self::Workspace => calls.destroy_workspace(object),
            Self::Buffer => calls.free(object),
            Self::Stream => calls.destroy_stream(object),
            Self::Handle => calls.destroy_handle(object),
        }
    }
}

impl<C: TestedCalls> Drop for Device<C> {
    /// Synchronizes the device's streams and copies, then releases what it
    /// owns; a release refused fails the test, unless it already failed.
    fn drop(&mut self) {
        let mut owned = std::mem::take(self.owned.get_mut());
        owned.sort_by_key(|&(kind, _)| kind);
        let calls = &*self.calls;
        let mut refused = Vec::new();
        let streams = owned
            .iter()
            .filter(|(kind, _)| *kind == NativeObject::Stream);
        for &(_, stream) in streams {
            refused.extend(calls.synchronize_stream(stream).err());
        }
        refused.extend(calls.synchronize_copies().err());
        for (kind, object) in owned {
            refused.extend(kind.release(calls, object).err());
        }
        assert!(
            refused.is_empty() || std::thread::panicking(),
            "the device releases every object it made: {refused:?}"
        );
    }
}

/// A device on new fake libraries.
fn fake() -> Device {
    Device::new(FakeLibraries::new())
}

impl Device {
    fn new(fake: FakeLibraries) -> Self {
        Self::on(Arc::new(fake))
    }
}

impl<C: TestedCalls> Device<C> {
    /// A device whose handle, stream and workspace `calls` makes, on the
    /// device the calling thread has selected.
    fn on(calls: Arc<C>) -> Self {
        let handle = calls.create_handle().expect("a device is selected");
        let stream = calls.create_stream().expect("a device is selected");
        let workspace = calls.create_workspace(handle).expect("the handle is live");
        Self {
            calls,
            handle,
            stream,
            workspace,
            owned: RefCell::new(vec![
                (NativeObject::Handle, handle),
                (NativeObject::Stream, stream),
                (NativeObject::Workspace, workspace),
            ]),
            host_memory: RefCell::default(),
        }
    }

    /// Owns `object`, of `kind`, until the device is dropped.
    fn own(&self, kind: NativeObject, object: OpaqueHandle) -> OpaqueHandle {
        self.owned.borrow_mut().push((kind, object));
        object
    }

    /// Gives up `object`, which the test released itself.
    fn disown(&self, object: OpaqueHandle) {
        self.owned
            .borrow_mut()
            .retain(|&(_, owned)| owned != object);
    }

    /// Gives up every buffer, which the fake refuses to free for good once
    /// a destroyed stream holds writes pending.
    fn disown_buffers(&self) {
        self.owned
            .borrow_mut()
            .retain(|&(kind, _)| kind != NativeObject::Buffer);
    }

    /// A handle beside the device's, owned.
    fn another_handle(&self) -> Result<OpaqueHandle, CudaAlgebraError> {
        let handle = self.calls.create_handle()?;
        Ok(self.own(NativeObject::Handle, handle))
    }

    /// A stream beside the device's, owned.
    fn another_stream(&self) -> Result<Stream, CudaAlgebraError> {
        let stream = self.calls.create_stream()?;
        Ok(self.own(NativeObject::Stream, stream))
    }

    /// A workspace of the device's handle beside its own, owned.
    fn another_workspace(&self) -> Result<OpaqueHandle, CudaAlgebraError> {
        let workspace = self.calls.create_workspace(self.handle)?;
        Ok(self.own(NativeObject::Workspace, workspace))
    }

    /// A device allocation of `bytes`, owned.
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, CudaAlgebraError> {
        let buffer = self.calls.allocate(bytes)?;
        Ok(self.own(NativeObject::Buffer, buffer))
    }

    /// A tensor descriptor of `modes` on the device's handle, owned.
    fn descriptor(&self, modes: TensorModes<'_>) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.descriptor_on(self.handle, modes)
    }

    /// A tensor descriptor of `modes` on `handle`, owned.
    fn descriptor_on(
        &self,
        handle: OpaqueHandle,
        modes: TensorModes<'_>,
    ) -> Result<OpaqueHandle, CudaAlgebraError> {
        let descriptor = self.calls.create_tensor_descriptor(handle, modes)?;
        Ok(self.own(NativeObject::TensorDescriptor, descriptor))
    }

    /// An SVD config on the device's handle, owned.
    fn svd_config(&self) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.svd_config_on(self.handle)
    }

    /// An SVD config on `handle`, owned.
    fn svd_config_on(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError> {
        let config = self.calls.create_svd_config(handle)?;
        Ok(self.own(NativeObject::SvdConfig, config))
    }

    /// An SVD info on the device's handle, owned.
    fn svd_info(&self) -> Result<OpaqueHandle, CudaAlgebraError> {
        let info = self.calls.create_svd_info(self.handle)?;
        Ok(self.own(NativeObject::SvdInfo, info))
    }

    /// The contraction network of `inputs` into `output_modes` on the
    /// device's handle, owned.
    fn network(
        &self,
        inputs: [TensorModes<'_>; 2],
        output_modes: &[i32],
    ) -> Result<NativeContractionNetwork, CudaAlgebraError> {
        let network = self
            .calls
            .create_contraction_network(self.handle, inputs, output_modes)?;
        self.own(NativeObject::Network, network.descriptor);
        Ok(network)
    }

    /// An optimizer info of `network` on the device's handle, owned.
    fn optimizer_info(&self, network: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError> {
        let info = self.calls.create_optimizer_info(self.handle, network)?;
        Ok(self.own(NativeObject::OptimizerInfo, info))
    }

    /// A device buffer holding `values`, once the copies are synchronized.
    fn buffer(&self, values: &[Complex64]) -> OpaqueHandle {
        let buffer = self
            .allocate(values.len() * size_of::<Complex64Abi>())
            .expect("the device has memory");
        self.load(buffer, values);
        buffer
    }

    /// Copies `values` to the start of `buffer`.
    fn load(&self, buffer: OpaqueHandle, values: &[Complex64]) {
        let values: Vec<Complex64Abi> = values.iter().map(|&value| value.into()).collect();
        self.calls
            .copy_to_device(buffer, &values)
            .expect("the buffer holds the values");
    }

    /// A descriptor of `extents` and `modes`, and a buffer holding `values`.
    fn operand(&self, extents: &[i64], modes: &[i32], values: &[Complex64]) -> TensorOperand {
        TensorOperand {
            descriptor: self
                .descriptor(TensorModes { extents, modes })
                .expect("the extents and modes match"),
            data: self.buffer(values),
        }
    }

    /// Lends `slot` the bytes the workspace asks for, if it asks for any:
    /// a slot that asks for nothing is lent nothing.
    fn lend(&self, slot: WorkspaceSlot) {
        let asked = self.asked(slot);
        if asked > 0 {
            self.lend_bytes(slot, asked);
        }
    }

    /// Lends every slot but `unlent` what it asks for.
    fn lend_all_but(&self, unlent: WorkspaceSlot) {
        for slot in WORKSPACE_SLOTS {
            if slot != unlent {
                self.lend(slot);
            }
        }
    }

    /// The bytes the workspace asks for in `slot`.
    fn asked(&self, slot: WorkspaceSlot) -> i64 {
        self.calls
            .workspace_size(self.handle, self.workspace, slot)
            .expect("the workspace is live")
    }

    /// Lends `slot` `bytes` of memory.
    fn lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) {
        self.try_lend_bytes(slot, bytes)
            .expect("the buffer holds the bytes lent");
    }

    /// Lends `slot` `bytes` of memory where the slot's memory lives: a
    /// device allocation for a device slot, host memory for a host slot.
    fn try_lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) -> Result<(), CudaAlgebraError> {
        let size = usize::try_from(bytes).expect("a size is not negative");
        let memory = match slot {
            WorkspaceSlot::DeviceScratch | WorkspaceSlot::DeviceCache => {
                self.allocate(size).expect("the device has memory")
            }
            WorkspaceSlot::HostScratch | WorkspaceSlot::HostCache => self.host(size),
        };
        self.calls
            .set_workspace(self.handle, self.workspace, slot, memory, bytes)
    }

    /// `bytes` of host memory, kept as long as the device.
    fn host(&self, bytes: usize) -> OpaqueHandle {
        let mut memory = vec![0_u8; bytes.max(1)];
        let start = NonNull::from(memory.as_mut_slice()).cast();
        self.host_memory.borrow_mut().push(memory);
        start
    }

    /// A buffer of zeros for a tensor of `extents`, once the copies are
    /// synchronized.
    fn zeros(&self, extents: &[i64]) -> OpaqueHandle {
        let elements = extents.iter().product::<i64>();
        self.buffer(&vec![
            Complex64::default();
            usize::try_from(elements).unwrap_or(0)
        ])
    }

    /// The first `elements` values of `buffer`, once the stream is done.
    fn read(&self, buffer: OpaqueHandle, elements: usize) -> Vec<Complex64> {
        self.calls
            .synchronize_stream(self.stream)
            .expect("the stream is live");
        let mut values = vec![Complex64Abi::default(); elements];
        self.calls
            .copy_to_host(&mut values, buffer)
            .expect("the buffer holds the values");
        values.into_iter().map(Complex64::from).collect()
    }
}

/// A contraction of two tensors: its device, network and buffers.
struct Contraction<C: TestedCalls = FakeLibraries> {
    device: Device<C>,
    network: NativeContractionNetwork,
    info: OpaqueHandle,
    inputs: [OpaqueHandle; 2],
    output: OpaqueHandle,
}

impl<C: TestedCalls> Contraction<C> {
    /// The network of `inputs`, holding `data`, and `output`, on `device`,
    /// optimized, in a workspace sized for its path, lent nothing yet; not
    /// prepared.
    fn sized(
        device: Device<C>,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let calls = &device.calls;
        let network = device
            .network(inputs, output.modes)
            .expect("the labels are consistent");
        let info = device
            .optimizer_info(network.descriptor)
            .expect("the network is live");
        calls
            .optimize_contraction(device.handle, network.descriptor, 1 << 20, info)
            .expect("the info is the network's");
        calls
            .contraction_workspace_sizes(device.handle, network.descriptor, info, device.workspace)
            .expect("the path is optimized");
        let inputs = [device.buffer(data[0]), device.buffer(data[1])];
        let output = device.zeros(output.extents);
        calls.synchronize_copies().expect("the copies were queued");
        Self {
            device,
            network,
            info,
            inputs,
            output,
        }
    }

    /// The dot product of two vectors of ones, of 2 elements, as
    /// [`sized`](Self::sized).
    fn dot_product_sized(device: Device<C>) -> Self {
        let ones = [Complex64::new(1.0, 0.0); 2];
        Self::sized(
            device,
            DOT_PRODUCT_INPUTS,
            [&ones, &ones],
            DOT_PRODUCT_OUTPUT,
        )
    }

    /// As [`dot_product_sized`](Self::dot_product_sized), its device
    /// scratch lent, and prepared.
    fn dot_product(device: Device<C>) -> Self {
        let contraction = Self::dot_product_sized(device);
        contraction.device.lend(WorkspaceSlot::DeviceScratch);
        contraction
            .prepare()
            .expect("the workspace is sized for the path and lent");
        contraction
    }

    fn prepare(&self) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device
            .calls
            .prepare_contraction(device.handle, self.network.descriptor, device.workspace)
    }

    fn contract(&self) -> Result<(), CudaAlgebraError> {
        self.contract_on(self.device.stream)
    }

    fn contract_on(&self, stream: Stream) -> Result<(), CudaAlgebraError> {
        self.contract_buffers(self.inputs, self.output, stream)
    }

    fn contract_buffers(
        &self,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.contract(
            device.handle,
            self.network,
            inputs,
            output,
            device.workspace,
            stream,
        )
    }
}

/// The inputs of a dot product of two vectors of 2 elements.
const DOT_PRODUCT_INPUTS: [TensorModes<'static>; 2] = [
    TensorModes {
        extents: &[2],
        modes: &[1],
    },
    TensorModes {
        extents: &[2],
        modes: &[1],
    },
];

/// The output of a dot product: a scalar.
const DOT_PRODUCT_OUTPUT: TensorModes<'static> = TensorModes {
    extents: &[],
    modes: &[],
};

/// Two dot-product networks on one handle, each with its optimizer info;
/// `optimized` says whether each info's path is searched.
fn two_networks<C: TestedCalls>(
    device: &Device<C>,
    optimized: bool,
) -> [(OpaqueHandle, OpaqueHandle); 2] {
    let (calls, handle) = (&device.calls, device.handle);
    [(); 2].map(|()| {
        let network = device
            .network(DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
            .expect("the labels are consistent")
            .descriptor;
        let info = device.optimizer_info(network).expect("the network is live");
        if optimized {
            calls
                .optimize_contraction(handle, network, 1 << 20, info)
                .expect("the info is the network's");
        }
        (network, info)
    })
}

// The labels of a gate split's modes: A[p, b], B[b, q] and the gate
// G[p', q', p, q] in, U[p', s] and V[s, q'] out.
const P: i32 = 1;
const BOND: i32 = 2;
const Q: i32 = 3;
const P_OUT: i32 = 4;
const Q_OUT: i32 = 5;
const S: i32 = 6;
/// An outer bond of A or B, kept in U or V.
const OUTER: i32 = 7;
/// The gate of a two-site split, G[p', q', p, q].
const GATE: [i32; 4] = [P_OUT, Q_OUT, P, Q];
/// The settings `CudaAlgebra` gives a gate split's SVD config, the only
/// ones the fake splits under: LAPACK's SVD, the kept values normalized
/// and absorbed into U, under a cutoff that drops only values of
/// rounding's size.
const ALGEBRA_SETTINGS: [SvdSetting; 4] = [
    SvdSetting::AlgorithmGesvd,
    SvdSetting::NormalizationL2,
    SvdSetting::PartitionUs,
    SvdSetting::DiscardedWeightCutoff(1e-12),
];

/// A two-site gate applied to two qubits in |0⟩: A and B hold |0⟩ at bond
/// extent 1, so the product θ[p', q'] is the gate's first column
/// G[p', q', 0, 0], a 2×2 matrix, which U and V factor at a shared extent
/// of 2.
struct GateSplit<C: TestedCalls = FakeLibraries> {
    device: Device<C>,
    config: OpaqueHandle,
    info: OpaqueHandle,
    operands: GateSplitOperands,
}

impl GateSplit {
    /// [`on`](Self::on) the fake libraries.
    fn of(theta: [Complex64; 4]) -> Self {
        Self::on(fake(), theta)
    }
}

impl<C: TestedCalls> GateSplit<C> {
    /// A split on `device` of `theta`, column-major, under the algebra's
    /// settings ([`ALGEBRA_SETTINGS`]).
    fn on(device: Device<C>, theta: [Complex64; 4]) -> Self {
        let (one, zero) = (Complex64::new(1.0, 0.0), Complex64::default());
        let config = device.svd_config().expect("the handle is live");
        for setting in ALGEBRA_SETTINGS {
            device
                .calls
                .configure_svd(device.handle, config, setting)
                .expect("the setting is in range");
        }
        let info = device.svd_info().expect("the handle is live");
        let operands = GateSplitOperands {
            a: device.operand(&[2, 1], &[P, BOND], &[one, zero]),
            b: device.operand(&[1, 2], &[BOND, Q], &[one, zero]),
            gate: device.operand(&[2, 2, 2, 2], &GATE, &values(16, 0.25)),
            u: device.operand(&[2, 2], &[P_OUT, S], &[zero; 4]),
            v: device.operand(&[2, 2], &[S, Q_OUT], &[zero; 4]),
            singular_values: device.buffer(&[zero; 2]),
        };
        let split = Self {
            device,
            config,
            info,
            operands,
        };
        split.device.load(split.operands.gate.data, &theta);
        split
            .device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        split
    }

    /// Sizes the workspace for a split under `config`, and lends its device
    /// scratch what it asks for.
    fn size(&self, config: OpaqueHandle) {
        self.size_workspace(config);
        self.device.lend(WorkspaceSlot::DeviceScratch);
    }

    /// Sizes the workspace for a split under `config`, and lends it
    /// nothing.
    fn size_workspace(&self, config: OpaqueHandle) {
        self.size_under(config)
            .expect("the descriptors and config are live");
    }

    /// Sizes the workspace for a split under `config`.
    fn size_under(&self, config: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        let Self {
            device, operands, ..
        } = self;
        device.calls.gate_split_workspace_sizes(
            device.handle,
            &descriptors(operands),
            config,
            device.workspace,
        )
    }

    /// Queues the split under `config` on `stream`, in the workspace as
    /// last sized.
    fn run(&self, config: OpaqueHandle, stream: Stream) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.gate_split(
            device.handle,
            &self.operands,
            config,
            self.info,
            device.workspace,
            stream,
        )
    }

    /// Sizes the workspace for the split, lends it, runs the split and
    /// waits for it; a refusal at any step is returned.
    fn split(&self) -> Result<(), CudaAlgebraError> {
        self.size_under(self.config)?;
        self.device.lend(WorkspaceSlot::DeviceScratch);
        self.run(self.config, self.device.stream)?;
        self.device
            .calls
            .synchronize_stream(self.device.stream)
            .expect("the stream is live");
        Ok(())
    }
}

/// The descriptors of a gate split's `operands`.
fn descriptors(operands: &GateSplitOperands) -> GateSplitDescriptors {
    GateSplitDescriptors {
        a: operands.a.descriptor,
        b: operands.b.descriptor,
        gate: operands.gate.descriptor,
        u: operands.u.descriptor,
        v: operands.v.descriptor,
    }
}

/// A gate split, under a cutoff, on a device `new` makes, of A, B and the
/// gate carrying the labels `a`, `b` and `gate`, into U and V carrying `u`
/// and `v`; every mode has extent 2 but the bond between A and B, of
/// extent 1.
fn split_with_labels<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
    a: &[i32],
    b: &[i32],
    gate: &[i32],
    u: &[i32],
    v: &[i32],
) -> Result<(), CudaAlgebraError> {
    let mut split = GateSplit::on(new(), matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    let extents = |modes: &[i32]| -> Vec<i64> {
        modes
            .iter()
            .map(|&mode| if mode == BOND { 1 } else { 2 })
            .collect()
    };
    let elements = |modes: &[i32]| 1 << (modes.len() - usize::from(modes.contains(&BOND)));
    split.operands.a = device.operand(&extents(a), a, &values(elements(a), 0.5));
    split.operands.b = device.operand(&extents(b), b, &values(elements(b), -0.25));
    split.operands.gate = device.operand(&extents(gate), gate, &values(elements(gate), 0.25));
    let zeros = |modes: &[i32]| vec![Complex64::default(); elements(modes)];
    split.operands.u = device.operand(&extents(u), u, &zeros(u));
    split.operands.v = device.operand(&extents(v), v, &zeros(v));
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    split.split()
}

/// Every slot of a workspace.
const WORKSPACE_SLOTS: [WorkspaceSlot; 4] = [
    WorkspaceSlot::DeviceScratch,
    WorkspaceSlot::DeviceCache,
    WorkspaceSlot::HostScratch,
    WorkspaceSlot::HostCache,
];

// The labels of the QR's input modes i, k and j.
const QR_I: i32 = 1;
const QR_K: i32 = 2;
const QR_J: i32 = 3;

/// A QR of T[i, k, j] (2×2×3) into Q[k, s, i] and R[j, s], whose
/// descriptors list the modes in another order than the input's: the
/// matrix's rows are (i, k), its columns j, and Q and R share an extent.
struct TensorQr<C: TestedCalls = FakeLibraries> {
    device: Device<C>,
    input: TensorOperand,
    q: TensorOperand,
    r: TensorOperand,
}

impl<C: TestedCalls> TensorQr<C> {
    /// The QR on `device` at a shared extent of `extent`, in a workspace
    /// sized for it, lent nothing yet.
    fn sized(device: Device<C>, extent: i64) -> Self {
        Self::sized_with_input(device, extent, &[2, 2, 3], &[QR_I, QR_K, QR_J])
    }

    /// As [`sized`](Self::sized), with an input of `input_extents` and
    /// `input_modes` instead of T[i, k, j].
    fn sized_with_input(
        device: Device<C>,
        extent: i64,
        input_extents: &[i64],
        input_modes: &[i32],
    ) -> Self {
        let qr = Self::described(device, extent, input_extents, input_modes);
        qr.size().expect("the descriptors are live");
        qr
    }

    /// As [`sized_with_input`](Self::sized_with_input), the workspace not
    /// sized yet.
    fn described(
        device: Device<C>,
        extent: i64,
        input_extents: &[i64],
        input_modes: &[i32],
    ) -> Self {
        let [mode_i, mode_k, mode_j, mode_s] = [QR_I, QR_K, QR_J, 4];
        let shared = usize::try_from(extent).expect("an extent is positive");
        let zero = Complex64::default();
        let elements = input_extents
            .iter()
            .map(|&extent| usize::try_from(extent).expect("an extent is positive"))
            .product();
        let input = device.operand(input_extents, input_modes, &values(elements, 0.5));
        let q = device.operand(
            &[2, extent, 2],
            &[mode_k, mode_s, mode_i],
            &vec![zero; 4 * shared],
        );
        let r = device.operand(&[3, extent], &[mode_j, mode_s], &vec![zero; 3 * shared]);
        device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        Self {
            device,
            input,
            q,
            r,
        }
    }

    /// Sizes the workspace for the QR.
    fn size(&self) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.qr_workspace_sizes(
            device.handle,
            self.input.descriptor,
            self.q.descriptor,
            self.r.descriptor,
            device.workspace,
        )
    }

    /// Queues the QR on `stream`.
    fn run_on(&self, stream: Stream) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.tensor_qr(
            device.handle,
            self.input,
            self.q,
            self.r,
            device.workspace,
            stream,
        )
    }
}

/// Runs the QR of [`TensorQr`] on `device` at a shared extent of `extent`:
/// sizes the workspace, lends its device scratch and queues the QR on the
/// stream `stream_of` gives; a refusal at any step is returned.
fn qr_on<C: TestedCalls>(
    device: Device<C>,
    extent: i64,
    stream_of: impl FnOnce(&Device<C>) -> Stream,
) -> Result<(), CudaAlgebraError> {
    let qr = TensorQr::described(device, extent, &[2, 2, 3], &[QR_I, QR_K, QR_J]);
    qr.size()?;
    qr.device.lend(WorkspaceSlot::DeviceScratch);
    qr.run_on(stream_of(&qr.device))
}

/// Makes every workspace of `fake` ask for `bytes` in each slot but the
/// device scratch.
fn ask_in_every_other_slot(fake: &FakeLibraries, bytes: i64) {
    for slot in WORKSPACE_SLOTS {
        if !matches!(slot, WorkspaceSlot::DeviceScratch) {
            fake.set_workspace_slot_bytes(slot, bytes);
        }
    }
}

/// Lends every slot of `device`'s workspace but the host cache, which asks
/// for memory too, and checks that `run` is refused until it is lent.
fn needs_every_slot(device: &Device, run: impl Fn() -> Result<(), CudaAlgebraError>) {
    device.lend_all_but(WorkspaceSlot::HostCache);
    run().expect_err("the host cache asks for memory and has none");
    device.lend(WorkspaceSlot::HostCache);
    run().expect("every slot is lent");
}

/// Asserts that `actual` equals `expected` to rounding.
fn assert_close(actual: &[Complex64], expected: &[Complex64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (actual_value, expected_value) in actual.iter().zip(expected) {
        assert!(
            (actual_value - expected_value).norm() < 1e-12,
            "{actual:?} differs from {expected:?}"
        );
    }
}

/// A real 2×2 matrix, column-major, from its rows.
fn matrix(rows: [[f64; 2]; 2]) -> [Complex64; 4] {
    [rows[0][0], rows[1][0], rows[0][1], rows[1][1]].map(|value| Complex64::new(value, 0.0))
}

/// Distinct values for `elements` elements, so a misplaced one shows.
fn values(elements: usize, seed: f64) -> Vec<Complex64> {
    (0..u32::try_from(elements).expect("a test tensor is small"))
        .map(|at| {
            let at = f64::from(at);
            Complex64::new(seed + at, 1.0 - at / 2.0)
        })
        .collect()
}
