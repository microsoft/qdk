// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the fake libraries' tensor API, which nothing in the crate
//! calls yet: the dense maths a caller's tests will be checked against (a
//! contraction, a gate split, a QR), and the workspace rules the fake holds
//! a caller to.

use crate::native_calls::{
    OpaqueHandle, Stream,
    complex::Complex64Abi,
    fake_libraries::FakeLibraries,
    memory_calls::{MemoryCalls, WorkspaceSlot},
    session_calls::SessionCalls,
    tensor_calls::{
        GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation, TensorCalls,
        TensorModes, TensorOperand,
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

/// A contraction of two tensors through the fake: its device, plan and
/// buffers.
struct Contraction {
    device: Device,
    plan: OpaqueHandle,
    inputs: [OpaqueHandle; 2],
    output: OpaqueHandle,
}

impl Contraction {
    /// Plans the contraction of `inputs`, holding `data`, into `output`, in
    /// a workspace sized for it, lent nothing yet.
    fn planned(
        fake: FakeLibraries,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let device = Device::new(fake);
        let (fake, handle) = (&device.fake, device.handle);
        let network = fake
            .create_contraction_network(handle, inputs, output)
            .expect("the labels are consistent");
        let info = fake
            .create_optimizer_info(handle, network)
            .expect("the network is live");
        fake.optimize_contraction(handle, network, 1 << 20, info)
            .expect("the info is the network's");
        fake.contraction_workspace_sizes(handle, network, info, device.workspace)
            .expect("the path is optimized");
        let plan = fake
            .create_contraction_plan(handle, network, info, device.workspace)
            .expect("the workspace is sized for the path");
        let inputs = [device.buffer(data[0]), device.buffer(data[1])];
        let elements = output.extents.iter().product::<i64>();
        let output = device.buffer(&vec![
            Complex64::default();
            usize::try_from(elements).unwrap_or(0)
        ]);
        fake.synchronize_copies().expect("the copies were queued");
        Self {
            device,
            plan,
            inputs,
            output,
        }
    }

    fn contract(&self) -> Result<(), crate::NativeError> {
        let device = &self.device;
        device.fake.contract(
            device.handle,
            self.plan,
            self.inputs,
            self.output,
            device.workspace,
            device.stream,
        )
    }

    /// The output's first `elements` values, once the stream is done.
    fn output(&self, elements: usize) -> Vec<Complex64> {
        self.device.read(self.output, elements)
    }
}

// The labels of a gate split's modes: A[p, b], B[b, q] and the gate
// G[p', q', p, q] in, U[p', s] and V[s, q'] out.
const P: i32 = 1;
const BOND: i32 = 2;
const Q: i32 = 3;
const P_OUT: i32 = 4;
const Q_OUT: i32 = 5;
const S: i32 = 6;

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

    /// Sizes the workspace for the split, lends it, runs the split and
    /// reads its factors and report.
    fn split(&self) -> Result<Factors, crate::NativeError> {
        let Self {
            device,
            config,
            info,
            operands,
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
            .gate_split_workspace_sizes(device.handle, &descriptors, *config, device.workspace)
            .expect("the descriptors and config are live");
        device.lend(WorkspaceSlot::DeviceScratch);
        device.fake.gate_split(
            device.handle,
            operands,
            *config,
            *info,
            device.workspace,
            device.stream,
        )?;
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
    device
        .fake
        .tensor_qr(device.handle, input, q, r, device.workspace, device.stream)?;
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
    let contraction = Contraction::planned(
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
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::planned(
        fake,
        [
            TensorModes {
                extents: &[2],
                modes: &[1],
            },
            TensorModes {
                extents: &[2],
                modes: &[1],
            },
        ],
        [&ones, &ones],
        TensorModes {
            extents: &[],
            modes: &[],
        },
    );
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
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::planned(
        FakeLibraries::new(),
        [
            TensorModes {
                extents: &[2],
                modes: &[1],
            },
            TensorModes {
                extents: &[2],
                modes: &[1],
            },
        ],
        [&ones, &ones],
        TensorModes {
            extents: &[],
            modes: &[],
        },
    );
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

/// A product state's θ has rank 1: the split keeps one value, and still
/// reports the bond it could have had, the smaller side, as cuTensorNet
/// does.
#[test]
fn a_gate_splits_full_extent_is_the_smaller_side_whatever_the_rank() {
    let factors = GateSplit::of(matrix([[1.0, 1.0], [1.0, 1.0]]), &[SvdSetting::PartitionUs])
        .split()
        .expect("θ is not zero");
    assert_eq!(factors.extent, 1, "θ has rank 1");
    assert_eq!(factors.truncation.full_extent, 2);
}

/// cuTensorNet writes the reduced extent into U's and V's descriptors, so a
/// caller that reuses them for the next gate has that gate's bond capped
/// there, until it resets them.
#[test]
fn a_gate_split_caps_the_bond_at_its_descriptors_extent_which_it_lowers() {
    let split = GateSplit::of(matrix([[1.0, 1.0], [1.0, 1.0]]), &[SvdSetting::PartitionUs]);
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
