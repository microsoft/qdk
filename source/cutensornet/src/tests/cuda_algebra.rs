// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the CUDA algebra on fake libraries that simulate cuTensorNet
//! and the CUDA Runtime on the CPU (`src/native_calls/fake_libraries.rs`),
//! which enforce cuTensorNet's call order and the rules on streams and
//! copies: what the algebra opens and releases, its tensors, and what it
//! does when a call fails. Its gate splits, its moves of the centre and
//! its scratch are in the submodules. The rules of its calls and the
//! circuits of the engine on it, checked against the reference algebra and
//! the exact reference (`algebra_rules.rs`, `engine_circuits.rs`), are
//! shared with the tests on a GPU (`tests/cuda_algebra_on_the_gpu.rs`).

mod algebra_rules;
mod engine_circuits;
mod gate_split;
mod qr_move;
mod scratch;

use crate::cuda_algebra::{CudaAlgebra, DeviceTensor};
use crate::native_calls::{
    NativeCalls, error::NativeError, fake_libraries::FakeLibraries, tensor_calls::SvdTruncation,
};
use algebra_rules::{Move, Split, random, random_gate, random_pair};
use canonical_mps::algebra::{CentreSide, SiteAlgebra};
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

/// An algebra opens only on a cuTensorNet whose tensor API is loaded,
/// which discovery does not require: one without it is refused before the
/// algebra opens anything on the device.
#[test]
fn an_algebra_needs_the_tensor_api() {
    let libraries = Arc::new(FakeLibraries::new());
    libraries.fail("check_tensor_functions", 1);
    let native_calls: Arc<dyn NativeCalls> = libraries.clone();
    let opened = CudaAlgebra::with_native_calls(native_calls);
    assert!(
        matches!(
            opened,
            Err(NativeError::NativeCallFailed {
                operation: "check_tensor_functions",
                ..
            })
        ),
        "{opened:?}"
    );
    assert_eq!(libraries.events(), ["check_tensor_functions"]);
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

/// A site uploaded and read back is the site: whole, or its first
/// elements.
#[test]
fn an_uploaded_tensor_reads_back_its_values() {
    let (algebra, _libraries) = algebra();
    let values = random(12, 1);
    let tensor = upload(&algebra, &values);
    assert_eq!(read(&algebra, &tensor, 12), values);
    assert_eq!(read(&algebra, &tensor, 5), values[..5]);
}

/// A fork's copy of a site holds its first `len` elements, and stays
/// whole once the site it copies is dropped.
#[test]
fn a_duplicate_outlives_its_source() {
    let (algebra, _libraries) = algebra();
    let values = random(12, 2);
    let source = upload(&algebra, &values);
    let copy = algebra
        .duplicate(&source, 8)
        .unwrap_or_else(|error| panic!("the duplicate succeeds: {error}"));
    drop(source);
    assert_eq!(read(&algebra, &copy, 8), values[..8]);
}

/// A call that would read past a tensor's elements is a defect of its
/// caller, refused before the device reads memory the tensor does not
/// own, whichever method makes it.
#[test]
fn a_call_reading_past_a_tensors_elements_is_refused() {
    let (algebra, _libraries) = algebra();
    let (a, b) = random_pair(1, 2, 1, 3);
    let (small, other) = (upload(&algebra, &a[..2]), upload(&algebra, &b));
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
    let refusals = [
        ("read", algebra.read(&small, 3).map(drop)),
        ("duplicate", algebra.duplicate(&small, 3).map(drop)),
        (
            "gate split",
            algebra.gate_split(&split.call(&small, &other)).map(drop),
        ),
        (
            "move of the centre",
            algebra.qr_move(&moved.call(&small, &other)).map(drop),
        ),
    ];
    for (method, refused) in refusals {
        assert!(
            matches!(refused, Err(NativeError::InvalidNativeCall { .. })),
            "{method}: {refused:?}"
        );
    }
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
/// singular values' buffers, the prepared contractions and the tensors.
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

/// A failed gate split whose cleanup fails too, a release of its SVD
/// report: both failures are returned, so the cleanup never hides the
/// cause.
#[test]
fn a_cleanup_failure_never_hides_the_cause() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 8);
    libraries.misreport_truncation(SvdTruncation {
        reduced_extent: 0,
        full_extent: 2,
        discarded_weight: 0.0,
    });
    libraries.fail("destroy_svd_info", 1);
    let failed = one_qubit_split(&a, &b).on(&algebra).map(drop);
    let Err(NativeError::ExecutionAndCleanupFailed { execution, cleanup }) = failed else {
        panic!("both failures, not {failed:?}");
    };
    assert!(
        matches!(*execution, NativeError::InvalidNativeResult { .. }),
        "{execution:?}"
    );
    assert!(
        matches!(
            *cleanup,
            NativeError::NativeCallFailed {
                operation: "destroy_svd_info",
                ..
            }
        ),
        "{cleanup:?}"
    );
}

/// A failed gate split: its library misreports the truncation, and, when
/// `wait_fails`, the failed call's wait for the device fails too. Returns
/// the algebra, the pair it split, which the device may still read, and
/// the split's failure.
fn a_failed_split(
    wait_fails: bool,
) -> (
    CudaAlgebra,
    Arc<FakeLibraries>,
    [DeviceTensor; 2],
    NativeError,
) {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 9);
    let pair = [upload(&algebra, &a), upload(&algebra, &b)];
    libraries.misreport_truncation(SvdTruncation {
        reduced_extent: 0,
        full_extent: 2,
        discarded_weight: 0.0,
    });
    if wait_fails {
        // The split's own wait succeeds; the next is the failed call's.
        let waits = libraries.count("synchronize_stream");
        libraries.fail("synchronize_stream", waits + 2);
    }
    let failed = algebra
        .gate_split(&one_qubit_split(&a, &b).call(&pair[0], &pair[1]))
        .expect_err("a misreported truncation");
    (algebra, libraries, pair, failed)
}

/// A failed call waits for the device before it releases its buffers;
/// when that wait fails, nothing tells when the device is done with them,
/// so every later call is refused as poisoned, the wait's failure kept
/// beside the call's.
#[test]
fn a_failed_call_that_cannot_wait_for_the_device_poisons_the_algebra() {
    let (algebra, _libraries, _pair, failed) = a_failed_split(true);
    let NativeError::ExecutionAndCleanupFailed { execution, cleanup } = failed else {
        panic!("both failures, not {failed:?}");
    };
    assert!(
        matches!(*execution, NativeError::InvalidNativeResult { .. }),
        "{execution:?}"
    );
    assert!(
        matches!(
            *cleanup,
            NativeError::NativeCallFailed {
                operation: "synchronize_stream",
                ..
            }
        ),
        "{cleanup:?}"
    );
    let next = algebra.upload(&random(2, 10));
    assert!(
        matches!(next, Err(NativeError::Poisoned { .. })),
        "{next:?}"
    );
}

/// A failed call that waited for the device has released what it made,
/// so the algebra stays usable.
#[test]
fn a_failed_call_that_waited_for_the_device_leaves_the_algebra_usable() {
    let (algebra, _libraries, _pair, failed) = a_failed_split(false);
    assert!(
        matches!(failed, NativeError::InvalidNativeResult { .. }),
        "{failed:?}"
    );
    let values = random(2, 11);
    let tensor = upload(&algebra, &values);
    assert_eq!(read(&algebra, &tensor, 2), values);
}

/// A poisoned algebra keeps the failed call's objects and buffers, which
/// the device may still use, and releases them when it is dropped, once
/// the device is done.
#[test]
fn a_poisoned_algebra_releases_the_failed_calls_objects_when_dropped() {
    let (algebra, libraries, pair, _failed) = a_failed_split(true);
    drop(algebra);
    drop(pair);
    assert_eq!(libraries.live_objects(), 0);
}

/// The engine on the algebra runs a long circuit as the exact reference
/// does, without truncation.
#[test]
fn the_engine_on_the_algebra_agrees_with_the_reference() {
    engine_circuits::a_long_circuit_agrees_with_the_reference_without_truncation(algebra().0);
}

/// The engine on the algebra, truncating, stays within the distance its
/// splits allow of the exact reference.
#[test]
fn the_engine_on_the_algebra_truncates_within_its_bound() {
    engine_circuits::a_truncated_circuit_stays_within_the_distance_its_splits_allow(algebra().0);
}

/// The engine on the algebra, capped, reads as the engine on the CPU.
#[test]
fn the_engine_on_the_algebra_reads_as_on_the_cpu_when_capped() {
    engine_circuits::a_capped_circuit_reads_as_on_the_cpu_engine(algebra().0);
}
