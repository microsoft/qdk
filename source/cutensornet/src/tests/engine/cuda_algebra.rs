// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the CUDA algebra on fake libraries that simulate cuTensorNet
//! and the CUDA Runtime on the CPU
//! (`src/engine/native_calls/fake_libraries.rs`), which enforce
//! cuTensorNet's call order and the rules on streams and copies: that it
//! meets `SiteAlgebra`'s contract (`canonical_mps`'s conformance kit), what
//! it opens and releases, the tensors it refuses, and what it does when a
//! call fails. The scope of one call, its gate splits' reports, its
//! contraction networks and its scratch are in the submodules. The kit and
//! the engine's circuits also run on a GPU
//! (`tests/cuda_algebra_on_the_gpu.rs`).

mod call_scope;
mod contraction;
mod error;
mod gate_split;
mod misaligned_allocator;
mod scratch;

use crate::engine::CudaAlgebraError;
use crate::engine::cuda_algebra::{CudaAlgebra, DeviceTensor};
use crate::engine::native_calls::{NativeCalls, SvdTruncation, fake_libraries::FakeLibraries};
use canonical_mps::site_algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
    conformance,
};
use num_complex::Complex64;
use std::sync::Arc;

fn algebra() -> (CudaAlgebra, Arc<FakeLibraries>) {
    let libraries = Arc::new(FakeLibraries::new());
    let native_calls: Arc<dyn NativeCalls> = libraries.clone();
    let algebra = CudaAlgebra::with_native_calls(native_calls)
        .unwrap_or_else(|error| panic!("the fake libraries open an algebra: {error}"));
    (algebra, libraries)
}

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn upload(algebra: &CudaAlgebra, values: &[Complex64]) -> DeviceTensor {
    algebra
        .upload(values)
        .unwrap_or_else(|error| panic!("the upload succeeds: {error}"))
}

fn read(algebra: &CudaAlgebra, tensor: &DeviceTensor, len: usize) -> Vec<Complex64> {
    algebra
        .read(tensor, len)
        .unwrap_or_else(|error| panic!("the read succeeds: {error}"))
        .into_owned()
}

/// A split of `a` and `b` read as two sites of one qubit with a bond of
/// one, under a random gate, without truncation: valid inputs for a call
/// whose outcome a test does not read.
fn one_qubit_split<'v>(a: &'v [Complex64], b: &'v [Complex64]) -> Split<'v> {
    Split {
        a,
        b,
        l: 1,
        m: 1,
        r: 1,
        gate: random_gate(7),
        cutoff: 0.0,
        max_extent: usize::MAX,
        centre: CentreSide::Left,
    }
}

/// A gate split of `a` and `b`, of extents `[l, 2, m]` and `[m, 2, r]`,
/// with `gate`, `cutoff`, `max_extent` and `centre`.
struct Split<'v> {
    a: &'v [Complex64],
    b: &'v [Complex64],
    l: usize,
    m: usize,
    r: usize,
    gate: [Complex64; 16],
    cutoff: f64,
    max_extent: usize,
    centre: CentreSide,
}

impl Split<'_> {
    /// The split on `algebra`, its pair uploaded first.
    fn on(
        &self,
        algebra: &CudaAlgebra,
    ) -> Result<GateSplitOutcome<DeviceTensor>, CudaAlgebraError> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        algebra.gate_split(&self.call(&a, &b))
    }

    /// The call on the tensors `a` and `b`.
    fn call<'t>(
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
struct Move<'v> {
    a: &'v [Complex64],
    b: &'v [Complex64],
    l: usize,
    m: usize,
    r: usize,
    gate: Option<[Complex64; 4]>,
    centre: CentreSide,
}

impl Move<'_> {
    /// The move on `algebra`, its pair uploaded first.
    fn on(&self, algebra: &CudaAlgebra) -> Result<QrMoveOutcome<DeviceTensor>, CudaAlgebraError> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        algebra.qr_move(&self.call(&a, &b))
    }

    /// The call on the tensors `a` and `b`.
    fn call<'t>(&self, a: &'t DeviceTensor, b: &'t DeviceTensor) -> QrMoveCall<'t, DeviceTensor> {
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

/// `len` complex numbers whose parts lie in `[-0.5, 0.5)`, from a linear
/// congruential generator seeded by `seed`.
fn random(len: usize, seed: u64) -> Vec<Complex64> {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The top 53 bits, exactly representable.
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit a double")]
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        unit - 0.5
    };
    (0..len)
        .map(|_| {
            let re = next();
            Complex64::new(re, next())
        })
        .collect()
}

/// A pair `A`, `B` of extents `[l, 2, m]` and `[m, 2, r]`, random from
/// `seed`.
fn random_pair(l: usize, m: usize, r: usize, seed: u64) -> (Vec<Complex64>, Vec<Complex64>) {
    (random(l * 2 * m, seed), random(m * 2 * r, seed + 1))
}

/// A random two-site gate, row-major.
fn random_gate(seed: u64) -> [Complex64; 16] {
    random(16, seed).try_into().expect("16 entries")
}

/// A random one-site gate, row-major.
fn one_qubit_gate(seed: u64) -> [Complex64; 4] {
    random(4, seed).try_into().expect("4 entries")
}

/// The algebra meets `SiteAlgebra`'s contract: every case of
/// `canonical_mps`'s conformance kit passes, on the fake libraries, whose
/// SVD and QR are `dense_linalg`'s, as the CPU algebra's are.
#[test]
fn the_cuda_algebra_conforms() {
    let failures = conformance::check(&algebra().0);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// An algebra whose opening fails after its device session is open (the
/// workspace descriptor, the gate's buffer) releases what it had made.
#[test]
fn a_failed_opening_releases_what_it_made() {
    for operation in ["create_workspace", "allocate"] {
        let libraries = Arc::new(FakeLibraries::new());
        libraries.fail(operation, 1);
        let native_calls: Arc<dyn NativeCalls> = libraries.clone();
        let opened = CudaAlgebra::with_native_calls(native_calls);
        assert!(opened.is_err(), "{operation}: {opened:?}");
        assert_eq!(libraries.live_objects(), 0, "{operation}");
    }
}

/// A call that would read past a tensor's elements is a defect of its
/// caller, refused before the device reads memory the tensor does not
/// own, and before the call makes any native object, whichever method
/// makes it and whichever of a pair's tensors is short.
#[test]
fn a_call_reading_past_a_tensors_elements_is_refused() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 2, 1, 3);
    let (small, other) = (upload(&algebra, &a[..2]), upload(&algebra, &b));
    let (left, small_right) = (upload(&algebra, &a), upload(&algebra, &b[..2]));
    let split = Split {
        m: 2,
        ..one_qubit_split(&a, &b)
    };
    let moved = Move {
        a: &a,
        b: &b,
        l: 1,
        m: 2,
        r: 1,
        gate: None,
        centre: CentreSide::Right,
    };
    let is_refused = |method: &str, refuse: &dyn Fn() -> Result<(), CudaAlgebraError>| {
        let before = libraries.events().len();
        let refused = refuse();
        assert!(
            matches!(refused, Err(CudaAlgebraError::InvalidNativeCall { .. })),
            "{method}: {refused:?}"
        );
        let made = libraries.events()[before..]
            .iter()
            .filter(|event| event.starts_with("create_") || **event == "allocate")
            .count();
        assert_eq!(made, 0, "{method}: no native object made");
    };
    is_refused("read", &|| algebra.read(&small, 3).map(drop));
    is_refused("duplicate", &|| algebra.duplicate(&small, 3).map(drop));
    for (side, left, right) in [("left", &small, &other), ("right", &left, &small_right)] {
        is_refused(&format!("gate split, {side} short"), &|| {
            algebra.gate_split(&split.call(left, right)).map(drop)
        });
        is_refused(&format!("move of the centre, {side} short"), &|| {
            algebra.qr_move(&moved.call(left, right)).map(drop)
        });
    }
}

/// A tensor is freed through the session of the algebra that made it,
/// which knows nothing of another algebra's device work: so another
/// algebra, whose failed call could not keep the tensor from being freed
/// while that call's work may still read it, refuses it before any native
/// call, whichever method is given it and whichever of a pair's tensors it
/// is, and so does an engine method that passes another engine's sites to
/// the algebra (its fork). Nothing is leaked.
#[test]
fn a_tensor_of_another_algebra_is_refused() {
    use crate::CudaMpsEngine;
    use canonical_mps::CanonicalMpsError;
    use tensornet::{MpsEngine, MpsShape};

    let libraries = Arc::new(FakeLibraries::new());
    let open = || {
        let native_calls: Arc<dyn NativeCalls> = libraries.clone();
        CudaAlgebra::with_native_calls(native_calls)
            .unwrap_or_else(|error| panic!("the fake libraries open an algebra: {error}"))
    };
    let (owner, algebra) = (open(), open());
    let (a, b) = random_pair(1, 1, 1, 12);
    let (foreign_a, foreign_b) = (upload(&owner, &a), upload(&owner, &b));
    let (own_a, own_b) = (upload(&algebra, &a), upload(&algebra, &b));
    let split = one_qubit_split(&a, &b);
    let moved = Move {
        a: &a,
        b: &b,
        l: 1,
        m: 1,
        r: 1,
        gate: None,
        centre: CentreSide::Right,
    };
    let (owner_engine, engine) = (CudaMpsEngine::new(open()), CudaMpsEngine::new(open()));
    let state = owner_engine
        .prepare(&MpsShape::qubit_chain(1, 1).expect("one qubit"))
        .expect("the zero state");
    let is_refused = |method: &str, refuse: &dyn Fn() -> Result<(), CudaAlgebraError>| {
        let before = libraries.events().len();
        let refused = refuse();
        assert!(
            matches!(refused, Err(CudaAlgebraError::ForeignTensor)),
            "{method}: {refused:?}"
        );
        let events = libraries.events();
        assert_eq!(
            events.len(),
            before,
            "{method}: no native call, not {:?}",
            &events[before..]
        );
    };
    is_refused("read", &|| algebra.read(&foreign_a, 2).map(drop));
    is_refused("duplicate", &|| algebra.duplicate(&foreign_a, 2).map(drop));
    for (side, left, right) in [("left", &foreign_a, &own_b), ("right", &own_a, &foreign_b)] {
        is_refused(&format!("gate split, {side} foreign"), &|| {
            algebra.gate_split(&split.call(left, right)).map(drop)
        });
        is_refused(&format!("move of the centre, {side} foreign"), &|| {
            algebra.qr_move(&moved.call(left, right)).map(drop)
        });
    }
    is_refused("the engine's fork", &|| match engine.fork(&state) {
        Ok(_) => Ok(()),
        Err(CanonicalMpsError::Algebra(error)) => Err(error),
        Err(other) => panic!("the fork fails in the algebra, not {other:?}"),
    });
    drop((foreign_a, foreign_b, own_a, own_b, owner, algebra));
    drop((state, owner_engine, engine));
    assert_eq!(libraries.live_objects(), 0);
}

/// Every method returns with its device work done and its copies made, so
/// that its outputs can be read, and given to the next call, at once: the
/// fake refuses a tensor call while a copy may still run, and a copy of a
/// buffer a stream may still write. An upload's and a duplicate's outputs
/// go to a move of the centre, whose first device work is a QR; a split's
/// and a move's are read.
#[test]
fn every_methods_outputs_are_ready_when_it_returns() {
    let (algebra, _libraries) = algebra();
    let (a, b) = random_pair(2, 2, 2, 4);
    let moved = Move {
        a: &a,
        b: &b,
        l: 2,
        m: 2,
        r: 2,
        gate: None,
        centre: CentreSide::Right,
    };
    let split = Split {
        l: 2,
        m: 2,
        r: 2,
        ..one_qubit_split(&a, &b)
    };
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    let uploaded = algebra.qr_move(&moved.call(&left, &right)).map(drop);
    let copy = algebra.duplicate(&left, a.len()).expect("the duplicate");
    let duplicated = algebra.qr_move(&moved.call(&copy, &right)).map(drop);
    let outputs = [
        ("upload", uploaded),
        ("duplicate", duplicated),
        ("gate split", split.on(&algebra).map(drop)),
        ("move of the centre", moved.on(&algebra).map(drop)),
    ];
    for (method, used) in outputs {
        assert!(used.is_ok(), "{method}'s outputs: {used:?}");
    }
}

/// Dropping an algebra and its tensors releases every native object: the
/// device session, the workspace and its scratch, the gate's and the
/// singular values' buffers (and the smaller one a growth replaced), the
/// prepared contractions and the tensors.
#[test]
fn dropping_the_algebra_and_its_tensors_releases_every_native_object() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 3, 2, 5);
    let split = Split {
        l: 2,
        m: 3,
        r: 2,
        ..one_qubit_split(&a, &b)
    };
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    // A smaller split first, so that the singular values' buffer grows.
    let (small_a, small_b) = random_pair(1, 1, 1, 4);
    one_qubit_split(&small_a, &small_b)
        .on(&algebra)
        .expect("the smaller split");
    let outcome = algebra
        .gate_split(&split.call(&left, &right))
        .expect("the split");
    let moved = Move {
        a: &a,
        b: &b,
        l: 2,
        m: 3,
        r: 2,
        gate: Some([c(0.0, 1.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)]),
        centre: CentreSide::Right,
    };
    let moved = algebra
        .qr_move(&moved.call(&left, &right))
        .expect("the move");
    drop(algebra);
    drop((left, right, outcome, moved));
    assert_eq!(libraries.live_objects(), 0);
}

/// A tensor may outlive its algebra, as a state may outlive its engine: it
/// keeps the device session open, and frees its buffer when dropped.
#[test]
fn a_tensor_outliving_its_algebra_frees_its_buffer_when_dropped() {
    let (algebra, libraries) = algebra();
    let values = random(4, 6);
    let tensor = upload(&algebra, &values);
    drop(algebra);
    assert_eq!(
        libraries.live_handles(),
        1,
        "the tensor keeps the session open"
    );
    drop(tensor);
    assert_eq!(libraries.live_objects(), 0);
}

/// CUDA's current device is per thread, and the engine may call the
/// algebra on a thread that last used another device: every method, and
/// the drop of a tensor and of the algebra, selects the algebra's device
/// before any other native call.
#[test]
fn every_call_selects_the_algebras_device_first() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 12);
    let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
    let split = one_qubit_split(&a, &b);
    let moved = Move {
        a: &a,
        b: &b,
        l: 1,
        m: 1,
        r: 1,
        gate: None,
        centre: CentreSide::Right,
    };
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
/// the algebra's buffers.
#[test]
fn a_drop_that_cannot_select_the_device_frees_nothing() {
    for tensor_dropped in [true, false] {
        let dropped = if tensor_dropped {
            "a tensor"
        } else {
            "the algebra"
        };
        let (algebra, libraries) = algebra();
        let (a, b) = random_pair(1, 1, 1, 15);
        drop(one_qubit_split(&a, &b).on(&algebra).expect("the split"));
        let tensor = upload(&algebra, &a);
        let (before, live) = (libraries.events().len(), libraries.live_objects());
        libraries.fail("set_device", libraries.count("set_device") + 1);
        if tensor_dropped {
            drop(tensor);
        } else {
            drop(algebra);
        }
        let events = libraries.events();
        let released = events[before..]
            .iter()
            .filter(|&&event| event == "free" || event.starts_with("destroy_"));
        assert_eq!(
            released.count(),
            0,
            "{dropped}: nothing freed: {:?}",
            &events[before..]
        );
        assert_eq!(libraries.live_objects(), live, "{dropped}: leaked");
    }
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

/// A failed call waits for the device before it releases its buffers;
/// when that wait fails, nothing tells when the device is done with them,
/// so every later call is refused as poisoned, whichever method makes it
/// and whatever its arguments (a tensor too short for the call, or another
/// algebra's, included), the wait's failure kept beside the call's.
#[test]
fn a_failed_call_that_cannot_wait_for_the_device_poisons_the_algebra() {
    let (algebra, _libraries, pair, failed) = a_failed_split(true);
    let CudaAlgebraError::ExecutionAndCleanupFailed { execution, cleanup } = failed else {
        panic!("both failures, not {failed:?}");
    };
    assert!(is_a_failed_wait(&execution), "{execution:?}");
    assert!(is_a_failed_wait(&cleanup), "{cleanup:?}");
    // The pair holds two elements each; these calls ask for more.
    let values = random(4, 10);
    let split = Split {
        m: 2,
        ..one_qubit_split(&values, &values)
    };
    let moved = Move {
        a: &values,
        b: &values,
        l: 1,
        m: 2,
        r: 1,
        gate: None,
        centre: CentreSide::Right,
    };
    let is_poisoned = |method: &str, call: &dyn Fn() -> Result<(), CudaAlgebraError>| {
        let next = call();
        assert!(
            matches!(next, Err(CudaAlgebraError::Poisoned { .. })),
            "{method}: {next:?}"
        );
    };
    is_poisoned("upload", &|| algebra.upload(&values).map(drop));
    is_poisoned("read, short", &|| algebra.read(&pair[0], 3).map(drop));
    is_poisoned("duplicate, short", &|| {
        algebra.duplicate(&pair[0], 3).map(drop)
    });
    is_poisoned("gate split, short", &|| {
        algebra
            .gate_split(&split.call(&pair[0], &pair[1]))
            .map(drop)
    });
    is_poisoned("move of the centre, short", &|| {
        algebra.qr_move(&moved.call(&pair[0], &pair[1])).map(drop)
    });
    let (other, _other_libraries) = self::algebra();
    let foreign = upload(&other, &values);
    is_poisoned("read, another algebra's", &|| {
        algebra.read(&foreign, 4).map(drop)
    });
    is_poisoned("gate split, another algebra's on the right", &|| {
        algebra
            .gate_split(&one_qubit_split(&values, &values).call(&pair[0], &foreign))
            .map(drop)
    });
}

/// A failed call that waited for the device has released what it made,
/// so the algebra stays usable.
#[test]
fn a_failed_call_that_waited_for_the_device_leaves_the_algebra_usable() {
    let (algebra, _libraries, _pair, failed) = a_failed_split(false);
    assert!(is_a_failed_wait(&failed), "{failed:?}");
    let values = random(2, 11);
    let tensor = upload(&algebra, &values);
    assert_eq!(read(&algebra, &tensor, 2), values);
}

/// After a failed call that could not wait for the device, nothing tells when
/// the device is done with that call's buffers, its inputs or the algebra's
/// memory: none is freed until a later wait for the device (its stream and
/// its copies) succeeds (when the algebra is dropped, or else when its last
/// tensor is), and if none does, they are leaked; once one succeeds, a tensor
/// dropped is freed at once. The fake refuses to free a buffer while a
/// stream's work or a copy may still use it. Cases: the wait that failed (a
/// split's, on the stream, or a duplicate's, of its copy), whether the engine
/// drops the failed call's inputs before the algebra (as after a failed gate,
/// when it drops the state and keeps the algebra), and how many of the same
/// waits that follow fail. A duplicate has one input, whose drop after the
/// algebra's is also the last tensor's, so its cases drop it first. The
/// session's handle and stream are destroyed whatever its wait returns, as
/// CUDA defers a stream's release until its work completes.
#[test]
fn a_poisoned_algebra_frees_nothing_until_a_wait_for_the_device_succeeds() {
    for (wait, inputs_dropped_first, failed_waits) in [
        ("synchronize_stream", false, 0),
        ("synchronize_stream", true, 0),
        ("synchronize_stream", false, 1),
        ("synchronize_stream", true, 1),
        ("synchronize_stream", true, 2),
        ("synchronize_copies", true, 0),
        ("synchronize_copies", true, 1),
        ("synchronize_copies", true, 2),
    ] {
        let case = format!(
            "{wait} failed, inputs dropped first: {inputs_dropped_first}, \
             failed waits: {failed_waits}"
        );
        let (algebra, libraries, inputs) = a_poisoned_algebra(wait);
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

/// What the session destroys after its own wait, whatever it returns.
const SESSION_DESTROYS: [&str; 2] = ["destroy_handle", "destroy_stream"];

/// An algebra poisoned by a failed call whose `wait` (`synchronize_stream`,
/// a split's, or `synchronize_copies`, a duplicate's) failed twice: the
/// call's own and its release's. Returns the algebra and the call's inputs,
/// which the device may still use.
fn a_poisoned_algebra(wait: &'static str) -> (CudaAlgebra, Arc<FakeLibraries>, Vec<DeviceTensor>) {
    if wait == "synchronize_stream" {
        let (algebra, libraries, pair, _failed) = a_failed_split(true);
        return (algebra, libraries, pair.into());
    }
    let (algebra, libraries) = algebra();
    let source = upload(&algebra, &random(4, 14));
    let waits = libraries.count(wait);
    libraries.fail(wait, waits + 1);
    libraries.fail(wait, waits + 2);
    let failed = algebra.duplicate(&source, 4);
    assert!(
        matches!(
            failed,
            Err(CudaAlgebraError::ExecutionAndCleanupFailed { .. })
        ),
        "{failed:?}"
    );
    (algebra, libraries, vec![source])
}
