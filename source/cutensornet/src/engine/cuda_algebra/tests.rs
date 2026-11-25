// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the CUDA algebra on the fake libraries
//! (`src/engine/native_calls/fake_libraries.rs`): that it meets
//! `SiteAlgebra`'s contract (`canonical_mps`'s conformance kit), what it
//! opens and releases, the tensors it refuses, and what it does when a
//! call fails.
//!
//! - The scope of one call, its gate splits' reports, its contraction
//!   networks and its scratch are tested beside their modules
//!   (`call_scope/tests.rs`, `gate_split/tests.rs`, `contraction/tests.rs`,
//!   `scratch/tests.rs`), which test the scratch, the scope's steps and the
//!   reports' check directly. Its cleanups are in `tests/cleanups.rs`.
//! - The rules of `qr_move.rs`, `device_tensor.rs`, `extents.rs`,
//!   `resources.rs`, `algebra_session.rs` and `device_session.rs` have no
//!   file of their own. They are tested here, through the algebra's
//!   methods.
//! - What a read returns is the kit's rule. This algebra's read copies to
//!   the host, unlike the CPU algebra's, so no test pins a borrow.
//!
//! The kit and the engine's circuits also run on a GPU
//! (`tests/cuda_mps_engine_on_the_gpu.rs`).

mod cleanups;

use crate::engine::CudaAlgebraError;
use crate::engine::cuda_algebra::{CudaAlgebra, DeviceTensor};
use crate::engine::native_calls::{
    NativeCalls,
    fake_libraries::{FakeLibraries, WORKSPACE_BYTES},
    memory_calls::WorkspaceSlot,
};
use crate::engine::tests::{algebra_on, algebra_on_the_fake as algebra};
use canonical_mps::site_algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
    conformance,
};
use num_complex::Complex64;
use std::sync::Arc;
use tensornet::sampling::SplitMix64;

pub(super) fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

pub(super) fn upload(algebra: &CudaAlgebra, values: &[Complex64]) -> DeviceTensor {
    algebra
        .upload(values)
        .unwrap_or_else(|error| panic!("the upload succeeds: {error}"))
}

pub(super) fn read(algebra: &CudaAlgebra, tensor: &DeviceTensor, len: usize) -> Vec<Complex64> {
    algebra
        .read(tensor, len)
        .unwrap_or_else(|error| panic!("the read succeeds: {error}"))
        .into_owned()
}

/// A split of `a` and `b` read as two sites of one qubit with a bond of
/// one, under a random gate, without truncation: valid inputs for a call
/// whose outcome a test does not read.
pub(super) fn one_qubit_split<'v>(a: &'v [Complex64], b: &'v [Complex64]) -> Split<'v> {
    Split::untruncated(a, b, [1, 1, 1])
}

/// A gate split of `a` and `b`, of extents `[l, 2, m]` and `[m, 2, r]`,
/// with `gate`, `cutoff`, `max_extent` and `centre`.
pub(super) struct Split<'v> {
    pub(super) a: &'v [Complex64],
    pub(super) b: &'v [Complex64],
    pub(super) l: usize,
    pub(super) m: usize,
    pub(super) r: usize,
    pub(super) gate: [Complex64; 16],
    pub(super) cutoff: f64,
    pub(super) max_extent: usize,
    pub(super) centre: CentreSide,
}

impl<'v> Split<'v> {
    /// A split of `left` and `right`, of extents `[l, 2, m]` and
    /// `[m, 2, r]`, under a random gate, without truncation, the centre
    /// left on the left site.
    pub(super) fn untruncated(
        left: &'v [Complex64],
        right: &'v [Complex64],
        [l, m, r]: [usize; 3],
    ) -> Self {
        Split {
            a: left,
            b: right,
            l,
            m,
            r,
            gate: random_gate(7),
            cutoff: 0.0,
            max_extent: usize::MAX,
            centre: CentreSide::Left,
        }
    }

    /// The split on `algebra`, its pair uploaded first.
    pub(super) fn on(
        &self,
        algebra: &CudaAlgebra,
    ) -> Result<GateSplitOutcome<DeviceTensor>, CudaAlgebraError> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        algebra.gate_split(&self.call(&a, &b))
    }

    /// The call on the tensors `a` and `b`.
    pub(super) fn call<'t>(
        &self,
        a: &'t DeviceTensor,
        b: &'t DeviceTensor,
    ) -> GateSplitCall<'t, DeviceTensor> {
        GateSplitCall {
            left: a,
            right: b,
            left_extent: self.l,
            bond: self.m,
            right_extent: self.r,
            gate: self.gate,
            max_extent: self.max_extent,
            cutoff: self.cutoff,
            centre: self.centre,
        }
    }
}

/// A move of the centre across the bond of `a` and `b`, of extents
/// `[l, 2, m]` and `[m, 2, r]`, with an optional one-site `gate`.
pub(super) struct Move<'v> {
    pub(super) a: &'v [Complex64],
    pub(super) b: &'v [Complex64],
    pub(super) l: usize,
    pub(super) m: usize,
    pub(super) r: usize,
    pub(super) gate: Option<[Complex64; 4]>,
    pub(super) centre: CentreSide,
}

impl<'v> Move<'v> {
    /// A move of the centre to the right site, across the bond of `left`
    /// and `right`, of extents `[l, 2, m]` and `[m, 2, r]`, without a gate.
    pub(super) fn to_the_right(
        left: &'v [Complex64],
        right: &'v [Complex64],
        [l, m, r]: [usize; 3],
    ) -> Self {
        Move {
            a: left,
            b: right,
            l,
            m,
            r,
            gate: None,
            centre: CentreSide::Right,
        }
    }

    /// The move on `algebra`, its pair uploaded first.
    pub(super) fn on(
        &self,
        algebra: &CudaAlgebra,
    ) -> Result<QrMoveOutcome<DeviceTensor>, CudaAlgebraError> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        algebra.qr_move(&self.call(&a, &b))
    }

    /// The call on the tensors `a` and `b`.
    pub(super) fn call<'t>(
        &self,
        a: &'t DeviceTensor,
        b: &'t DeviceTensor,
    ) -> QrMoveCall<'t, DeviceTensor> {
        QrMoveCall {
            left: a,
            right: b,
            left_extent: self.l,
            bond: self.m,
            right_extent: self.r,
            gate: self.gate,
            centre: self.centre,
        }
    }
}

/// A move of the centre to the right without a gate, of the `shape`-th of
/// a sequence of distinct shapes, on random sites of extents `[16, 2, m]`
/// and `[m, 2, r]`: it contracts one network, of extents `[m, m]` and
/// `[m, 2, r]` (as `m <= 2 * 16`).
pub(super) fn move_of_a_new_shape(
    algebra: &CudaAlgebra,
    shape: usize,
) -> Result<(), CudaAlgebraError> {
    let (m, r) = (shape / 17 + 1, shape % 17 + 1);
    assert!(m <= 2 * 16, "shape {shape}: a move contracting one network");
    let (left, right) = random_pair(16, m, r, 29);
    Move::to_the_right(&left, &right, [16, m, r])
        .on(algebra)
        .map(drop)
}

/// `len` complex numbers whose parts lie in `[-0.5, 0.5)`, from
/// `tensornet`'s [`SplitMix64`] seeded by `seed`.
pub(super) fn random(len: usize, seed: u64) -> Vec<Complex64> {
    let mut stream = SplitMix64::new(seed);
    let mut next = move || stream.next_uniform() - 0.5;
    (0..len)
        .map(|_| {
            let re = next();
            Complex64::new(re, next())
        })
        .collect()
}

/// A pair `A`, `B` of extents `[l, 2, m]` and `[m, 2, r]`, random from
/// `seed`.
pub(super) fn random_pair(
    l: usize,
    m: usize,
    r: usize,
    seed: u64,
) -> (Vec<Complex64>, Vec<Complex64>) {
    (random(l * 2 * m, seed), random(m * 2 * r, seed + 1))
}

/// A random two-site gate, row-major.
fn random_gate(seed: u64) -> [Complex64; 16] {
    random(16, seed).try_into().expect("16 entries")
}

/// A random one-site gate, row-major.
pub(super) fn one_qubit_gate(seed: u64) -> [Complex64; 4] {
    random(4, seed).try_into().expect("4 entries")
}

/// The algebra meets `SiteAlgebra`'s contract: every case of
/// `canonical_mps`'s conformance kit passes, on the fake libraries, whose
/// SVD and QR are `dense_linalg`'s, as the CPU algebra's are. Once the
/// algebra is dropped, every native object the kit's calls made is
/// released.
#[test]
fn the_cuda_algebra_conforms() {
    let (algebra, libraries) = algebra();
    let failures = conformance::check(&algebra);
    assert!(failures.is_empty(), "{failures:#?}");
    drop(algebra);
    assert_eq!(libraries.live_objects(), 0, "the kit's objects leaked");
}

/// An algebra whose opening fails once the device is selected, at any of
/// the objects it makes (the stream, the cuTensorNet handle, the workspace
/// descriptor, the gate's buffer), reports that failure and releases what
/// it had made.
#[test]
fn a_failed_opening_releases_what_it_made() {
    for operation in [
        "create_stream",
        "create_handle",
        "create_workspace",
        "allocate",
    ] {
        let libraries = Arc::new(FakeLibraries::new());
        libraries.fail(operation, 1);
        let native_calls: Arc<dyn NativeCalls> = libraries.clone();
        let opened = CudaAlgebra::with_native_calls(native_calls);
        assert_eq!(failed_at(&opened), Some(operation), "{opened:?}");
        assert_eq!(libraries.live_objects(), 0, "{operation}");
    }
}

/// The operation whose native call failed, if `result` is such a failure.
pub(super) fn failed_at<T>(result: &Result<T, CudaAlgebraError>) -> Option<&'static str> {
    match result {
        Err(CudaAlgebraError::NativeCallFailed { operation, .. }) => Some(*operation),
        _ => None,
    }
}

/// A host whose CUDA Runtime finds no device has no device 0: opening an
/// algebra there is `NoDevice`, refused before any device is selected or
/// any object made.
#[test]
fn a_host_without_a_cuda_device_is_refused() {
    let libraries = Arc::new(FakeLibraries::new());
    libraries.set_device_count(0);
    let native_calls: Arc<dyn NativeCalls> = libraries.clone();
    let opened = CudaAlgebra::with_native_calls(native_calls);
    assert!(
        matches!(opened, Err(CudaAlgebraError::NoDevice)),
        "{opened:?}"
    );
    assert_eq!(libraries.events(), ["device_count"], "no device selected");
}

/// A call that would read past a tensor's elements is a defect of its
/// caller, refused before the device reads memory the tensor does not
/// own, and before the call makes any native object, whichever method
/// makes it and whichever of a pair's tensors is short. The refusal names
/// the site tensor and both lengths, so that its reader finds the bond
/// dimensions at fault. A read of no element reads nothing: it returns no
/// value and copies nothing.
#[test]
fn a_call_reading_past_a_tensors_elements_is_refused() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 2, 1, 3);
    let (small, other) = (upload(&algebra, &a[..2]), upload(&algebra, &b));
    let (left, small_right) = (upload(&algebra, &a), upload(&algebra, &b[..2]));
    let refusal = ExpectedRefusal::before_any_native_object(&libraries, is_an_invalid_call);
    refusal.assert_refuses("read", || algebra.read(&small, 3).map(drop));
    refusal.assert_refuses("duplicate", || algebra.duplicate(&small, 3).map(drop));
    let refused = algebra
        .read(&small, 3)
        .expect_err("the tensor holds 2 values")
        .to_string();
    assert!(
        ["site tensor", "3", "2"]
            .iter()
            .all(|named| refused.contains(named)),
        "the refusal names the site tensor and both lengths: {refused}"
    );
    for (side, left, right) in [("left", &small, &other), ("right", &left, &small_right)] {
        for (method, call) in each_tensor_call(&algebra, [1, 2, 1], left, right) {
            refusal.assert_refuses(&format!("{method}, {side} short"), call);
        }
    }
    let before = libraries.events().len();
    let nothing = algebra.read(&small, 0).expect("a read of no element");
    assert!(nothing.is_empty(), "{nothing:?}");
    let events = libraries.events();
    assert!(
        !events[before..].contains(&"copy_to_host"),
        "nothing copied, not {:?}",
        &events[before..]
    );
}

/// A call whose extents multiply past `usize`, or pass the `i64` the
/// library takes, names a tensor no buffer could hold: a defect of its
/// caller, refused with an error, not a panic, before the call makes any
/// native object, whichever method makes it and whichever of a pair's
/// extents is at fault. A right extent past `i64::MAX` beside a bond of 0
/// gives tensors of no element, so only the library's extent type refuses
/// it: a move of the centre to the left describes that site first.
#[test]
fn a_call_whose_extents_overflow_is_refused() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 3);
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    let huge = usize::MAX / 2 + 1;
    let refusal = ExpectedRefusal::before_any_native_object(&libraries, is_an_overflow);
    for (side, l, r) in [("left", huge, 1), ("right", 1, huge)] {
        for (method, call) in each_tensor_call(&algebra, [l, 1, r], &left, &right) {
            refusal.assert_refuses(&format!("{method}, {side} extent"), call);
        }
    }
    let no_element = Move {
        a: &a,
        b: &b,
        l: 1,
        m: 0,
        r: huge,
        gate: None,
        centre: CentreSide::Left,
    };
    refusal.assert_refuses("move beside a bond of 0", || {
        algebra.qr_move(&no_element.call(&left, &right)).map(drop)
    });
}

/// `SiteAlgebra`'s contract never asks for an empty tensor (an upload holds
/// at least one element, and a duplicate copies at least one): a caller
/// that breaks it, uploading nothing or duplicating no element, gets an
/// invalid call, refused before any native object is made.
#[test]
fn a_call_for_an_empty_tensor_is_refused() {
    let (algebra, libraries) = algebra();
    let tensor = upload(&algebra, &random(2, 7));
    let refusal = ExpectedRefusal::before_any_native_object(&libraries, is_an_invalid_call);
    refusal.assert_refuses("upload", || algebra.upload(&[]).map(drop));
    refusal.assert_refuses("duplicate", || algebra.duplicate(&tensor, 0).map(drop));
}

/// A tensor is freed through the session of the algebra that made it,
/// which knows nothing of another algebra's device work: so another
/// algebra, whose failed call could not keep the tensor from being freed
/// while that call's work may still read it, refuses it before any native
/// call, whichever method is given it and whichever of a pair's tensors it
/// is, even when it reads no element. Nothing is leaked.
#[test]
fn a_tensor_of_another_algebra_is_refused() {
    let libraries = Arc::new(FakeLibraries::new());
    let (owner, algebra) = (algebra_on(&libraries), algebra_on(&libraries));
    let (a, b) = random_pair(1, 1, 1, 12);
    let (foreign_a, foreign_b) = (upload(&owner, &a), upload(&owner, &b));
    let (own_a, own_b) = (upload(&algebra, &a), upload(&algebra, &b));
    let refusal = ExpectedRefusal::before_any_native_call(&libraries, is_foreign);
    refusal.assert_refuses("read", || algebra.read(&foreign_a, 2).map(drop));
    refusal.assert_refuses("duplicate", || algebra.duplicate(&foreign_a, 2).map(drop));
    refusal.assert_refuses("read of no element", || {
        algebra.read(&foreign_a, 0).map(drop)
    });
    refusal.assert_refuses("duplicate of no element", || {
        algebra.duplicate(&foreign_a, 0).map(drop)
    });
    for (side, left, right) in [("left", &foreign_a, &own_b), ("right", &own_a, &foreign_b)] {
        for (method, call) in each_tensor_call(&algebra, [1, 1, 1], left, right) {
            refusal.assert_refuses(&format!("{method}, {side} foreign"), call);
        }
    }
    drop((foreign_a, foreign_b, own_a, own_b, owner, algebra));
    assert_eq!(libraries.live_objects(), 0);
}

/// The refusal a test expects of each call it runs: an error `is_expected`
/// accepts, returned before the call makes what `before` names.
struct ExpectedRefusal<'l> {
    libraries: &'l FakeLibraries,
    is_expected: fn(&CudaAlgebraError) -> bool,
    before: RefusedBefore,
}

/// What a refused call returns before making.
enum RefusedBefore {
    /// No `create_*` call and no allocation; other native calls may run.
    AnyNativeObject,
    /// No native call at all.
    AnyNativeCall,
}

impl<'l> ExpectedRefusal<'l> {
    /// A refusal returned before the call makes any native object.
    fn before_any_native_object(
        libraries: &'l FakeLibraries,
        is_expected: fn(&CudaAlgebraError) -> bool,
    ) -> Self {
        ExpectedRefusal {
            libraries,
            is_expected,
            before: RefusedBefore::AnyNativeObject,
        }
    }

    /// A refusal returned before the call makes any native call.
    fn before_any_native_call(
        libraries: &'l FakeLibraries,
        is_expected: fn(&CudaAlgebraError) -> bool,
    ) -> Self {
        ExpectedRefusal {
            libraries,
            is_expected,
            before: RefusedBefore::AnyNativeCall,
        }
    }

    /// Runs `call` and asserts that it is refused so, naming `method` if
    /// not.
    fn assert_refuses(&self, method: &str, call: impl FnOnce() -> Result<(), CudaAlgebraError>) {
        let before = self.libraries.events().len();
        let refused = call();
        let events = &self.libraries.events()[before..];
        match self.before {
            RefusedBefore::AnyNativeObject => {
                let made = events
                    .iter()
                    .filter(|event| event.starts_with("create_") || **event == "allocate")
                    .count();
                assert_eq!(made, 0, "{method}: no native object made");
            }
            RefusedBefore::AnyNativeCall => {
                assert!(
                    events.is_empty(),
                    "{method}: no native call, not {events:?}"
                );
            }
        }
        assert!(
            refused.as_ref().err().is_some_and(self.is_expected),
            "{method}: {refused:?}"
        );
    }
}

fn is_an_invalid_call(error: &CudaAlgebraError) -> bool {
    matches!(error, CudaAlgebraError::InvalidNativeCall { .. })
}

fn is_an_overflow(error: &CudaAlgebraError) -> bool {
    matches!(error, CudaAlgebraError::ResourceSizeOverflow { .. })
}

fn is_foreign(error: &CudaAlgebraError) -> bool {
    matches!(error, CudaAlgebraError::ForeignTensor)
}

/// A call a test runs for its error alone.
type TensorCall<'t> = Box<dyn Fn() -> Result<(), CudaAlgebraError> + 't>;

/// Each of `algebra`'s calls on a pair, named: an untruncated gate split
/// and a move of the centre to the right without a gate, of `left` and
/// `right` read as of `extents` `[l, m, r]` (`[l, 2, m]` and `[m, 2, r]`).
fn each_tensor_call<'t>(
    algebra: &'t CudaAlgebra,
    extents: [usize; 3],
    left: &'t DeviceTensor,
    right: &'t DeviceTensor,
) -> [(&'static str, TensorCall<'t>); 2] {
    // `call` takes the tensors given: the values, which only `on` uploads,
    // are left empty.
    let split = move || {
        let split = Split::untruncated(&[], &[], extents);
        algebra.gate_split(&split.call(left, right)).map(drop)
    };
    let moved = move || {
        let moved = Move::to_the_right(&[], &[], extents);
        algebra.qr_move(&moved.call(left, right)).map(drop)
    };
    [
        ("gate split", Box::new(split)),
        ("move of the centre", Box::new(moved)),
    ]
}

/// A duplicate's output is ready for the next call at once: its copy runs
/// on CUDA's legacy default stream, which the algebra's stream does not
/// wait for, so the duplicate waits for it before it returns. The fake
/// refuses a tensor call while a copy may still run, here a move of the
/// centre on the duplicate, right after it. (The conformance kit misses
/// this case: it uploads a tensor after each duplicate, and the upload's
/// wait covers the duplicate's copy.)
#[test]
fn a_duplicate_is_ready_for_the_next_call() {
    let (algebra, _libraries) = algebra();
    let (a, b) = random_pair(2, 2, 2, 4);
    let moved = Move::to_the_right(&a, &b, [2, 2, 2]);
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    let copy = algebra.duplicate(&left, a.len()).expect("the duplicate");
    algebra
        .qr_move(&moved.call(&copy, &right))
        .expect("the duplicate's copy is done");
}

/// A move of the centre whose contraction asks for more scratch than its
/// QR was lent, either side, with a gate or not: the move grows the
/// scratch between its two steps, so it must free the QR's smaller buffer
/// only once the QR has finished with it. The fake refuses to free a
/// buffer a stream may still use, so each move succeeds only if it waits.
#[test]
fn a_move_frees_its_qrs_scratch_only_once_the_qr_is_done() {
    let (a, b) = random_pair(1, 1, 1, 41);
    for centre in [CentreSide::Left, CentreSide::Right] {
        for gate in [None, Some(one_qubit_gate(42))] {
            let case = format!("{centre:?}, gate {}", gate.is_some());
            let (algebra, libraries) = algebra();
            libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, 64);
            libraries.set_workspace_slot_bytes_after_the_next_qr(
                WorkspaceSlot::DeviceScratch,
                WORKSPACE_BYTES,
            );
            let moved = Move {
                a: &a,
                b: &b,
                l: 1,
                m: 1,
                r: 1,
                gate,
                centre,
            };
            let grown = moved.on(&algebra).map(drop);
            assert!(grown.is_ok(), "{case}: {grown:?}");
            assert_eq!(
                libraries.scratch_buffers(),
                2,
                "{case}: the scratch grew within the move"
            );
        }
    }
}

/// Dropping an algebra and its tensors, in either order, releases every
/// native object: the workspace and its scratch, the gate's and the
/// singular values' buffers (and the smaller one a growth replaced), the
/// prepared contractions, the tensors, and last, once the device is
/// selected and the stream's work done, the cuTensorNet handle, then the
/// stream its work runs on (an order the fake libraries do not enforce, so
/// checked here). A tensor may outlive its algebra, as a state may outlive
/// its engine: it keeps the device session open until it is dropped.
#[test]
fn dropping_the_algebra_and_its_tensors_in_either_order_releases_every_native_object() {
    for algebra_first in [true, false] {
        let (algebra, libraries) = algebra();
        let tensors = tensors_holding_every_resource(&algebra);
        if algebra_first {
            drop(algebra);
            assert_eq!(
                libraries.live_handles(),
                1,
                "the tensors keep the session open"
            );
            drop(tensors);
        } else {
            drop(tensors);
            drop(algebra);
        }
        assert_eq!(
            libraries.live_objects(),
            0,
            "algebra first: {algebra_first}"
        );
        let events = libraries.events();
        assert_eq!(
            events[events.len() - 4..],
            [
                "set_device",
                "synchronize_stream",
                "destroy_handle",
                "destroy_stream"
            ],
            "the close selects the device and waits for the stream; the \
             handle's work runs on the stream: the handle goes first"
        );
    }
}

/// Tensors of `algebra` that leave it holding every resource it keeps: two
/// uploads, a gate split's after a smaller one (so that the singular
/// values' buffer grows), and a gated move of the centre's (so that a
/// contraction network is prepared).
fn tensors_holding_every_resource(algebra: &CudaAlgebra) -> Vec<DeviceTensor> {
    let (a, b) = random_pair(2, 3, 2, 5);
    let split = Split::untruncated(&a, &b, [2, 3, 2]);
    let (left, right) = (upload(algebra, &a), upload(algebra, &b));
    let (small_a, small_b) = random_pair(1, 1, 1, 4);
    one_qubit_split(&small_a, &small_b)
        .on(algebra)
        .expect("the smaller split");
    let outcome = algebra
        .gate_split(&split.call(&left, &right))
        .expect("the split");
    let moved = Move {
        gate: Some([c(0.0, 1.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)]),
        ..Move::to_the_right(&a, &b, [2, 3, 2])
    };
    let moved = algebra
        .qr_move(&moved.call(&left, &right))
        .expect("the move");
    vec![
        left,
        right,
        outcome.left,
        outcome.right,
        moved.left,
        moved.right,
    ]
}

/// CUDA's current device is per thread, and the engine may call the
/// algebra on a thread that last used another device: the opening, every
/// method, and the drop of a tensor and of the algebra, select the
/// algebra's device before any other native call (the opening, once it
/// has counted the devices).
#[test]
fn every_call_selects_the_algebras_device_first() {
    let (algebra, libraries) = algebra();
    assert_eq!(
        libraries.events()[..2],
        ["device_count", "set_device"],
        "the opening"
    );
    let (a, b) = random_pair(1, 1, 1, 12);
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    let split = one_qubit_split(&a, &b);
    let moved = Move::to_the_right(&a, &b, [1, 1, 1]);
    let calls: [(&str, &dyn Fn()); 5] = [
        ("upload", &|| drop(upload(&algebra, &a))),
        ("read", &|| drop(read(&algebra, &left, a.len()))),
        ("duplicate", &|| {
            drop(algebra.duplicate(&left, a.len()).expect("the duplicate"));
        }),
        ("gate split", &|| {
            drop(
                algebra
                    .gate_split(&split.call(&left, &right))
                    .expect("the split"),
            );
        }),
        ("move of the centre", &|| {
            drop(
                algebra
                    .qr_move(&moved.call(&left, &right))
                    .expect("the move"),
            );
        }),
    ];
    for (method, call) in calls {
        let first = first_native_call(&libraries, call);
        assert_eq!(first, Some("set_device"), "{method}");
    }
    let first = first_native_call(&libraries, move || drop(left));
    assert_eq!(first, Some("set_device"), "the drop of a tensor");
    let first = first_native_call(&libraries, move || drop(algebra));
    assert_eq!(first, Some("set_device"), "the drop of the algebra");
    let (poisoned, libraries, _pair, _failed) = a_failed_split(true);
    let first = first_native_call(&libraries, move || drop(poisoned));
    assert_eq!(first, Some("set_device"), "the drop of a poisoned algebra");
}

/// A free acts on the current device, which is per thread: a tensor or an
/// algebra dropped while the algebra's device cannot be selected frees
/// nothing, so that nothing is freed on another device, and its memory is
/// leaked. Cases: the tensor dropped, or the algebra, after a split grew
/// the algebra's buffers; or the algebra dropped first and then its last
/// tensor, whose drop also closes the session, which still destroys its
/// handle and stream.
#[test]
fn a_drop_that_cannot_select_the_device_frees_nothing() {
    for dropped in [
        Dropped::Tensor,
        Dropped::Algebra,
        Dropped::LastTensorAfterTheAlgebra,
    ] {
        let (algebra, libraries) = algebra();
        let (a, b) = random_pair(1, 1, 1, 15);
        drop(one_qubit_split(&a, &b).on(&algebra).expect("the split"));
        let tensor = upload(&algebra, &a);
        let algebra = match dropped {
            Dropped::LastTensorAfterTheAlgebra => {
                drop(algebra);
                None
            }
            Dropped::Tensor | Dropped::Algebra => Some(algebra),
        };
        let (before, live) = (libraries.events().len(), libraries.live_objects());
        let selections = libraries.count("set_device");
        libraries.fail("set_device", selections + 1);
        match dropped {
            Dropped::Algebra => drop(algebra),
            Dropped::Tensor => drop(tensor),
            Dropped::LastTensorAfterTheAlgebra => {
                libraries.fail("set_device", selections + 2);
                drop(tensor);
            }
        }
        let events = &libraries.events()[before..];
        let frees = events.iter().filter(|&&event| event == "free").count();
        assert_eq!(frees, 0, "{dropped:?}: nothing freed: {events:?}");
        if dropped == Dropped::LastTensorAfterTheAlgebra {
            assert!(
                events.contains(&"destroy_handle") && events.contains(&"destroy_stream"),
                "{dropped:?}: the session's handle and stream destroyed: {events:?}"
            );
            assert_eq!(libraries.live_handles(), 0, "{dropped:?}: no handle left");
        } else {
            let destroys = events.iter().filter(|event| event.starts_with("destroy_"));
            assert_eq!(destroys.count(), 0, "{dropped:?}: nothing destroyed");
            assert_eq!(libraries.live_objects(), live, "{dropped:?}: leaked");
        }
    }
}

/// What `a_drop_that_cannot_select_the_device_frees_nothing` drops.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Dropped {
    Tensor,
    Algebra,
    LastTensorAfterTheAlgebra,
}

/// The first native call `run` makes, if any.
fn first_native_call(libraries: &FakeLibraries, run: impl FnOnce()) -> Option<&'static str> {
    let before = libraries.events().len();
    run();
    libraries.events().get(before).copied()
}

/// A failed gate split: its wait for its device work fails, so that work
/// may still run, and, when `wait_fails`, the failed call's own wait for
/// the device fails too. Returns the algebra, the pair it split, which the
/// device may still read, and the split's failure.
fn a_failed_split(
    wait_fails: bool,
) -> (
    CudaAlgebra,
    Arc<FakeLibraries>,
    [DeviceTensor; 2],
    CudaAlgebraError,
) {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 9);
    let pair = [upload(&algebra, &a), upload(&algebra, &b)];
    // The split's first wait is for its factorization; the next is the
    // failed call's.
    let waits = libraries.count("synchronize_stream");
    libraries.fail("synchronize_stream", waits + 1);
    if wait_fails {
        libraries.fail("synchronize_stream", waits + 2);
    }
    let failed = algebra
        .gate_split(&one_qubit_split(&a, &b).call(&pair[0], &pair[1]))
        .expect_err("a failed wait");
    (algebra, libraries, pair, failed)
}

/// The failure of the `synchronize_stream` the fake was told to fail.
fn is_a_failed_wait(error: &CudaAlgebraError) -> bool {
    matches!(
        error,
        CudaAlgebraError::NativeCallFailed {
            operation: "synchronize_stream",
            ..
        }
    )
}

/// A failed call waits for the device before it releases its buffers. When
/// that wait succeeds, the call has released what it made, so the algebra
/// stays usable. When it fails, nothing tells when the device is done with
/// them, so every later call is refused as poisoned, whichever method makes
/// it and whatever its arguments (a tensor too short for the call, another
/// algebra's, or no element at all, included), the wait's failure kept
/// beside the call's.
#[test]
fn a_failed_call_poisons_the_algebra_only_when_it_cannot_wait_for_the_device() {
    for wait_fails in [false, true] {
        let (algebra, _libraries, pair, failed) = a_failed_split(wait_fails);
        let values = random(4, 10);
        if !wait_fails {
            assert!(is_a_failed_wait(&failed), "{failed:?}");
            let tensor = upload(&algebra, &values);
            assert_eq!(read(&algebra, &tensor, 4), values, "a usable algebra");
            continue;
        }
        assert_failed_call_poisons(&algebra, &pair, &values, failed);
    }
}

/// The poisoned row's checks: both failures kept, and every later call
/// refused, `values` (four elements) asking more than `pair` holds.
fn assert_failed_call_poisons(
    algebra: &CudaAlgebra,
    pair: &[DeviceTensor; 2],
    values: &[Complex64],
    failed: CudaAlgebraError,
) {
    let CudaAlgebraError::ExecutionAndCleanupFailed { execution, cleanup } = failed else {
        panic!("both failures, not {failed:?}");
    };
    assert!(is_a_failed_wait(&execution), "{execution:?}");
    assert!(is_a_failed_wait(&cleanup), "{cleanup:?}");
    // The pair holds two elements each; these calls ask for more.
    let is_poisoned = |method: &str, call: &dyn Fn() -> Result<(), CudaAlgebraError>| {
        let next = call();
        assert!(
            matches!(next, Err(CudaAlgebraError::Poisoned { .. })),
            "{method}: {next:?}"
        );
    };
    is_poisoned("upload", &|| algebra.upload(values).map(drop));
    is_poisoned("upload of no element", &|| algebra.upload(&[]).map(drop));
    is_poisoned("read of no element", &|| {
        algebra.read(&pair[0], 0).map(drop)
    });
    is_poisoned("read, short", &|| algebra.read(&pair[0], 3).map(drop));
    is_poisoned("duplicate, short", &|| {
        algebra.duplicate(&pair[0], 3).map(drop)
    });
    for (method, call) in each_tensor_call(algebra, [1, 2, 1], &pair[0], &pair[1]) {
        is_poisoned(&format!("{method}, short"), &call);
    }
    let (other, _other_libraries) = self::algebra();
    let foreign = upload(&other, values);
    is_poisoned("read, another algebra's", &|| {
        algebra.read(&foreign, 4).map(drop)
    });
    is_poisoned("gate split, another algebra's on the right", &|| {
        algebra
            .gate_split(&one_qubit_split(values, values).call(&pair[0], &foreign))
            .map(drop)
    });
}

/// After a failed call that could not wait for the device, nothing tells when
/// the device is done with that call's buffers, its inputs or the algebra's
/// memory: none is freed until a later wait for the device (its stream and
/// its copies) succeeds (when the algebra is dropped, or else when its last
/// tensor is), and if none does, they are leaked; once one succeeds, a tensor
/// dropped is freed at once. The fake refuses to free a buffer while a
/// stream's work or a copy may still use it. Cases: the call whose wait
/// failed (a split's, on the stream, or a duplicate's or an upload's, of its
/// copy), whether the engine drops the failed call's inputs before the
/// algebra (as after a failed gate, when it drops the state and keeps the
/// algebra), and how many of the same waits that follow fail. A duplicate
/// has one input, whose drop after the algebra's is also the last tensor's,
/// so its cases drop it first; an upload has none. The session's close calls
/// the destroys of its handle and stream whatever its wait returns, as CUDA
/// defers a stream's release until its work completes.
#[test]
fn a_poisoned_algebra_frees_nothing_until_a_wait_for_the_device_succeeds() {
    for (failed_call, inputs_dropped_first, failed_waits) in [
        (FailedCall::Split, false, 0),
        (FailedCall::Split, true, 0),
        (FailedCall::Split, false, 1),
        (FailedCall::Split, true, 1),
        (FailedCall::Split, true, 2),
        (FailedCall::Duplicate, true, 0),
        (FailedCall::Duplicate, true, 1),
        (FailedCall::Duplicate, true, 2),
        (FailedCall::Upload, true, 1),
    ] {
        let wait = failed_call.wait();
        let case = format!(
            "{failed_call:?}'s {wait} failed, inputs dropped first: \
             {inputs_dropped_first}, failed waits: {failed_waits}"
        );
        let (algebra, libraries, inputs) = a_poisoned_algebra(failed_call);
        let poisoned_at = libraries.events().len();
        let waits = libraries.count(wait);
        for failed in 1..=failed_waits {
            libraries.fail(wait, waits + failed);
        }
        if inputs_dropped_first {
            drop(inputs);
            drop(algebra);
        } else {
            drop(algebra);
            let mut inputs = inputs.into_iter();
            let live = libraries.live_objects();
            drop(inputs.next().expect("the failed call's input"));
            let freed_at_once = usize::from(failed_waits == 0);
            assert_eq!(
                libraries.live_objects(),
                live - freed_at_once,
                "{case}: a tensor dropped is freed at once only after a wait succeeded"
            );
            drop(inputs);
        }
        let events = libraries.events();
        let before_a_wait = events[poisoned_at..]
            .iter()
            .scan(0, |waits, &event| {
                *waits += usize::from(event == wait);
                (*waits <= failed_waits).then_some(event)
            })
            .filter(|&event| event == "free" || event.starts_with("destroy_"))
            .filter(|&event| !SESSION_DESTROYS.contains(&event))
            .count();
        assert_eq!(
            before_a_wait, 0,
            "{case}: nothing freed before a wait succeeds"
        );
        if failed_waits < 2 {
            assert_eq!(libraries.live_objects(), 0, "{case}: freed after it");
        } else {
            let frees = events[poisoned_at..]
                .iter()
                .filter(|&&event| event == "free");
            assert_eq!(frees.count(), 0, "{case}: leaked");
        }
    }
}

/// The destroys the session's close calls after its own wait, whatever it
/// returns.
const SESSION_DESTROYS: [&str; 2] = ["destroy_handle", "destroy_stream"];

/// The call `a_poisoned_algebra` makes fail, by the wait that fails.
#[derive(Clone, Copy, Debug)]
enum FailedCall {
    /// A gate split, whose wait for its factorization fails.
    Split,
    /// A duplicate, whose wait for its copy on the device fails.
    Duplicate,
    /// An upload, whose wait for its copy from the host fails.
    Upload,
}

impl FailedCall {
    /// The wait that fails.
    fn wait(self) -> &'static str {
        match self {
            Self::Split => "synchronize_stream",
            Self::Duplicate | Self::Upload => "synchronize_copies",
        }
    }
}

/// An algebra poisoned by a failed call whose wait failed twice: the
/// call's own and its release's, which waits again before freeing, as the
/// copy may still be in flight. Returns the algebra and the call's inputs,
/// which the device may still use.
fn a_poisoned_algebra(
    failed_call: FailedCall,
) -> (CudaAlgebra, Arc<FakeLibraries>, Vec<DeviceTensor>) {
    if let FailedCall::Split = failed_call {
        let (algebra, libraries, pair, _failed) = a_failed_split(true);
        return (algebra, libraries, pair.into());
    }
    let (algebra, libraries) = algebra();
    let duplicate = matches!(failed_call, FailedCall::Duplicate);
    let inputs = if duplicate {
        vec![upload(&algebra, &random(4, 14))]
    } else {
        Vec::new()
    };
    let wait = failed_call.wait();
    let waits = libraries.count(wait);
    libraries.fail(wait, waits + 1);
    libraries.fail(wait, waits + 2);
    let failed = if duplicate {
        algebra.duplicate(&inputs[0], 4)
    } else {
        algebra.upload(&random(4, 14))
    };
    assert!(
        matches!(
            failed,
            Err(CudaAlgebraError::ExecutionAndCleanupFailed { .. })
        ),
        "{failed_call:?}: both waits failed, not {failed:?}"
    );
    (algebra, libraries, inputs)
}
