// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the fake libraries' tensor API, which the CUDA algebra calls
//! through the native calls (`src/cuda_algebra/`): the dense maths its
//! tests are checked against (a contraction, a gate split, a QR), and the
//! rules the fake holds a caller to (tensor descriptors, workspaces,
//! streams, objects' handles and lifetimes, a gate split's labels, copies
//! of its results to the host), each refused on the fake as a misuse would
//! fail on a GPU. They call the API directly, because only a direct caller
//! can make the misuse the rules refuse; the rules on streams are shared
//! with the state API, so a compute's are tested here too.

use crate::native_calls::{
    OpaqueHandle, Stream,
    complex::Complex64Abi,
    fake_libraries::FakeLibraries,
    memory_calls::{MemoryCalls, WorkspaceSlot},
    session_calls::SessionCalls,
    state_calls::{MpsLayout, NativeCapacity, StateCalls},
    tensor_calls::{
        ContractionNetwork, GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation,
        TensorCalls, TensorModes, TensorOperand,
    },
};
use num_complex::Complex64;
use std::{cell::RefCell, ptr::NonNull};
use tensornet::MpsShape;

/// What every tensor call on the fake needs: a handle, a stream and a
/// workspace.
struct Device {
    fake: FakeLibraries,
    handle: OpaqueHandle,
    stream: Stream,
    workspace: OpaqueHandle,
    /// The host memory lent to host slots, kept as long as the device.
    host_memory: RefCell<Vec<Vec<u8>>>,
}

impl Device {
    fn new(fake: FakeLibraries) -> Self {
        let handle = fake.create_handle().expect("the fake has a device");
        let stream = fake.create_stream().expect("the fake has a device");
        let workspace = fake.create_workspace(handle).expect("the handle is live");
        Self {
            fake,
            handle,
            stream,
            workspace,
            host_memory: RefCell::default(),
        }
    }

    /// A device buffer holding `values`, once the copies are synchronized.
    fn buffer(&self, values: &[Complex64]) -> OpaqueHandle {
        let buffer = self
            .fake
            .allocate(values.len() * size_of::<Complex64Abi>())
            .expect("the fake has memory");
        self.load(buffer, values);
        buffer
    }

    /// Copies `values` to the start of `buffer`.
    fn load(&self, buffer: OpaqueHandle, values: &[Complex64]) {
        let values: Vec<Complex64Abi> = values.iter().map(|&value| value.into()).collect();
        self.fake
            .copy_to_device(buffer, &values)
            .expect("the buffer holds the values");
    }

    /// A descriptor of `extents` and `modes`, and a buffer holding `values`.
    fn operand(&self, extents: &[i64], modes: &[i32], values: &[Complex64]) -> TensorOperand {
        TensorOperand {
            descriptor: self
                .fake
                .create_tensor_descriptor(self.handle, TensorModes { extents, modes })
                .expect("the extents and modes match"),
            data: self.buffer(values),
        }
    }

    /// Lends `slot` the bytes the workspace asks for.
    fn lend(&self, slot: WorkspaceSlot) {
        self.lend_bytes(slot, self.asked(slot));
    }

    /// The bytes the workspace asks for in `slot`.
    fn asked(&self, slot: WorkspaceSlot) -> i64 {
        self.fake
            .workspace_size(self.handle, self.workspace, slot)
            .expect("the workspace is live")
    }

    /// Lends `slot` `bytes` of memory where the slot's memory lives: a
    /// device allocation for a device slot, host memory for a host slot.
    fn lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) {
        let size = usize::try_from(bytes).expect("a size is not negative");
        let memory = match slot {
            WorkspaceSlot::DeviceScratch | WorkspaceSlot::DeviceCache => {
                self.fake.allocate(size).expect("the fake has memory")
            }
            WorkspaceSlot::HostScratch | WorkspaceSlot::HostCache => self.host(size),
        };
        self.fake
            .set_workspace(self.handle, self.workspace, slot, memory, bytes)
            .expect("the buffer holds the bytes lent");
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
        self.fake
            .synchronize_stream(self.stream)
            .expect("the stream is live");
        let mut values = vec![Complex64Abi::default(); elements];
        self.fake
            .copy_to_host(&mut values, buffer)
            .expect("the buffer holds the values");
        values.into_iter().map(Complex64::from).collect()
    }
}

/// A contraction of two tensors through the fake: its device, network and
/// buffers.
struct Contraction {
    device: Device,
    network: ContractionNetwork,
    info: OpaqueHandle,
    inputs: [OpaqueHandle; 2],
    output: OpaqueHandle,
}

impl Contraction {
    /// The network of `inputs`, holding `data`, and `output`, optimized,
    /// in a workspace sized for its path, lent nothing yet; not prepared.
    fn sized(
        fake: FakeLibraries,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let device = Device::new(fake);
        let (fake, handle) = (&device.fake, device.handle);
        let network = fake
            .create_contraction_network(handle, inputs, output.modes)
            .expect("the labels are consistent");
        let info = fake
            .create_optimizer_info(handle, network.descriptor)
            .expect("the network is live");
        fake.optimize_contraction(handle, network.descriptor, 1 << 20, info)
            .expect("the info is the network's");
        fake.contraction_workspace_sizes(handle, network.descriptor, info, device.workspace)
            .expect("the path is optimized");
        let inputs = [device.buffer(data[0]), device.buffer(data[1])];
        let output = device.zeros(output.extents);
        fake.synchronize_copies().expect("the copies were queued");
        Self {
            device,
            network,
            info,
            inputs,
            output,
        }
    }

    /// As [`sized`](Self::sized), its device scratch lent, and prepared.
    fn prepared(
        fake: FakeLibraries,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let contraction = Self::sized(fake, inputs, data, output);
        contraction.device.lend(WorkspaceSlot::DeviceScratch);
        contraction
            .prepare()
            .expect("the workspace is sized for the path and lent");
        contraction
    }

    /// Prepares the dot product of two vectors of ones, of 2 elements.
    fn dot_product(fake: FakeLibraries) -> Self {
        let ones = [Complex64::new(1.0, 0.0); 2];
        Self::prepared(fake, DOT_PRODUCT_INPUTS, [&ones, &ones], DOT_PRODUCT_OUTPUT)
    }

    fn prepare(&self) -> Result<(), crate::NativeError> {
        let device = &self.device;
        device
            .fake
            .prepare_contraction(device.handle, self.network.descriptor, device.workspace)
    }

    fn contract(&self) -> Result<(), crate::NativeError> {
        self.contract_on(self.device.stream)
    }

    fn contract_on(&self, stream: Stream) -> Result<(), crate::NativeError> {
        self.contract_buffers(self.inputs, self.output, stream)
    }

    fn contract_buffers(
        &self,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), crate::NativeError> {
        let device = &self.device;
        device.fake.contract(
            device.handle,
            self.network,
            inputs,
            output,
            device.workspace,
            stream,
        )
    }

    /// The output's first `elements` values, once the stream is done.
    fn output(&self, elements: usize) -> Vec<Complex64> {
        self.device.read(self.output, elements)
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
/// A cutoff that drops only values of rounding's size: the split then
/// truncates by value, as the canonical MPS always asks.
const ROUNDING_CUTOFF: SvdSetting = SvdSetting::DiscardedWeightCutoff(1e-12);

/// A two-site gate applied to two qubits in |0⟩ through the fake: A and B
/// hold |0⟩ at bond extent 1, so the product θ[p', q'] is the gate's first
/// column G[p', q', 0, 0], a 2×2 matrix, which U and V factor at a shared
/// extent of 2.
struct GateSplit {
    device: Device,
    config: OpaqueHandle,
    info: OpaqueHandle,
    operands: GateSplitOperands,
}

impl GateSplit {
    /// A split of `theta`, column-major, under the `settings` given in
    /// order to a new SVD config.
    fn of(theta: [Complex64; 4], settings: &[SvdSetting]) -> Self {
        let device = Device::new(FakeLibraries::new());
        let (one, zero) = (Complex64::new(1.0, 0.0), Complex64::default());
        let config = device
            .fake
            .create_svd_config(device.handle)
            .expect("the handle is live");
        for &setting in settings {
            device
                .fake
                .configure_svd(device.handle, config, setting)
                .expect("the setting is in range");
        }
        let info = device
            .fake
            .create_svd_info(device.handle)
            .expect("the handle is live");
        let operands = GateSplitOperands {
            a: device.operand(&[2, 1], &[P, BOND], &[one, zero]),
            b: device.operand(&[1, 2], &[BOND, Q], &[one, zero]),
            gate: device.operand(&[2, 2, 2, 2], &[P_OUT, Q_OUT, P, Q], &values(16, 0.25)),
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
        split.load(theta);
        split
    }

    /// Sets the gate's first column, θ, to `theta`.
    fn load(&self, theta: [Complex64; 4]) {
        self.device.load(self.operands.gate.data, &theta);
        self.device
            .fake
            .synchronize_copies()
            .expect("the copies were queued");
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
    fn size_under(&self, config: OpaqueHandle) -> Result<(), crate::NativeError> {
        let Self {
            device, operands, ..
        } = self;
        let descriptors = GateSplitDescriptors {
            a: operands.a.descriptor,
            b: operands.b.descriptor,
            gate: operands.gate.descriptor,
            u: operands.u.descriptor,
            v: operands.v.descriptor,
        };
        device.fake.gate_split_workspace_sizes(
            device.handle,
            &descriptors,
            config,
            device.workspace,
        )
    }

    /// Queues the split under `config` on `stream`, in the workspace as
    /// last sized.
    fn run(&self, config: OpaqueHandle, stream: Stream) -> Result<(), crate::NativeError> {
        let device = &self.device;
        device.fake.gate_split(
            device.handle,
            &self.operands,
            config,
            self.info,
            device.workspace,
            stream,
        )
    }

    /// Sizes the workspace for the split, lends it, runs the split and
    /// reads its factors and report.
    fn split(&self) -> Result<Factors, crate::NativeError> {
        let Self {
            device,
            config,
            info,
            operands,
        } = self;
        self.size(*config);
        self.run(*config, device.stream)?;
        device
            .fake
            .synchronize_stream(device.stream)
            .expect("the stream is live");
        let truncation = device
            .fake
            .svd_truncation(device.handle, *info)
            .expect("the stream is synchronized");
        let extent = usize::try_from(truncation.reduced_extent).expect("an extent is positive");
        Ok(Factors {
            u: device.read(operands.u.data, 2 * extent),
            v: device.read(operands.v.data, 2 * extent),
            extent,
            truncation,
        })
    }
}

/// A gate split's U (2×extent) and V (extent×2), column-major, and its
/// report.
#[derive(Debug)]
struct Factors {
    u: Vec<Complex64>,
    v: Vec<Complex64>,
    extent: usize,
    truncation: SvdTruncation,
}

impl Factors {
    /// U·V, column-major.
    fn product(&self) -> Vec<Complex64> {
        (0..4)
            .map(|at| {
                let (row, col) = (at % 2, at / 2);
                (0..self.extent)
                    .map(|s| self.u[row + 2 * s] * self.v[s + self.extent * col])
                    .sum()
            })
            .collect()
    }

    /// U†U, column-major.
    fn u_gram(&self) -> Vec<Complex64> {
        let n = self.extent;
        (0..n * n)
            .map(|at| {
                let (s, t) = (at % n, at / n);
                (0..2)
                    .map(|row| self.u[row + 2 * s].conj() * self.u[row + 2 * t])
                    .sum()
            })
            .collect()
    }

    /// V·V†, column-major.
    fn v_gram(&self) -> Vec<Complex64> {
        let n = self.extent;
        (0..n * n)
            .map(|at| {
                let (s, t) = (at % n, at / n);
                (0..2)
                    .map(|col| self.v[s + n * col] * self.v[t + n * col].conj())
                    .sum()
            })
            .collect()
    }
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

/// A QR through the fake of T[i, k, j] (2×2×3) into Q[k, s, i] and R[j, s],
/// whose descriptors list the modes in another order than the input's: the
/// matrix's rows are (i, k), its columns j, and Q and R share an extent.
struct TensorQr {
    device: Device,
    input: TensorOperand,
    q: TensorOperand,
    r: TensorOperand,
    input_values: Vec<Complex64>,
    shared: usize,
}

impl TensorQr {
    /// The QR on `fake` at a shared extent of `extent`, in a workspace
    /// sized for it, lent nothing yet.
    fn sized(fake: FakeLibraries, extent: i64) -> Self {
        Self::sized_with_input(fake, extent, &[2, 2, 3], &[QR_I, QR_K, QR_J])
    }

    /// As [`sized`](Self::sized), with an input of `input_extents` and
    /// `input_modes` instead of T[i, k, j].
    fn sized_with_input(
        fake: FakeLibraries,
        extent: i64,
        input_extents: &[i64],
        input_modes: &[i32],
    ) -> Self {
        let [mode_i, mode_k, mode_j, mode_s] = [QR_I, QR_K, QR_J, 4];
        let device = Device::new(fake);
        let shared = usize::try_from(extent).expect("an extent is positive");
        let zero = Complex64::default();
        let elements = input_extents
            .iter()
            .map(|&extent| usize::try_from(extent).expect("an extent is positive"))
            .product();
        let input_values = values(elements, 0.5);
        let input = device.operand(input_extents, input_modes, &input_values);
        let q = device.operand(
            &[2, extent, 2],
            &[mode_k, mode_s, mode_i],
            &vec![zero; 4 * shared],
        );
        let r = device.operand(&[3, extent], &[mode_j, mode_s], &vec![zero; 3 * shared]);
        device
            .fake
            .synchronize_copies()
            .expect("the copies were queued");
        let qr = Self {
            device,
            input,
            q,
            r,
            input_values,
            shared,
        };
        qr.size();
        qr
    }

    /// Sizes the workspace for the QR.
    fn size(&self) {
        let device = &self.device;
        device
            .fake
            .qr_workspace_sizes(
                device.handle,
                self.input.descriptor,
                self.q.descriptor,
                self.r.descriptor,
                device.workspace,
            )
            .expect("the descriptors are live");
    }

    /// Queues the QR on `stream`.
    fn run_on(&self, stream: Stream) -> Result<(), crate::NativeError> {
        let device = &self.device;
        device.fake.tensor_qr(
            device.handle,
            self.input,
            self.q,
            self.r,
            device.workspace,
            stream,
        )
    }
}

/// The QR of [`TensorQr`] at a shared extent of `extent`, its device
/// scratch lent. Returns T, Q and R, each in its own mode order, first mode
/// fastest.
fn qr(extent: i64) -> Result<[Vec<Complex64>; 3], crate::NativeError> {
    qr_on(extent, |device| device.stream)
}

/// [`qr`], on the stream `stream_of` gives for its device.
fn qr_on(
    extent: i64,
    stream_of: impl FnOnce(&Device) -> Stream,
) -> Result<[Vec<Complex64>; 3], crate::NativeError> {
    let qr = TensorQr::sized(FakeLibraries::new(), extent);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    qr.run_on(stream_of(device))?;
    Ok([
        qr.input_values.clone(),
        device.read(qr.q.data, 4 * qr.shared),
        device.read(qr.r.data, 3 * qr.shared),
    ])
}

/// Lends every slot of `device`'s workspace but the host cache, which asks
/// for memory too, and checks that `run` is refused until it is lent.
fn needs_every_slot(device: &Device, run: impl Fn() -> Result<(), crate::NativeError>) {
    for slot in [
        WorkspaceSlot::DeviceScratch,
        WorkspaceSlot::DeviceCache,
        WorkspaceSlot::HostScratch,
    ] {
        device.lend(slot);
    }
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

/// The `n`×`n` identity, column-major.
fn identity(n: usize) -> Vec<Complex64> {
    (0..n * n)
        .map(|at| Complex64::new(if at % n == at / n { 1.0 } else { 0.0 }, 0.0))
        .collect()
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

/// A caller's test of `C[j, i] = Σ_{k, l} A[i, k, l] · B[k, j, l]` is
/// checked against the fake: it sums every label absent from the output,
/// and lays the output out in its own mode order, first mode fastest.
#[test]
fn the_fake_contracts_as_the_labels_say() {
    let (i, k, l, j) = (1, 2, 3, 4);
    let left = values(2 * 3 * 2, 0.5);
    let right = values(3 * 2 * 2, -1.0);
    let contraction = Contraction::prepared(
        FakeLibraries::new(),
        [
            TensorModes {
                extents: &[2, 3, 2],
                modes: &[i, k, l],
            },
            TensorModes {
                extents: &[3, 2, 2],
                modes: &[k, j, l],
            },
        ],
        [&left, &right],
        TensorModes {
            extents: &[2, 2],
            modes: &[j, i],
        },
    );
    contraction.contract().expect("the workspace is lent");
    let mut expected = vec![Complex64::default(); 4];
    for (ji, element) in expected.iter_mut().enumerate() {
        let (jj, ii) = (ji % 2, ji / 2);
        for kk in 0..3 {
            for ll in 0..2 {
                *element += left[ii + 2 * kk + 6 * ll] * right[kk + 3 * jj + 6 * ll];
            }
        }
    }
    assert_eq!(contraction.output(4), expected);
}

/// cuTensorNet may ask for memory in any slot of a workspace (a device
/// cache, host scratch), and runs a call only once each is lent; a caller
/// that lends device scratch alone is caught on the fake, whether it
/// contracts, splits a gate or runs a QR, and so is one that leaves any
/// single slot unlent.
#[test]
fn a_tensor_call_needs_memory_in_every_slot_that_asks_for_it() {
    let fake = FakeLibraries::new();
    fake.set_other_workspace_slot_bytes(64);
    let contraction = Contraction::dot_product(fake);
    needs_every_slot(&contraction.device, || contraction.contract());
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    split.device.fake.set_other_workspace_slot_bytes(64);
    split.size_workspace(split.config);
    needs_every_slot(&split.device, || {
        split.run(split.config, split.device.stream)
    });
    for unlent in WORKSPACE_SLOTS {
        let fake = FakeLibraries::new();
        fake.set_other_workspace_slot_bytes(64);
        let qr = TensorQr::sized(fake, 3);
        for slot in WORKSPACE_SLOTS.into_iter().filter(|&slot| slot != unlent) {
            qr.device.lend(slot);
        }
        let refused = qr
            .run_on(qr.device.stream)
            .expect_err("one slot is lent nothing");
        assert!(
            refused_for(&refused, "no memory lent"),
            "{unlent:?}: {refused}"
        );
        qr.device.lend(unlent);
        qr.run_on(qr.device.stream).expect("every slot is lent");
    }
}

/// A caller that lends a slot less than the size query asked for (an old
/// size, after the workspace was sized for a larger call) is caught on the
/// fake, whichever slot it is.
#[test]
fn a_tensor_call_needs_at_least_what_each_slot_asks_for() {
    for short in WORKSPACE_SLOTS {
        let fake = FakeLibraries::new();
        fake.set_other_workspace_slot_bytes(64);
        let qr = TensorQr::sized(fake, 3);
        for slot in WORKSPACE_SLOTS {
            let asked = qr.device.asked(slot);
            qr.device
                .lend_bytes(slot, if slot == short { asked - 1 } else { asked });
        }
        let refused = qr
            .run_on(qr.device.stream)
            .expect_err("one slot is one byte short");
        assert!(
            refused_for(&refused, "less memory lent"),
            "{short:?}: {refused}"
        );
        qr.device.lend(short);
        qr.run_on(qr.device.stream)
            .expect("every slot is lent in full");
    }
}

/// A caller's test of a two-site gate is checked against the fake: U·V is
/// the product of A, B and the gate when nothing is truncated.
#[test]
fn a_gate_split_factors_the_product_of_its_inputs_and_gate() {
    let theta = values(4, 0.5);
    let theta = [theta[0], theta[1], theta[2], theta[3]];
    let factors = GateSplit::of(theta, &[SvdSetting::PartitionUs])
        .split()
        .expect("θ is not zero");
    assert_eq!(factors.extent, 2, "θ has full rank");
    assert_close(&factors.product(), &theta);
}

/// A canonical MPS keeps the singular values on the side its centre moves
/// to: the other side is an isometry, as the config names it.
#[test]
fn a_gate_split_absorbs_the_singular_values_into_the_side_its_config_names() {
    let theta = matrix([[2.0, 1.0], [0.5, -1.5]]);
    let into_u = GateSplit::of(theta, &[SvdSetting::PartitionUs])
        .split()
        .expect("θ is not zero");
    assert_close(&into_u.v_gram(), &identity(2));
    let into_v = GateSplit::of(theta, &[SvdSetting::PartitionSv])
        .split()
        .expect("θ is not zero");
    assert_close(&into_v.u_gram(), &identity(2));
}

/// A canonical MPS whose centre moves right absorbs the singular values
/// into V: U·V is still the product, each of V's rows scaled by its own
/// value.
#[test]
fn a_gate_split_absorbed_into_v_still_factors_the_product() {
    let theta = matrix([[2.0, 1.0], [0.5, -1.5]]);
    let factors = GateSplit::of(theta, &[SvdSetting::PartitionSv])
        .split()
        .expect("θ is not zero");
    assert_eq!(factors.extent, 2, "θ has full rank");
    assert_close(&factors.product(), &theta);
}

/// θ = diag(3, 1) weighs 9 and 1: a cutoff of 0.2 drops the value 1, whose
/// weight 0.1 it reports, and a cutoff of 0.05 keeps it.
#[test]
fn a_gate_split_drops_the_smallest_values_within_the_cutoffs_weight() {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let settings = |cutoff| {
        [
            SvdSetting::PartitionUs,
            SvdSetting::DiscardedWeightCutoff(cutoff),
        ]
    };
    let dropped = GateSplit::of(theta, &settings(0.2))
        .split()
        .expect("θ is not zero");
    assert_eq!(dropped.extent, 1);
    assert!((dropped.truncation.discarded_weight - 0.1).abs() < 1e-12);
    assert_close(&dropped.product(), &matrix([[3.0, 0.0], [0.0, 0.0]]));
    let kept = GateSplit::of(theta, &settings(0.05))
        .split()
        .expect("θ is not zero");
    assert_eq!(kept.extent, 2);
    assert!(kept.truncation.discarded_weight.abs() < 1e-12);
}

/// A cutoff drops a value whose weight is exactly the cutoff's: θ =
/// diag(3, 1), whose value 1 weighs exactly 0.1 of the total, splits to a
/// bond of 1 under a cutoff of 0.1.
#[test]
fn a_gate_split_drops_a_value_whose_weight_equals_the_cutoff() {
    let factors = GateSplit::of(
        matrix([[3.0, 0.0], [0.0, 1.0]]),
        &[
            SvdSetting::PartitionUs,
            SvdSetting::DiscardedWeightCutoff(0.1),
        ],
    )
    .split()
    .expect("θ is not zero");
    assert_eq!(factors.extent, 1, "the value 1 weighs the cutoff exactly");
}

/// A cutoff of 1.0 would drop every value's weight: the split keeps the
/// largest, so a caller always gets a bond.
#[test]
fn a_gate_split_keeps_one_value_whatever_the_cutoff() {
    let factors = GateSplit::of(
        matrix([[3.0, 0.0], [0.0, 1.0]]),
        &[
            SvdSetting::PartitionUs,
            SvdSetting::DiscardedWeightCutoff(1.0),
        ],
    )
    .split()
    .expect("θ is not zero");
    assert_eq!(factors.extent, 1);
    assert_close(&factors.product(), &matrix([[3.0, 0.0], [0.0, 0.0]]));
}

/// A caller grows a bond up to the extent the split could have kept, which
/// the fake reports as the smaller side of θ (cuTensorNet may report less
/// under its reduced algorithm, which no GPU run has checked yet). A
/// product state's θ has rank 1, so a cutoff keeps one value, yet the fake
/// reports 2; a site with an outer bond of 2 makes θ 4×2 on the left, or
/// 2×4 on the right, and the fake reports 2 either way.
#[test]
fn a_gate_split_reports_the_smaller_side_as_its_full_extent() {
    let rank_one = GateSplit::of(
        matrix([[1.0, 1.0], [1.0, 1.0]]),
        &[SvdSetting::PartitionUs, ROUNDING_CUTOFF],
    )
    .split()
    .expect("θ is not zero");
    assert_eq!(rank_one.extent, 1, "θ has rank 1");
    assert_eq!(rank_one.truncation.full_extent, 2);
    let (one, zero) = (Complex64::new(1.0, 0.0), Complex64::default());
    let theta = matrix([[2.0, 1.0], [0.5, -1.5]]);
    let mut left = GateSplit::of(theta, &[SvdSetting::PartitionUs]);
    left.operands.a = left
        .device
        .operand(&[2, 2, 1], &[OUTER, P, BOND], &values(4, 0.5));
    left.operands.u = left
        .device
        .operand(&[2, 2, 2], &[OUTER, P_OUT, S], &[zero; 8]);
    left.device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let tall = left.split().expect("θ is not zero");
    assert_eq!(tall.truncation.full_extent, 2, "θ is 4×2");
    let mut right = GateSplit::of(theta, &[SvdSetting::PartitionUs]);
    right.operands.b = right
        .device
        .operand(&[1, 2, 2], &[BOND, Q, OUTER], &[one, zero, zero, one]);
    right.operands.v = right
        .device
        .operand(&[2, 2, 2], &[S, Q_OUT, OUTER], &[zero; 8]);
    right
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let wide = right.split().expect("θ is not zero");
    assert_eq!(wide.truncation.full_extent, 2, "θ is 2×4");
}

/// Under a cutoff, cuTensorNet writes U and V before it knows the reduced
/// extent, so a caller that sizes their buffers by the bond it expects
/// overruns them on a GPU; the fake refuses it, for U and for V, even
/// where the reduced bond (1, for a rank-1 θ) would fit.
#[test]
fn a_gate_split_needs_u_and_v_buffers_of_their_descriptors_full_extents() {
    let zero = Complex64::default();
    let mut split = GateSplit::of(
        matrix([[1.0, 1.0], [1.0, 1.0]]),
        &[SvdSetting::PartitionUs, ROUNDING_CUTOFF],
    );
    let full = split.operands.u.data;
    let reduced = split.device.buffer(&[zero; 2]);
    split
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    split.operands.u.data = reduced;
    split
        .split()
        .expect_err("U's buffer holds the reduced bond only");
    split.operands.u.data = full;
    let full = split.operands.v.data;
    split.operands.v.data = reduced;
    split
        .split()
        .expect_err("V's buffer holds the reduced bond only");
    split.operands.v.data = full;
    split
        .split()
        .expect("U's and V's buffers hold the full extents");
}

/// cuTensorNet may write the singular values to their buffer, as many as
/// the shared mode's extent: a caller that sized it by an earlier, smaller
/// bond is caught on the fake. Two sites with outer bonds of 2 make θ 4×4,
/// split at a shared extent of 4.
#[test]
fn a_gate_split_needs_a_singular_values_buffer_of_the_shared_extent() {
    const OUTER_RIGHT: i32 = 8;
    let zero = Complex64::default();
    let mut split = GateSplit::of(
        matrix([[2.0, 1.0], [0.5, -1.5]]),
        &[SvdSetting::PartitionUs],
    );
    let device = &split.device;
    split.operands.a = device.operand(&[2, 2, 1], &[OUTER, P, BOND], &values(4, 0.5));
    split.operands.b = device.operand(&[1, 2, 2], &[BOND, Q, OUTER_RIGHT], &values(4, -0.25));
    split.operands.u = device.operand(&[2, 2, 4], &[OUTER, P_OUT, S], &[zero; 16]);
    split.operands.v = device.operand(&[4, 2, 2], &[S, Q_OUT, OUTER_RIGHT], &[zero; 16]);
    let bond_two = device.buffer(&[zero; 1]);
    let bond_four = device.buffer(&[zero; 2]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    split.operands.singular_values = bond_two;
    split
        .split()
        .expect_err("the buffer holds 2 doubles of the 4 the split may write");
    split.operands.singular_values = bond_four;
    split.split().expect("the buffer holds 4 doubles");
}

/// cuTensorNet's shared extent "shall not exceed" the smaller side of the
/// split: a caller that sizes U and V at its bond cap without clamping it
/// to θ's sides is caught on the fake. A 2×2 θ splits at an extent of 2,
/// and is refused at 3, under a cutoff that would otherwise reduce it.
#[test]
fn a_gate_split_needs_a_shared_extent_within_the_smaller_side() {
    let theta = matrix([[2.0, 1.0], [0.5, -1.5]]);
    let settings = [SvdSetting::PartitionUs, ROUNDING_CUTOFF];
    GateSplit::of(theta, &settings)
        .split()
        .expect("the shared extent is the smaller side");
    let zero = Complex64::default();
    let mut above = GateSplit::of(theta, &settings);
    let device = &above.device;
    above.operands.u = device.operand(&[2, 3], &[P_OUT, S], &[zero; 6]);
    above.operands.v = device.operand(&[3, 2], &[S, Q_OUT], &[zero; 6]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let error = above
        .split()
        .expect_err("the shared extent of 3 exceeds θ's 2×2");
    assert!(
        error
            .to_string()
            .contains("exceeds the split's smaller side"),
        "{error}"
    );
}

/// cuTensorNet may write the singular values to their buffer: a caller
/// that freed it is caught on the fake.
#[test]
fn a_gate_split_needs_a_live_singular_values_buffer() {
    let mut split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    split
        .device
        .fake
        .free(split.operands.singular_values)
        .expect("the buffer is live");
    split
        .split()
        .expect_err("the singular values' buffer is freed");
    split.operands.singular_values = split.device.buffer(&[Complex64::default(); 2]);
    split
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    split.split().expect("the singular values' buffer is live");
}

/// Under a discarded-weight cutoff, cuTensorNet writes the reduced extent
/// into U's and V's descriptors, so a caller that reuses them for the next
/// gate has that gate's bond capped there, until it resets them.
#[test]
fn a_gate_split_caps_the_bond_at_its_descriptors_extent_which_it_lowers() {
    let split = GateSplit::of(
        matrix([[1.0, 1.0], [1.0, 1.0]]),
        &[SvdSetting::PartitionUs, ROUNDING_CUTOFF],
    );
    split.split().expect("θ is not zero");
    split.load(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let capped = split.split().expect("θ is not zero");
    assert_eq!(capped.extent, 1);
    assert!((capped.truncation.discarded_weight - 0.1).abs() < 1e-12);
}

/// With no cutoff set, cuTensorNet keeps the shared extent it is asked
/// for, completing the singular vectors of a rank-deficient θ, which the
/// fake does not model: it refuses a product state's θ (rank 1) at extent
/// 2, and accepts it at a fixed extent of 1.
#[test]
fn a_gate_split_with_no_cutoff_refuses_fewer_values_than_its_extent() {
    let product_state = matrix([[1.0, 1.0], [1.0, 1.0]]);
    GateSplit::of(product_state, &[SvdSetting::PartitionUs])
        .split()
        .expect_err("θ has rank 1, below the extent of 2");
    let zero = Complex64::default();
    let mut fixed = GateSplit::of(product_state, &[SvdSetting::PartitionUs]);
    fixed.operands.u = fixed.device.operand(&[2, 1], &[P_OUT, S], &[zero; 2]);
    fixed.operands.v = fixed.device.operand(&[1, 2], &[S, Q_OUT], &[zero; 2]);
    fixed
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let factors = fixed.split().expect("θ has rank 1, the extent asked");
    assert_eq!(factors.extent, 1);
}

/// With no cutoff set, cuTensorNet keeps the shared extent it is asked for,
/// below θ's rank too, and reports the weight that extent drops: a
/// simulator that caps its bond at a fixed extent gets the largest singular
/// values' part of θ, and descriptors still at that extent.
#[test]
fn a_gate_split_with_no_cutoff_truncates_at_its_extent() {
    let zero = Complex64::default();
    let mut split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    split.operands.u = split.device.operand(&[2, 1], &[P_OUT, S], &[zero; 2]);
    split.operands.v = split.device.operand(&[1, 2], &[S, Q_OUT], &[zero; 2]);
    split
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let capped = split.split().expect("θ has full rank");
    assert_eq!(capped.extent, 1, "the extent asked");
    assert!(
        (capped.truncation.discarded_weight - 0.1).abs() < 1e-12,
        "{:?}",
        capped.truncation
    );
    assert_close(&capped.product(), &matrix([[3.0, 0.0], [0.0, 0.0]]));
    let next = split.split().expect("θ has full rank");
    assert_eq!(next.extent, 1, "the descriptors keep the extent of 1");
}

/// A cutoff of 0, as a simulator asking for no loss sets it, still asks
/// for a truncation by value: a product state's θ splits to bond 1, and
/// the descriptors are lowered to it, so the next split is capped there.
#[test]
fn a_gate_split_at_a_cutoff_of_zero_truncates_by_value() {
    let split = GateSplit::of(
        matrix([[1.0, 1.0], [1.0, 1.0]]),
        &[
            SvdSetting::PartitionUs,
            SvdSetting::DiscardedWeightCutoff(0.0),
        ],
    );
    let product = split.split().expect("a cutoff is set");
    assert_eq!(product.extent, 1, "θ has rank 1");
    split.load(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let capped = split.split().expect("θ has full rank");
    assert_eq!(capped.extent, 1, "the descriptors were lowered to 1");
}

/// With no cutoff set, cuTensorNet leaves U's and V's descriptors as they
/// are, so a caller reusing them splits the next gate at the same extent.
#[test]
fn a_gate_split_with_no_cutoff_keeps_its_descriptors_extent() {
    let split = GateSplit::of(
        matrix([[2.0, 1.0], [0.5, -1.5]]),
        &[SvdSetting::PartitionUs],
    );
    let first = split.split().expect("θ has full rank");
    assert_eq!(first.extent, 2);
    split.load(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let next = split.split().expect("θ has full rank");
    assert_eq!(next.extent, 2, "the descriptors keep the extent of 2");
}

/// A simulator that keeps its state normalized asks the split to scale the
/// kept values to unit norm: U·V then has unit Frobenius norm.
#[test]
fn a_gate_split_normalizes_the_kept_values_when_asked() {
    let factors = GateSplit::of(
        matrix([[3.0, 0.0], [0.0, 1.0]]),
        &[SvdSetting::PartitionUs, SvdSetting::NormalizationL2],
    )
    .split()
    .expect("θ is not zero");
    let norm = factors
        .product()
        .iter()
        .map(Complex64::norm_sqr)
        .sum::<f64>();
    assert!((norm - 1.0).abs() < 1e-12, "the squared norm is {norm}");
}

/// A normalized MPS meets a truncation at every gate that exceeds its
/// bond: θ = diag(3, 1) under a cutoff of 0.2 keeps 3, scaled to 1, so U·V
/// has unit Frobenius norm.
#[test]
fn a_gate_split_normalizes_the_values_it_keeps_after_a_truncation() {
    let factors = GateSplit::of(
        matrix([[3.0, 0.0], [0.0, 1.0]]),
        &[
            SvdSetting::PartitionUs,
            SvdSetting::NormalizationL2,
            SvdSetting::DiscardedWeightCutoff(0.2),
        ],
    )
    .split()
    .expect("θ is not zero");
    assert_eq!(factors.extent, 1, "the cutoff drops the value 1");
    assert_close(&factors.product(), &matrix([[1.0, 0.0], [0.0, 0.0]]));
}

/// The singular vectors of a zero θ are arbitrary, so a caller whose
/// product vanished (a gate that annihilates the state) is told so.
#[test]
fn a_gate_split_of_a_zero_product_fails() {
    GateSplit::of([Complex64::default(); 4], &[SvdSetting::PartitionUs])
        .split()
        .expect_err("θ is zero");
}

/// A caller's QR of a site tensor, whose descriptors order the modes as the
/// caller lays them out, is checked against the fake: Q·R is the input.
#[test]
fn a_qr_factors_its_input_whatever_its_descriptors_mode_order() {
    let [input, q_tensor, r_tensor] = qr(3).expect("the shared extent is the smaller side");
    let mut product = vec![Complex64::default(); 12];
    for (at, element) in product.iter_mut().enumerate() {
        let (i, k, j) = (at % 2, at / 2 % 2, at / 4);
        *element = (0..3)
            .map(|s| q_tensor[k + 2 * s + 6 * i] * r_tensor[j + 3 * s])
            .sum();
    }
    assert_close(&product, &input);
}

/// A QR moves a canonical MPS's centre: Q, left behind, is an isometry.
#[test]
fn a_qrs_q_has_orthonormal_columns() {
    let [_, q_tensor, _] = qr(3).expect("the shared extent is the smaller side");
    let gram: Vec<Complex64> = (0..9)
        .map(|at| {
            let (s, t) = (at % 3, at / 3);
            (0..4)
                .map(|row| {
                    let (i, k) = (row % 2, row / 2);
                    q_tensor[k + 2 * s + 6 * i].conj() * q_tensor[k + 2 * t + 6 * i]
                })
                .sum()
        })
        .collect();
    assert_close(&gram, &identity(3));
}

/// The QR is thin: a caller whose descriptors share more than the smaller
/// side (4 rows, 3 columns), or less, is caught on the fake.
#[test]
fn a_qr_needs_the_shared_extent_to_be_the_smaller_side() {
    for extent in [4, 2] {
        let refused = qr(extent).expect_err("Q and R do not share the smaller side");
        assert!(
            refused_for(&refused, "not the smaller side"),
            "{extent}: {refused}"
        );
    }
    qr(3).expect("Q and R share the smaller side");
}

/// cuTensorNet's QR splits its input's modes between Q and R, and sums
/// none: a caller whose input carries a mode that neither Q nor R keeps
/// (a bond left on the input) is caught on the fake rather than summed.
#[test]
fn a_qr_needs_its_input_modes_to_be_q_and_rs_other_modes() {
    const ELSEWHERE: i32 = 9;
    let run = |extents: &[i64], modes: &[i32]| {
        let qr = TensorQr::sized_with_input(FakeLibraries::new(), 3, extents, modes);
        qr.device.lend(WorkspaceSlot::DeviceScratch);
        qr.run_on(qr.device.stream)
    };
    run(&[2, 2, 3, 2], &[QR_I, QR_K, QR_J, ELSEWHERE])
        .expect_err("the input carries a mode neither Q nor R keeps");
    run(&[2, 2, 3], &[QR_I, QR_K, QR_J]).expect("the input's modes are Q's and R's other modes");
}

/// A gate split's U and V share one mode, which no input carries: a caller
/// that labels A's and B's bond as the shared mode, which the split would
/// sum and keep at once, is caught on the fake.
#[test]
fn a_gate_split_refuses_its_shared_mode_on_an_input() {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let with_bond = |bond| {
        let mut split = GateSplit::of(theta, &[SvdSetting::PartitionUs]);
        let device = &split.device;
        split.operands.a = device.operand(&[2, 2], &[P, bond], &values(4, 0.5));
        split.operands.b = device.operand(&[2, 2], &[bond, Q], &values(4, -0.25));
        device
            .fake
            .synchronize_copies()
            .expect("the copies were queued");
        split
    };
    let refused = with_bond(S)
        .split()
        .expect_err("A's and B's bond is labelled as U's and V's shared mode");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
    with_bond(BOND)
        .split()
        .expect("A's and B's bond has its own label");
}

/// A gate split is sized for its config: a caller that sizes it under an
/// SVD config of another handle is caught on the fake.
#[test]
fn a_gate_split_is_sized_under_a_config_of_its_handle() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let fake = &split.device.fake;
    let other_handle = fake.create_handle().expect("the fake has a device");
    let foreign = fake
        .create_svd_config(other_handle)
        .expect("the other handle is live");
    split
        .size_under(foreign)
        .expect_err("the config belongs to another handle");
    split
        .size_under(split.config)
        .expect("the config belongs to the split's handle");
}

/// cuTensorNet checks a descriptor's shape when it is created: one extent
/// per mode.
#[test]
fn a_tensor_descriptor_needs_one_extent_per_mode() {
    let device = Device::new(FakeLibraries::new());
    let create = |extents: &[i64], modes: &[i32]| {
        device
            .fake
            .create_tensor_descriptor(device.handle, TensorModes { extents, modes })
    };
    create(&[2, 3], &[1]).expect_err("two extents for one mode");
    create(&[2, 3], &[1, 2]).expect("an extent for each mode");
}

/// A mode of extent 0 holds no element: a caller whose bond was sized to
/// nothing is caught when it creates the descriptor.
#[test]
fn a_tensor_descriptor_refuses_an_extent_below_one() {
    let device = Device::new(FakeLibraries::new());
    let create = |extents: &[i64]| {
        device.fake.create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents,
                modes: &[1, 2],
            },
        )
    };
    create(&[2, 0]).expect_err("the second mode has extent 0");
    create(&[2, 1]).expect("every extent is at least 1");
}

/// A label names one mode of a tensor: a caller that gives a tensor one
/// label twice is caught when it creates the descriptor.
#[test]
fn a_tensor_descriptor_refuses_a_repeated_mode() {
    let device = Device::new(FakeLibraries::new());
    let create = |modes: &[i32]| {
        device.fake.create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents: &[2, 2, 2],
                modes,
            },
        )
    };
    create(&[1, 2, 1]).expect_err("the label 1 names two modes");
    create(&[1, 2, 3]).expect("every label names one mode");
}

/// A tensor's element count can overflow the host's addresses, which no
/// buffer could hold: a caller whose extents multiply past `usize` is
/// refused with an error, not a panic.
#[test]
fn a_tensor_whose_element_count_overflows_is_refused() {
    let device = Device::new(FakeLibraries::new());
    let zero = Complex64::default();
    let qr_of = |extent: i64, elements: usize| {
        let input = device.operand(&[extent, 3], &[1, 2], &values(elements, 0.5));
        let q = device.operand(&[extent, 3], &[1, 4], &vec![zero; elements]);
        let r = device.operand(&[3, 3], &[4, 2], &[zero; 9]);
        device
            .fake
            .synchronize_copies()
            .expect("the copies were queued");
        device.fake.qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )?;
        device.lend(WorkspaceSlot::DeviceScratch);
        device
            .fake
            .tensor_qr(device.handle, input, q, r, device.workspace, device.stream)
    };
    let refused = qr_of(i64::MAX, 1).expect_err("the input's element count overflows");
    assert!(
        matches!(refused, crate::NativeError::ResourceSizeOverflow { .. }),
        "{refused:?}"
    );
    qr_of(3, 9).expect("each buffer holds its tensor's 9 elements");
}

/// A tensor call writes its output on its stream, which a copy to the host
/// does not wait for: a caller that copies a contraction's output before
/// synchronizing that stream, or after synchronizing only another one, is
/// caught on the fake.
#[test]
fn a_copy_to_the_host_needs_the_writing_stream_synchronized() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let other_stream = device.fake.create_stream().expect("the fake has a device");
    contraction.contract().expect("the network is prepared");
    let mut output = [Complex64Abi::default()];
    device
        .fake
        .copy_to_host(&mut output, contraction.output)
        .expect_err("the stream may still be writing the output");
    device
        .fake
        .synchronize_stream(other_stream)
        .expect("the other stream is live");
    device
        .fake
        .copy_to_host(&mut output, contraction.output)
        .expect_err("only a stream that did not write the output is synchronized");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .copy_to_host(&mut output, contraction.output)
        .expect("the stream is synchronized");
    assert_eq!(Complex64::from(output[0]), Complex64::new(2.0, 0.0));
}

/// The copies on the device run on CUDA's legacy stream, which a
/// non-blocking stream's work does not wait for: a caller that copies from
/// a contraction's output, or into it, from the host or on the device,
/// before synchronizing the stream writing it, is caught on the fake.
#[test]
fn a_copy_on_the_device_needs_the_stream_writing_its_buffers_synchronized() {
    type CopyCall = fn(&Device, OpaqueHandle, OpaqueHandle) -> Result<(), crate::NativeError>;
    let copies: [(&str, CopyCall); 3] = [
        ("from the output", |device, output, other| {
            device.fake.copy_on_device(other, output, 16)
        }),
        ("into the output", |device, output, other| {
            device.fake.copy_on_device(output, other, 16)
        }),
        ("from the host into the output", |device, output, _| {
            device
                .fake
                .copy_to_device(output, &[Complex64Abi::default()])
        }),
    ];
    for (case, copy) in copies {
        let contraction = Contraction::dot_product(FakeLibraries::new());
        let device = &contraction.device;
        let other = device.zeros(&[1]);
        device
            .fake
            .synchronize_copies()
            .expect("the copy was queued");
        contraction.contract().expect("the network is prepared");
        copy(device, contraction.output, other).expect_err(&format!(
            "{case}: the stream may still be writing the output"
        ));
        device
            .fake
            .synchronize_stream(device.stream)
            .expect("the stream is live");
        copy(device, contraction.output, other)
            .unwrap_or_else(|error| panic!("{case}: the stream is synchronized: {error}"));
    }
}

/// A gate split and a QR write each of their outputs on their stream (the
/// split's U, V and singular values, the QR's Q and R): a caller that
/// copies any of them to the host before synchronizing it is caught on the
/// fake.
#[test]
fn every_output_of_a_tensor_call_is_read_once_its_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let device = &split.device;
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    let zero = Complex64::default();
    let input = device.operand(&[2, 3], &[1, 2], &values(6, 0.5));
    let q = device.operand(&[2, 2], &[1, 3], &[zero; 4]);
    let r = device.operand(&[2, 3], &[3, 2], &[zero; 6]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .fake
        .qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )
        .expect("the descriptors are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    device
        .fake
        .tensor_qr(device.handle, input, q, r, device.workspace, device.stream)
        .expect("the workspace is sized and lent");
    let outputs = [
        split.operands.u.data,
        split.operands.v.data,
        split.operands.singular_values,
        q.data,
        r.data,
    ];
    let mut values = [Complex64Abi::default()];
    for output in outputs {
        device
            .fake
            .copy_to_host(&mut values, output)
            .expect_err("the stream may still be writing the output");
    }
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    for output in outputs {
        device
            .fake
            .copy_to_host(&mut values, output)
            .expect("the stream is synchronized");
    }
}

/// The fake does not track which buffers a stream's work reads: a caller
/// that contracts on two streams and frees an input once only one of them
/// is synchronized, while a GPU may still be reading it on the other, is
/// caught on the fake.
#[test]
fn a_buffer_is_freed_once_every_stream_is_synchronized() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let second = device.fake.create_stream().expect("the fake has a device");
    let second_output = device.zeros(&[]);
    device
        .fake
        .synchronize_copies()
        .expect("the copy was queued");
    contraction.contract().expect("the network is prepared");
    contraction
        .contract_buffers(contraction.inputs, second_output, second)
        .expect("the network is prepared");
    device
        .fake
        .free(contraction.inputs[0])
        .expect_err("both contractions may still be reading the input");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .free(contraction.inputs[0])
        .expect_err("the second contraction may still be reading the input");
    device
        .fake
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .fake
        .free(contraction.inputs[0])
        .expect("every stream is synchronized");
}

/// A two-qubit state computed on `device`'s stream, which is not yet
/// synchronized, and the compute's output buffers, one per site.
fn computed_state(device: &Device) -> (OpaqueHandle, Vec<OpaqueHandle>) {
    computed_state_on(device, device.stream).expect("the state is prepared and its scratch lent")
}

/// A two-qubit state computed on `stream`, and the compute's output
/// buffers, one per site; or the compute's refusal.
fn computed_state_on(
    device: &Device,
    stream: Stream,
) -> Result<(OpaqueHandle, Vec<OpaqueHandle>), crate::NativeError> {
    let fake = &device.fake;
    let (state, capacity) = prepared_state(device, device.workspace);
    device.lend(WorkspaceSlot::DeviceScratch);
    let mut outputs: Vec<OpaqueHandle> = (0..2).map(|_| device.zeros(&[2, 2])).collect();
    fake.synchronize_copies().expect("the copies were queued");
    let mut layout = MpsLayout::new(&capacity);
    fake.compute_state(
        device.handle,
        state,
        device.workspace,
        &mut layout,
        &mut outputs,
        stream,
    )?;
    Ok((state, outputs))
}

/// A two-qubit state prepared in `workspace`, and its capacity.
fn prepared_state(device: &Device, workspace: OpaqueHandle) -> (OpaqueHandle, NativeCapacity) {
    let fake = &device.fake;
    let state = fake
        .create_state(device.handle, &[2, 2])
        .expect("the handle is live");
    let shape = MpsShape::new(vec![vec![2, 2], vec![2, 2]]).expect("a two-site chain");
    let capacity = NativeCapacity::new(&shape).expect("the extents fit cuTensorNet's");
    fake.finalize_mps(device.handle, state, &capacity)
        .expect("the state is finalized once");
    fake.prepare_state(device.handle, state, 1 << 20, workspace, device.stream)
        .expect("the state is finalized");
    (state, capacity)
}

/// An expectation on a two-qubit state, prepared in `workspace`.
fn prepared_expectation(device: &Device, workspace: OpaqueHandle) {
    let (fake, handle) = (&device.fake, device.handle);
    let state = fake
        .create_state(handle, &[2, 2])
        .expect("the handle is live");
    let operator = fake
        .create_network_operator(handle, &[2, 2])
        .expect("the handle is live");
    let expectation = fake
        .create_expectation(handle, state, operator)
        .expect("the state and operator are live");
    fake.prepare_expectation(handle, expectation, 1 << 20, workspace, device.stream)
        .expect("the expectation is live");
}

/// A compute is queued on a stream: a caller that passes a destroyed one is
/// caught on the fake.
#[test]
fn a_compute_runs_on_a_live_stream() {
    let device = Device::new(FakeLibraries::new());
    let stream = device.fake.create_stream().expect("the fake has a device");
    device
        .fake
        .destroy_stream(stream)
        .expect("the stream is live");
    computed_state_on(&device, stream).expect_err("the compute's stream is destroyed");
    let device = Device::new(FakeLibraries::new());
    computed_state_on(&device, device.stream).expect("the compute's stream is live");
}

/// A compute writes the state's site tensors on its stream: a caller that
/// copies one to the host before synchronizing that stream, or after
/// synchronizing only another one, is caught on the fake.
#[test]
fn a_computed_site_is_copied_to_the_host_once_its_stream_is_synchronized() {
    let device = Device::new(FakeLibraries::new());
    let other_stream = device.fake.create_stream().expect("the fake has a device");
    let (_, outputs) = computed_state(&device);
    let mut values = [Complex64Abi::default()];
    device
        .fake
        .copy_to_host(&mut values, outputs[0])
        .expect_err("the compute may still be writing the site");
    device
        .fake
        .synchronize_stream(other_stream)
        .expect("the other stream is live");
    device
        .fake
        .copy_to_host(&mut values, outputs[0])
        .expect_err("only a stream that did not write the site is synchronized");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .copy_to_host(&mut values, outputs[0])
        .expect("the stream is synchronized");
}

/// `capture_mps` waits for every stream, a cautious choice of the fake's
/// (the rules on streams, in its header): a caller that captures a compute
/// once its stream is synchronized, while a contraction it queued on
/// another stream may still run, is caught on the fake.
#[test]
fn a_compute_is_captured_once_every_stream_is_synchronized() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let second = device.fake.create_stream().expect("the fake has a device");
    contraction
        .contract_on(second)
        .expect("the network is prepared");
    let (state, _) = computed_state(device);
    device
        .fake
        .capture_mps(device.handle, state)
        .expect_err("the compute may still be running");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .capture_mps(device.handle, state)
        .expect_err("the contraction on the second stream may still run");
    device
        .fake
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .fake
        .capture_mps(device.handle, state)
        .expect("every stream is synchronized");
}

/// Work queued on one stream does not hold back a read of what another,
/// synchronized stream wrote: a caller that runs a QR on a second stream
/// still reads a gate split's factors and report once the split's stream
/// is synchronized, and is refused only the QR's factors.
#[test]
fn a_read_waits_only_for_the_stream_that_wrote_it() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let device = &split.device;
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    let zero = Complex64::default();
    let input = device.operand(&[2, 3], &[1, 2], &values(6, 0.5));
    let q = device.operand(&[2, 2], &[1, 3], &[zero; 4]);
    let r = device.operand(&[2, 3], &[3, 2], &[zero; 6]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .fake
        .qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )
        .expect("the descriptors are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    let qr_stream = device.fake.create_stream().expect("the fake has a device");
    device
        .fake
        .tensor_qr(device.handle, input, q, r, device.workspace, qr_stream)
        .expect("the workspace is sized and lent");
    let mut values = [Complex64Abi::default()];
    device
        .fake
        .svd_truncation(device.handle, split.info)
        .expect("the split's stream is synchronized");
    device
        .fake
        .copy_to_host(&mut values, split.operands.u.data)
        .expect("the split's stream is synchronized");
    device
        .fake
        .copy_to_host(&mut values, q.data)
        .expect_err("the QR's stream may still be writing Q");
}

/// The fake records, for what is written, the stream last queued to write
/// it: a caller that contracts into one output on two streams and copies it
/// to the host once only the first stream is synchronized is caught on the
/// fake.
#[test]
fn a_read_waits_for_the_last_stream_that_wrote_it() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let second = device.fake.create_stream().expect("the fake has a device");
    contraction.contract().expect("the first write");
    contraction.contract_on(second).expect("the second write");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the first stream is live");
    let mut output = [Complex64Abi::default()];
    device
        .fake
        .copy_to_host(&mut output, contraction.output)
        .expect_err("the second stream may still be writing the output");
    device
        .fake
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .fake
        .copy_to_host(&mut output, contraction.output)
        .expect("the last writer is synchronized");
}

/// CUDA lets a stream's work run on after `cudaStreamDestroy`: a caller
/// that destroys the stream of a contraction and then frees the
/// contraction's input is caught on the fake.
#[test]
fn a_destroyed_stream_keeps_its_writes_pending() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let doomed = device.fake.create_stream().expect("the fake has a device");
    contraction
        .contract_on(doomed)
        .expect("the network is prepared");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .free(contraction.inputs[0])
        .expect_err("the contraction may still be reading its input");
    device
        .fake
        .destroy_stream(doomed)
        .expect("the stream is live");
    device
        .fake
        .free(contraction.inputs[0])
        .expect_err("the destroyed stream's work may still run");
}

/// The fake records one writer per pending write, the stream last queued to
/// write it: once a second stream contracts into the first one's output and
/// is synchronized, the fake no longer holds `free` for the first stream's
/// contraction, which a GPU may still be running. A caller's test does not
/// catch that misuse on the fake.
#[test]
fn a_free_waits_only_for_the_last_writer_of_each_pending_write() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let second = device.fake.create_stream().expect("the fake has a device");
    contraction.contract().expect("the first write");
    contraction.contract_on(second).expect("the second write");
    device
        .fake
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .fake
        .free(contraction.inputs[0])
        .expect("the first stream is no longer the output's recorded writer");
}

/// A stream is synchronized while it exists: a caller that synchronizes one
/// it destroyed is caught on the fake.
#[test]
fn a_stream_is_synchronized_only_while_live() {
    let device = Device::new(FakeLibraries::new());
    let stream = device.fake.create_stream().expect("the fake has a device");
    device
        .fake
        .synchronize_stream(stream)
        .expect("the stream is live");
    device
        .fake
        .destroy_stream(stream)
        .expect("the stream is live");
    device
        .fake
        .synchronize_stream(stream)
        .expect_err("the stream is destroyed");
}

/// No tensor is empty, so the loaded libraries refuse an empty copy before
/// calling CUDA: a caller that copies nothing is refused on the fake as
/// well, both ways.
#[test]
fn an_empty_copy_is_refused() {
    let device = Device::new(FakeLibraries::new());
    let buffer = device.zeros(&[1]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .fake
        .copy_to_device(buffer, &[])
        .expect_err("the copy to the device is empty");
    device
        .fake
        .copy_to_device(buffer, &[Complex64Abi::default()])
        .expect("the copy to the device holds one element");
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .fake
        .copy_to_host(&mut [], buffer)
        .expect_err("the copy to the host is empty");
    device
        .fake
        .copy_to_host(&mut [Complex64Abi::default()], buffer)
        .expect("the copy to the host holds one element");
}

/// A caller that sizes a workspace for, or prepares, a contraction before
/// its network's path is optimized is caught on the fake.
#[test]
fn a_contraction_needs_an_optimized_path() {
    let device = Device::new(FakeLibraries::new());
    device.lend(WorkspaceSlot::DeviceScratch);
    let (fake, handle, workspace) = (&device.fake, device.handle, device.workspace);
    let network = fake
        .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = fake
        .create_optimizer_info(handle, network)
        .expect("the network is live");
    fake.contraction_workspace_sizes(handle, network, info, workspace)
        .expect_err("the path is not optimized");
    fake.prepare_contraction(handle, network, workspace)
        .expect_err("the path is not optimized");
    fake.optimize_contraction(handle, network, 1 << 20, info)
        .expect("the info is the network's");
    fake.contraction_workspace_sizes(handle, network, info, workspace)
        .expect("the path is optimized");
    fake.prepare_contraction(handle, network, workspace)
        .expect("the workspace is sized for the path");
}

/// Two dot-product networks on one handle, each with its optimizer info;
/// `optimized` says whether each info's path is searched.
fn two_networks(device: &Device, optimized: bool) -> [(OpaqueHandle, OpaqueHandle); 2] {
    let (fake, handle) = (&device.fake, device.handle);
    [(); 2].map(|()| {
        let network = fake
            .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
            .expect("the labels are consistent")
            .descriptor;
        let info = fake
            .create_optimizer_info(handle, network)
            .expect("the network is live");
        if optimized {
            fake.optimize_contraction(handle, network, 1 << 20, info)
                .expect("the info is the network's");
        }
        (network, info)
    })
}

/// A caller that caches a path per shape and searches one network's path
/// into another's optimizer info, mixing up two cache entries, is caught
/// on the fake.
#[test]
fn a_contraction_path_is_searched_with_an_info_of_its_network() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle) = (&device.fake, device.handle);
    let [(first, first_info), (_, second_info)] = two_networks(&device, false);
    fake.optimize_contraction(handle, first, 1 << 20, second_info)
        .expect_err("the info is the second network's");
    fake.optimize_contraction(handle, first, 1 << 20, first_info)
        .expect("the info is the first network's");
}

/// A caller that sizes a workspace for one network with the optimized path
/// of another, mixing up two cache entries, is caught on the fake.
#[test]
fn a_contraction_workspace_is_sized_with_an_info_of_its_network() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle, workspace) = (&device.fake, device.handle, device.workspace);
    let [(first, first_info), (_, second_info)] = two_networks(&device, true);
    fake.contraction_workspace_sizes(handle, first, second_info, workspace)
        .expect_err("the info is the second network's");
    fake.contraction_workspace_sizes(handle, first, first_info, workspace)
        .expect("the info is the first network's");
}

/// cuTensorNet prepares a network in the workspace sized for its path: a
/// caller that prepares an optimized network in a workspace it never sized
/// is caught on the fake.
#[test]
fn a_contraction_is_prepared_in_a_workspace_sized_for_its_path() {
    let device = Device::new(FakeLibraries::new());
    device.lend(WorkspaceSlot::DeviceScratch);
    let (fake, handle, workspace) = (&device.fake, device.handle, device.workspace);
    let network = fake
        .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = fake
        .create_optimizer_info(handle, network)
        .expect("the network is live");
    fake.optimize_contraction(handle, network, 1 << 20, info)
        .expect("the info is the network's");
    fake.prepare_contraction(handle, network, workspace)
        .expect_err("the workspace is not sized");
    fake.contraction_workspace_sizes(handle, network, info, workspace)
        .expect("the path is optimized");
    fake.prepare_contraction(handle, network, workspace)
        .expect("the workspace is sized for the path");
}

/// cuTensorNet checks the device scratch's size when it prepares a
/// network: a caller that prepares before lending the workspace its memory
/// is caught on the fake.
#[test]
fn a_contraction_is_prepared_with_its_device_scratch_lent() {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        FakeLibraries::new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    contraction
        .prepare()
        .expect_err("the device scratch is not lent");
    contraction.device.lend(WorkspaceSlot::DeviceScratch);
    contraction
        .prepare()
        .expect("the device scratch is lent what it asks for");
}

/// cuTensorNet contracts with at least the device scratch it was prepared
/// with: a caller that prepares a network with a large scratch, then lends
/// a smaller one to contract it, is caught on the fake.
#[test]
fn a_contraction_needs_at_least_the_scratch_lent_at_its_preparation() {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        FakeLibraries::new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    let asked = device.asked(WorkspaceSlot::DeviceScratch);
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked);
    contraction
        .prepare()
        .expect("the device scratch is lent more than it asks for");
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked - 1);
    contraction
        .contract()
        .expect_err("the device scratch is one byte short of the preparation's");
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked);
    contraction
        .contract()
        .expect("the device scratch is the preparation's");
}

/// cuTensorNet's optimizer infos refer to their network: a caller that
/// destroys a network first is caught on the fake.
#[test]
fn a_network_outlives_its_optimizer_infos() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let fake = &contraction.device.fake;
    fake.destroy_contraction_network(contraction.network.descriptor)
        .expect_err("its optimizer info is live");
    fake.destroy_optimizer_info(contraction.info)
        .expect("the info is live");
    fake.destroy_contraction_network(contraction.network.descriptor)
        .expect("nothing of the network is live");
}

/// cuTensorNet contracts a network as its last preparation set it up, for
/// the path it had then: a caller that contracts an optimized network it
/// never prepared, or did not prepare again after optimizing it again
/// (sizing the workspace again, so that only the preparation is missing),
/// is caught on the fake.
#[test]
fn a_contraction_needs_a_network_prepared_for_its_path() {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        FakeLibraries::new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let refused = contraction
        .contract()
        .expect_err("the network is not prepared");
    assert!(refused_for(&refused, "not prepared"), "{refused}");
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction.contract().expect("the network is prepared");
    let network = contraction.network.descriptor;
    device
        .fake
        .optimize_contraction(device.handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    device
        .fake
        .contraction_workspace_sizes(device.handle, network, contraction.info, device.workspace)
        .expect("the path is optimized");
    let refused = contraction
        .contract()
        .expect_err("the network is not prepared for its new path");
    assert!(refused_for(&refused, "not prepared"), "{refused}");
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction
        .contract()
        .expect("the network is prepared again");
}

/// cuTensorNet's optimizer may find another path at each search, even into
/// the same optimizer info, and a workspace sized for the old path may not
/// hold the new one's: a caller that searches a prepared network's path
/// again and prepares it without sizing the workspace again is caught on
/// the fake.
#[test]
fn a_new_path_search_needs_the_workspace_sized_again() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let network = contraction.network.descriptor;
    device
        .fake
        .optimize_contraction(device.handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    let refused = contraction
        .prepare()
        .expect_err("the workspace is sized for the earlier search's path");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    device
        .fake
        .contraction_workspace_sizes(device.handle, network, contraction.info, device.workspace)
        .expect("the path is optimized");
    contraction
        .prepare()
        .expect("the workspace is sized for the new path");
    contraction
        .contract()
        .expect("the network is prepared for its new path");
}

/// A new path search changes the path every workspace sized for the network
/// was sized for: a caller that keeps two workspaces sized for one network,
/// searches its path again and prepares it in either without sizing that
/// one again is caught on the fake.
#[test]
fn a_new_path_search_needs_every_workspace_sized_again() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let (fake, handle) = (&device.fake, device.handle);
    let network = contraction.network.descriptor;
    let second = fake.create_workspace(handle).expect("the handle is live");
    fake.contraction_workspace_sizes(handle, network, contraction.info, second)
        .expect("the path is optimized");
    let bytes = fake
        .workspace_size(handle, second, WorkspaceSlot::DeviceScratch)
        .expect("the workspace is live");
    let memory = fake
        .allocate(usize::try_from(bytes).expect("a size is not negative"))
        .expect("the fake has memory");
    fake.set_workspace(handle, second, WorkspaceSlot::DeviceScratch, memory, bytes)
        .expect("the buffer holds the bytes lent");
    fake.optimize_contraction(handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    for workspace in [device.workspace, second] {
        let refused = fake
            .prepare_contraction(handle, network, workspace)
            .expect_err("the workspace is sized for the earlier search's path");
        assert!(
            refused_for(&refused, "not sized for this call"),
            "{refused}"
        );
    }
    for workspace in [device.workspace, second] {
        fake.contraction_workspace_sizes(handle, network, contraction.info, workspace)
            .expect("the path is optimized");
        fake.prepare_contraction(handle, network, workspace)
            .expect("the workspace is sized for the new path");
    }
}

/// A path search changes its network's path, not a workspace's sizing for
/// a call of another kind: a caller that searches a new network's path
/// between sizing a QR and running it runs the QR without sizing again.
#[test]
fn a_path_search_keeps_the_sizing_for_a_qr() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    two_networks(device, true);
    qr.run_on(device.stream)
        .expect("the path searches were for other networks");
}

/// A path search changes its own network's path alone: a caller that keeps
/// one network per shape, and searches a new shape's path between sizing a
/// workspace for one network and preparing it, prepares it without sizing
/// again.
#[test]
fn a_path_search_keeps_the_sizing_for_another_network() {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        FakeLibraries::new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    two_networks(device, true);
    contraction
        .prepare()
        .expect("the path searches were for other networks");
}

/// cuTensorNet takes each output mode's extent from the inputs: a caller
/// whose output names a label no input has is caught on the fake.
#[test]
fn a_contraction_output_label_must_be_an_input_label() {
    let device = Device::new(FakeLibraries::new());
    device
        .fake
        .create_contraction_network(device.handle, DOT_PRODUCT_INPUTS, &[2])
        .expect_err("no input has the label 2");
    device
        .fake
        .create_contraction_network(device.handle, DOT_PRODUCT_INPUTS, &[1])
        .expect("both inputs have the label 1");
}

/// A label names one mode of the network: a caller whose two inputs give
/// one label two extents is caught on the fake.
#[test]
fn a_contraction_label_has_one_extent() {
    let device = Device::new(FakeLibraries::new());
    let inputs = |extent| {
        [
            DOT_PRODUCT_INPUTS[0],
            TensorModes {
                extents: extent,
                modes: &[1],
            },
        ]
    };
    device
        .fake
        .create_contraction_network(device.handle, inputs(&[3]), &[])
        .expect_err("the label 1 has the extents 2 and 3");
    device
        .fake
        .create_contraction_network(device.handle, inputs(&[2]), &[])
        .expect("the label 1 has the extent 2 in both inputs");
}

/// A simulator keeps one prepared network per shape and contracts it again
/// on new buffers at each step: the second contraction on the fake computes
/// from its own buffers, as the first did from its.
#[test]
fn a_prepared_network_contracts_new_buffers_each_time() {
    let first = [
        [Complex64::new(1.0, 1.0), Complex64::new(2.0, 0.0)],
        [Complex64::new(3.0, 0.0), Complex64::new(4.0, -1.0)],
    ];
    let contraction = Contraction::prepared(
        FakeLibraries::new(),
        DOT_PRODUCT_INPUTS,
        [&first[0], &first[1]],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    contraction.contract().expect("the network is prepared");
    let second = [
        [Complex64::new(5.0, 0.0), Complex64::new(0.0, 6.0)],
        [Complex64::new(7.0, 2.0), Complex64::new(-1.0, 0.0)],
    ];
    let inputs = [device.buffer(&second[0]), device.buffer(&second[1])];
    let output = device.zeros(&[]);
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    contraction
        .contract_buffers(inputs, output, device.stream)
        .expect("the network is still prepared");
    let dot = |[a, b]: [[Complex64; 2]; 2]| a[0] * b[0] + a[1] * b[1];
    assert_eq!(contraction.output(1), vec![dot(first)]);
    assert_eq!(device.read(output, 1), vec![dot(second)]);
}

/// A workspace holds the sizes of the call it was last sized for: a caller
/// that runs a gate split under another SVD config, without sizing again,
/// is caught on the fake.
#[test]
fn a_gate_split_needs_a_workspace_sized_for_its_config() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let device = &split.device;
    let other = device
        .fake
        .create_svd_config(device.handle)
        .expect("the handle is live");
    device
        .fake
        .configure_svd(device.handle, other, SvdSetting::PartitionUs)
        .expect("the setting is in range");
    split.size(split.config);
    split
        .run(other, device.stream)
        .expect_err("the workspace is sized for the other config");
    split.size(other);
    split
        .run(other, device.stream)
        .expect("the workspace is sized for this config");
}

/// A workspace is sized for a gate split's descriptors, not their shapes: a
/// caller that sizes it with a new descriptor of U's shape, then runs the
/// split with U's own, is caught on the fake.
#[test]
fn a_gate_split_needs_a_workspace_sized_for_its_descriptors() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let (device, operands) = (&split.device, &split.operands);
    let same_shape_as_u = device
        .fake
        .create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents: &[2, 2],
                modes: &[P_OUT, S],
            },
        )
        .expect("the extents and modes match");
    device
        .fake
        .gate_split_workspace_sizes(
            device.handle,
            &GateSplitDescriptors {
                a: operands.a.descriptor,
                b: operands.b.descriptor,
                gate: operands.gate.descriptor,
                u: same_shape_as_u,
                v: operands.v.descriptor,
            },
            split.config,
            device.workspace,
        )
        .expect("the descriptors and config are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    let refused = split
        .run(split.config, device.stream)
        .expect_err("the workspace is sized for another U descriptor");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    split.size_workspace(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized for these descriptors");
}

/// A caller that shares one workspace between its tensor calls, sizes it
/// for a contraction, then runs a QR without sizing it again is caught on
/// the fake.
#[test]
fn a_qr_needs_a_workspace_sized_for_it() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let [(network, info), _] = two_networks(device, true);
    device
        .fake
        .contraction_workspace_sizes(device.handle, network, info, device.workspace)
        .expect("the path is optimized");
    let refused = qr
        .run_on(device.stream)
        .expect_err("the workspace is sized for a contraction");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    qr.size();
    qr.run_on(device.stream)
        .expect("the workspace is sized for the QR");
}

/// A caller that keeps one prepared network per shape in one workspace,
/// sizes that workspace for another network, then contracts the first
/// without sizing again is caught on the fake.
#[test]
fn a_contraction_needs_a_workspace_sized_for_its_path() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let device = &contraction.device;
    let [(other, other_info), _] = two_networks(device, true);
    device
        .fake
        .contraction_workspace_sizes(device.handle, other, other_info, device.workspace)
        .expect("the path is optimized");
    let refused = contraction
        .contract()
        .expect_err("the workspace is sized for another network");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    device
        .fake
        .contraction_workspace_sizes(
            device.handle,
            contraction.network.descriptor,
            contraction.info,
            device.workspace,
        )
        .expect("the path is optimized");
    contraction
        .contract()
        .expect("the workspace is sized for the network's path");
}

/// cuTensorNet's state preparation sets the sizes of the workspace it is
/// given, for the state (`cutensornetStatePrepare`): a caller that shares
/// one workspace between a state and a QR, and runs the QR after preparing
/// the state without sizing the workspace for the QR again, is caught on
/// the fake.
#[test]
fn a_state_preparation_undoes_the_sizing_for_a_tensor_call() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    prepared_state(device, device.workspace);
    let refused = qr
        .run_on(device.stream)
        .expect_err("the workspace is sized for the state");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    qr.size();
    qr.run_on(device.stream)
        .expect("the workspace is sized for the QR again");
}

/// cuTensorNet's expectation preparation sets the sizes of the workspace it
/// is given, as a state's does (`cutensornetExpectationPrepare`): a caller
/// that runs a QR after preparing an expectation in the QR's workspace,
/// without sizing it for the QR again, is caught on the fake.
#[test]
fn an_expectation_preparation_undoes_the_sizing_for_a_tensor_call() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    prepared_expectation(device, device.workspace);
    let refused = qr
        .run_on(device.stream)
        .expect_err("the workspace is sized for the expectation");
    assert!(
        refused_for(&refused, "not sized for this call"),
        "{refused}"
    );
    qr.size();
    qr.run_on(device.stream)
        .expect("the workspace is sized for the QR again");
}

/// cuTensorNet's state preparation sets the sizes of the workspace it is
/// given alone: a caller that keeps its QR's workspace apart from its
/// states' runs the QR after preparing a state in the other workspace,
/// without sizing again.
#[test]
fn a_state_preparation_keeps_the_sizing_of_another_workspace() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let states_workspace = device
        .fake
        .create_workspace(device.handle)
        .expect("the handle is live");
    prepared_state(device, states_workspace);
    qr.run_on(device.stream)
        .expect("the preparation sized another workspace");
}

/// cuTensorNet's expectation preparation sets the sizes of the workspace it
/// is given alone: a caller that keeps its QR's workspace apart from its
/// expectations' runs the QR after preparing an expectation in the other
/// workspace, without sizing again.
#[test]
fn an_expectation_preparation_keeps_the_sizing_of_another_workspace() {
    let qr = TensorQr::sized(FakeLibraries::new(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let expectations_workspace = device
        .fake
        .create_workspace(device.handle)
        .expect("the handle is live");
    prepared_expectation(device, expectations_workspace);
    qr.run_on(device.stream)
        .expect("the preparation sized another workspace");
}

/// cuTensorNet writes a gate split's report on the split's stream: a
/// caller that reads it before synchronizing that stream, or after
/// synchronizing only another one, is caught on the fake.
#[test]
fn an_svd_info_is_read_once_its_writing_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let device = &split.device;
    let other_stream = device.fake.create_stream().expect("the fake has a device");
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    device
        .fake
        .svd_truncation(device.handle, split.info)
        .expect_err("the stream may still be writing the info");
    device
        .fake
        .synchronize_stream(other_stream)
        .expect("the other stream is live");
    device
        .fake
        .svd_truncation(device.handle, split.info)
        .expect_err("only a stream that did not write the info is synchronized");
    device
        .fake
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .fake
        .svd_truncation(device.handle, split.info)
        .expect("the stream is synchronized");
}

/// Every object belongs to the handle it was created with: a caller that
/// mixes a descriptor of one handle into a call on another is caught on
/// the fake.
#[test]
fn a_tensor_call_refuses_an_object_of_another_handle() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle) = (&device.fake, device.handle);
    let other_handle = fake.create_handle().expect("the fake has a device");
    let descriptor = |handle, extents: &[i64], modes: &[i32]| {
        fake.create_tensor_descriptor(handle, TensorModes { extents, modes })
            .expect("the extents and modes match")
    };
    let q = descriptor(handle, &[2, 2], &[1, 3]);
    let r = descriptor(handle, &[2, 3], &[3, 2]);
    let foreign = descriptor(other_handle, &[2, 3], &[1, 2]);
    fake.qr_workspace_sizes(handle, foreign, q, r, device.workspace)
        .expect_err("the input belongs to another handle");
    let own = descriptor(handle, &[2, 3], &[1, 2]);
    fake.qr_workspace_sizes(handle, own, q, r, device.workspace)
        .expect("every descriptor belongs to the handle");
}

/// A discarded-weight cutoff is a fraction of the total weight: a caller
/// that passes one outside [0, 1] is refused.
#[test]
fn an_svd_cutoff_outside_zero_to_one_is_refused() {
    let device = Device::new(FakeLibraries::new());
    let config = device
        .fake
        .create_svd_config(device.handle)
        .expect("the handle is live");
    let configure = |cutoff| {
        device.fake.configure_svd(
            device.handle,
            config,
            SvdSetting::DiscardedWeightCutoff(cutoff),
        )
    };
    configure(-0.1).expect_err("the cutoff is below 0");
    configure(1.5).expect_err("the cutoff is above 1");
    configure(0.0).expect("0 is a fraction");
    configure(1.0).expect("1 is a fraction");
}

/// The fake refuses memory for a slot that asks for none, a rule wider
/// than the evidence (on an A100, cuTensorNet refuses a zero-size
/// workspace with a pointer): a caller that lends every slot regardless of
/// its size is caught on the fake.
#[test]
fn a_workspace_slot_that_asks_for_nothing_is_lent_nothing() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle, workspace) = (&device.fake, device.handle, device.workspace);
    let memory = device.host(64);
    fake.set_workspace(handle, workspace, WorkspaceSlot::HostCache, memory, 64)
        .expect_err("the host cache asks for nothing");
    fake.set_other_workspace_slot_bytes(64);
    fake.set_workspace(handle, workspace, WorkspaceSlot::HostCache, memory, 64)
        .expect("the host cache asks for 64 bytes");
}

/// A gate split, a QR and a contraction are queued on a stream: a caller
/// that passes a destroyed one is caught on the fake.
#[test]
fn a_tensor_call_runs_on_a_live_stream() {
    let destroyed_stream = |device: &Device| {
        let stream = device.fake.create_stream().expect("the fake has a device");
        device
            .fake
            .destroy_stream(stream)
            .expect("the stream is live");
        stream
    };
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    split.size(split.config);
    split
        .run(split.config, destroyed_stream(&split.device))
        .expect_err("the gate split's stream is destroyed");
    split
        .run(split.config, split.device.stream)
        .expect("the gate split's stream is live");
    qr_on(3, destroyed_stream).expect_err("the QR's stream is destroyed");
    let contraction = Contraction::dot_product(FakeLibraries::new());
    contraction
        .contract_on(destroyed_stream(&contraction.device))
        .expect_err("the contraction's stream is destroyed");
    contraction
        .contract()
        .expect("the contraction's stream is live");
}

/// A gate split contracts A, B and the gate along the labels they share,
/// and keeps the others in U or V: a caller whose gate carries a label no
/// other tensor has, which the split would sum alone, is caught on the
/// fake.
#[test]
fn a_gate_split_refuses_a_label_outside_its_topology() {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let mut split = GateSplit::of(theta, &[SvdSetting::PartitionUs]);
    split.split().expect("every label is shared or kept");
    let extra_label = 7;
    let mut gate = values(16, 0.25);
    gate[..4].copy_from_slice(&theta);
    split.operands.gate =
        split
            .device
            .operand(&[2, 2, 2, 2, 1], &[P_OUT, Q_OUT, P, Q, extra_label], &gate);
    split
        .device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let refused = split
        .split()
        .expect_err("the gate's extra label is in no other tensor");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split, under a cutoff, of A, B and the gate carrying the labels
/// `a`, `b` and `gate`, into U and V carrying `u` and `v`; every mode has
/// extent 2 but the bond between A and B, of extent 1.
fn split_with_labels(
    a: &[i32],
    b: &[i32],
    gate: &[i32],
    u: &[i32],
    v: &[i32],
) -> Result<(), crate::NativeError> {
    let mut split = GateSplit::of(
        matrix([[3.0, 0.0], [0.0, 1.0]]),
        &[SvdSetting::PartitionUs, ROUNDING_CUTOFF],
    );
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
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    split.split().map(|_| ())
}

/// The gate of a two-site split, G[p', q', p, q].
const GATE: [i32; 4] = [P_OUT, Q_OUT, P, Q];

/// Whether `refused` is the fake's refusal for `reason`.
fn refused_for(refused: &crate::NativeError, reason: &str) -> bool {
    refused.to_string().contains(reason)
}

/// cuTensorNet's gate split takes A, B and the gate as a fully connected
/// graph: a caller whose A and B share no bond, so that the split is not a
/// two-site gate on neighbouring sites, or whose gate acts on one site
/// alone, sharing no label with B's site or none with A's, is caught on the
/// fake.
#[test]
fn a_gate_split_needs_a_b_and_the_gate_pairwise_connected() {
    let not_connected = "not pairwise connected";
    split_with_labels(&[P, BOND], &[BOND, Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("each pair shares a label");
    let refused = split_with_labels(&[P], &[Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect_err("A and B share no label");
    assert!(refused_for(&refused, not_connected), "{refused}");
    let refused = split_with_labels(&[P, BOND], &[BOND, Q], &[Q_OUT, Q], &[P, S], &[S, Q_OUT])
        .expect_err("A and the gate share no label");
    assert!(refused_for(&refused, not_connected), "{refused}");
    let refused = split_with_labels(&[P, BOND], &[BOND, Q], &[P_OUT, P], &[P_OUT, S], &[S, Q])
        .expect_err("B and the gate share no label");
    assert!(refused_for(&refused, not_connected), "{refused}");
}

/// A gate split contracts each label two inputs share: a caller whose A, B
/// and gate all carry one label, which the split could neither contract
/// pairwise nor keep, is caught on the fake.
#[test]
fn a_gate_split_refuses_a_label_on_all_three_inputs() {
    let hyperedge = OUTER + 1;
    split_with_labels(&[P, BOND], &[BOND, Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("each label is on one or two inputs");
    let refused = split_with_labels(
        &[P, BOND, hyperedge],
        &[BOND, Q, hyperedge],
        &[P_OUT, Q_OUT, P, Q, hyperedge],
        &[P_OUT, S],
        &[S, Q_OUT],
    )
    .expect_err("a label is on all three inputs");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split contracts the labels two inputs share: a caller that also
/// keeps A's and B's bond in U, where the split would sum it and keep it at
/// once, is caught on the fake.
#[test]
fn a_gate_split_refuses_a_contracted_label_kept_in_an_output() {
    let (a, b) = ([P, BOND], [BOND, Q]);
    split_with_labels(&a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("A's and B's bond is contracted only");
    let refused = split_with_labels(&a, &b, &GATE, &[P_OUT, BOND, S], &[S, Q_OUT])
        .expect_err("A's and B's bond is also kept in U");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// U's and V's shared mode is made by the split: a caller that also labels
/// a mode of A alone with it, which no output could keep, is caught on the
/// fake.
#[test]
fn a_gate_split_refuses_its_shared_mode_on_a_single_input() {
    let b = [BOND, Q];
    split_with_labels(
        &[P, BOND, OUTER],
        &b,
        &GATE,
        &[OUTER, P_OUT, S],
        &[S, Q_OUT],
    )
    .expect("A's outer bond has its own label");
    let refused = split_with_labels(&[P, BOND, S], &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect_err("A's outer bond is labelled as U's and V's shared mode");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split keeps A's site in U: a caller that puts a label of A alone
/// (its outer bond) in V is caught on the fake.
#[test]
fn a_gate_split_keeps_a_label_of_a_alone_in_u() {
    let (a, b) = ([OUTER, P, BOND], [BOND, Q]);
    split_with_labels(&a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect("A's outer bond is in U");
    let refused = split_with_labels(&a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect_err("A's outer bond is in V");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split keeps B's site in V: a caller that puts a label of B alone
/// (its outer bond) in U is caught on the fake.
#[test]
fn a_gate_split_keeps_a_label_of_b_alone_in_v() {
    let (a, b) = ([P, BOND], [BOND, Q, OUTER]);
    split_with_labels(&a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect("B's outer bond is in V");
    let refused = split_with_labels(&a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect_err("B's outer bond is in U");
    assert!(
        refused_for(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate's outputs belong to neither site until the split places them: a
/// caller that keeps the gate's second output in U and its first in V, as
/// a swap of the two sites does, gets its split on the fake.
#[test]
fn a_gate_split_keeps_a_label_of_the_gate_alone_on_either_side() {
    let (a, b) = ([P, BOND], [BOND, Q]);
    split_with_labels(&a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("the gate's first output is in U, its second in V");
    split_with_labels(&a, &b, &GATE, &[Q_OUT, S], &[S, P_OUT])
        .expect("the gate's second output is in U, its first in V");
}

/// A site of a product state, or one at the chain's end, has bonds of
/// extent 1: a caller whose every mode, the gate's too, has extent 1 still
/// gets the product of A, B and the gate, at a reduced and full extent of
/// 1.
#[test]
fn a_gate_split_takes_modes_of_extent_one() {
    let device = Device::new(FakeLibraries::new());
    let config = device
        .fake
        .create_svd_config(device.handle)
        .expect("the handle is live");
    device
        .fake
        .configure_svd(device.handle, config, SvdSetting::PartitionSv)
        .expect("the setting is in range");
    let info = device
        .fake
        .create_svd_info(device.handle)
        .expect("the handle is live");
    let a_value = Complex64::new(1.0, 2.0);
    let b_value = Complex64::new(0.5, -0.5);
    let gate_value = Complex64::new(-1.0, 0.25);
    let zero = Complex64::default();
    let operands = GateSplitOperands {
        a: device.operand(&[1, 1], &[P, BOND], &[a_value]),
        b: device.operand(&[1, 1], &[BOND, Q], &[b_value]),
        gate: device.operand(&[1, 1, 1, 1], &[P_OUT, Q_OUT, P, Q], &[gate_value]),
        u: device.operand(&[1, 1], &[P_OUT, S], &[zero]),
        v: device.operand(&[1, 1], &[S, Q_OUT], &[zero]),
        singular_values: device.buffer(&[zero]),
    };
    device
        .fake
        .synchronize_copies()
        .expect("the copies were queued");
    let descriptors = GateSplitDescriptors {
        a: operands.a.descriptor,
        b: operands.b.descriptor,
        gate: operands.gate.descriptor,
        u: operands.u.descriptor,
        v: operands.v.descriptor,
    };
    device
        .fake
        .gate_split_workspace_sizes(device.handle, &descriptors, config, device.workspace)
        .expect("the descriptors and config are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    device
        .fake
        .gate_split(
            device.handle,
            &operands,
            config,
            info,
            device.workspace,
            device.stream,
        )
        .expect("every label is shared or kept on its side");
    let (u, v) = (
        device.read(operands.u.data, 1),
        device.read(operands.v.data, 1),
    );
    assert_close(&[u[0] * v[0]], &[a_value * b_value * gate_value]);
    let report = device
        .fake
        .svd_truncation(device.handle, info)
        .expect("the stream is synchronized");
    assert_eq!(
        (
            report.reduced_extent,
            report.full_extent,
            report.discarded_weight
        ),
        (1, 1, 0.0)
    );
}
