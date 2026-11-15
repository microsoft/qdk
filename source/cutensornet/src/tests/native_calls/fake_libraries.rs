// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the fake libraries' tensor API, which nothing in the crate
//! calls yet: the dense maths a caller's tests will be checked against (a
//! contraction, a gate split, a QR), and the rules the fake holds a caller
//! to (workspaces, streams, objects' handles and lifetimes, a gate split's
//! labels), each refused on the fake as a misuse would fail on a GPU.

use crate::native_calls::{
    OpaqueHandle, Stream,
    complex::Complex64Abi,
    fake_libraries::FakeLibraries,
    memory_calls::{MemoryCalls, WorkspaceSlot},
    session_calls::SessionCalls,
    tensor_calls::{
        ContractionNetwork, GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation,
        TensorCalls, TensorModes, TensorOperand,
    },
};
use num_complex::Complex64;

/// What every tensor call on the fake needs: a handle, a stream and a
/// workspace.
struct Device {
    fake: FakeLibraries,
    handle: OpaqueHandle,
    stream: Stream,
    workspace: OpaqueHandle,
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

    /// Lends `slot` a buffer of `bytes`.
    fn lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) {
        let memory = self
            .fake
            .allocate(usize::try_from(bytes).expect("a size is not negative"))
            .expect("the fake has memory");
        self.fake
            .set_workspace(self.handle, self.workspace, slot, memory, bytes)
            .expect("the buffer holds the bytes lent");
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

    /// As [`sized`](Self::sized), and prepared.
    fn prepared(
        fake: FakeLibraries,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let contraction = Self::sized(fake, inputs, data, output);
        contraction
            .prepare()
            .expect("the workspace is sized for the path");
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

    /// Sizes the workspace for a split under `config`, and lends it what
    /// it asks for.
    fn size(&self, config: OpaqueHandle) {
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
        device
            .fake
            .gate_split_workspace_sizes(device.handle, &descriptors, config, device.workspace)
            .expect("the descriptors and config are live");
        device.lend(WorkspaceSlot::DeviceScratch);
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

/// A QR through the fake of T[i, k, j] (2×2×3) into Q[k, s, i] and R[j, s],
/// whose descriptors list the modes in another order than the input's: the
/// matrix's rows are (i, k), its columns j, and Q and R share `extent`.
/// Returns T, Q and R, each in its own mode order, first mode fastest.
fn qr(extent: i64) -> Result<[Vec<Complex64>; 3], crate::NativeError> {
    qr_on(extent, |device| device.stream)
}

/// [`qr`], on the stream `stream_of` gives for its device.
fn qr_on(
    extent: i64,
    stream_of: impl FnOnce(&Device) -> Stream,
) -> Result<[Vec<Complex64>; 3], crate::NativeError> {
    let [mode_i, mode_k, mode_j, mode_s] = [1, 2, 3, 4];
    let device = Device::new(FakeLibraries::new());
    let shared = usize::try_from(extent).expect("an extent is positive");
    let zero = Complex64::default();
    let input_values = values(12, 0.5);
    let input = device.operand(&[2, 2, 3], &[mode_i, mode_k, mode_j], &input_values);
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
    let stream = stream_of(&device);
    device
        .fake
        .tensor_qr(device.handle, input, q, r, device.workspace, stream)?;
    Ok([
        input_values,
        device.read(q.data, 4 * shared),
        device.read(r.data, 3 * shared),
    ])
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
    contraction.device.lend(WorkspaceSlot::DeviceScratch);
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
/// that lends device scratch alone is caught on the fake.
#[test]
fn a_tensor_call_needs_memory_in_every_slot_that_asks_for_it() {
    let fake = FakeLibraries::new();
    fake.set_other_workspace_slot_bytes(64);
    let contraction = Contraction::dot_product(fake);
    contraction.device.lend(WorkspaceSlot::DeviceScratch);
    contraction.device.lend(WorkspaceSlot::DeviceCache);
    contraction.device.lend(WorkspaceSlot::HostScratch);
    contraction
        .contract()
        .expect_err("the host cache asks for memory and has none");
    contraction.device.lend(WorkspaceSlot::HostCache);
    contraction.contract().expect("every slot is lent");
}

/// A caller that lends a slot less than the size query asked for (an old
/// size, after the workspace was sized for a larger call) is caught on the
/// fake.
#[test]
fn a_tensor_call_needs_at_least_what_each_slot_asks_for() {
    let contraction = Contraction::dot_product(FakeLibraries::new());
    let asked = contraction.device.asked(WorkspaceSlot::DeviceScratch);
    contraction
        .device
        .lend_bytes(WorkspaceSlot::DeviceScratch, asked - 1);
    contraction
        .contract()
        .expect_err("the device scratch is one byte short");
    contraction
        .device
        .lend_bytes(WorkspaceSlot::DeviceScratch, asked);
    contraction
        .contract()
        .expect("the device scratch is lent in full");
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

/// A caller grows a bond up to the extent the split could have kept, as
/// cuTensorNet reports it: the smaller side of θ. A product state's θ has
/// rank 1, yet reports 2; a site with an outer bond of 2 makes θ 4×2 on
/// the left, or 2×4 on the right, and reports 2 either way.
#[test]
fn a_gate_split_reports_the_smaller_side_as_its_full_extent() {
    let rank_one = GateSplit::of(matrix([[1.0, 1.0], [1.0, 1.0]]), &[SvdSetting::PartitionUs])
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

/// cuTensorNet writes U and V before it knows the reduced extent, so a
/// caller that sizes their buffers by an earlier split's bond overruns
/// them on a GPU; the fake refuses it, for U and for V.
#[test]
fn a_gate_split_needs_u_and_v_buffers_of_their_descriptors_full_extents() {
    let zero = Complex64::default();
    let mut split = GateSplit::of(matrix([[1.0, 1.0], [1.0, 1.0]]), &[SvdSetting::PartitionUs]);
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
        &[
            SvdSetting::PartitionUs,
            SvdSetting::DiscardedWeightCutoff(0.0),
        ],
    );
    split.split().expect("θ is not zero");
    split.load(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let capped = split.split().expect("θ is not zero");
    assert_eq!(capped.extent, 1);
    assert!((capped.truncation.discarded_weight - 0.1).abs() < 1e-12);
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

/// cuTensorNet's QR is thin: a caller whose descriptors share more than the
/// smaller side (4 rows, 3 columns) is caught on the fake.
#[test]
fn a_qr_needs_the_shared_extent_to_be_the_smaller_side() {
    qr(4).expect_err("Q and R share 4, the larger side");
}

/// A contraction runs its network whole: a caller whose path the
/// optimizer sliced (cuTensorNet slices one that would not fit its
/// workspace limit) is refused before it can contract one slice alone.
#[test]
fn a_contraction_path_cut_into_slices_is_refused() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle) = (&device.fake, device.handle);
    let network = fake
        .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = fake
        .create_optimizer_info(handle, network)
        .expect("the network is live");
    fake.set_contraction_slices(2);
    fake.optimize_contraction(handle, network, 1 << 20, info)
        .expect_err("the path has two slices");
    fake.set_contraction_slices(1);
    fake.optimize_contraction(handle, network, 1 << 20, info)
        .expect("the path is unsliced");
}

/// A caller that sizes a workspace for, or prepares, a contraction before
/// its network's path is optimized is caught on the fake.
#[test]
fn a_contraction_needs_an_optimized_path() {
    let device = Device::new(FakeLibraries::new());
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

/// cuTensorNet prepares a network in the workspace sized for its path: a
/// caller that prepares an optimized network in a workspace it never sized
/// is caught on the fake.
#[test]
fn a_contraction_is_prepared_in_a_workspace_sized_for_its_path() {
    let device = Device::new(FakeLibraries::new());
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
/// never prepared, or did not prepare again after optimizing it again, is
/// caught on the fake.
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
    contraction
        .contract()
        .expect_err("the network is not prepared");
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
    device.lend(WorkspaceSlot::DeviceScratch);
    contraction
        .contract()
        .expect_err("the network is not prepared for its new path");
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction
        .contract()
        .expect("the network is prepared again");
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
    device.lend(WorkspaceSlot::DeviceScratch);
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
fn a_tensor_call_needs_a_workspace_sized_for_it() {
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

/// cuTensorNet writes a gate split's report on the stream: a caller that
/// reads it before synchronizing is caught on the fake.
#[test]
fn an_svd_info_is_read_once_the_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]), &[SvdSetting::PartitionUs]);
    let device = &split.device;
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

/// cuTensorNet refuses memory for a slot that asks for none: a caller that
/// lends every slot regardless of its size is caught on the fake.
#[test]
fn a_workspace_slot_that_asks_for_nothing_is_lent_nothing() {
    let device = Device::new(FakeLibraries::new());
    let (fake, handle, workspace) = (&device.fake, device.handle, device.workspace);
    let memory = fake.allocate(64).expect("the fake has memory");
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
    contraction.device.lend(WorkspaceSlot::DeviceScratch);
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
    split
        .split()
        .expect_err("the gate's extra label is in no other tensor");
}
