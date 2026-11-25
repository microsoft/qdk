// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The scope of one call of the CUDA algebra: what a failed call releases,
//! that its cleanup's failure never hides its own, that it never writes a
//! tensor it reads, and that it grows the singular values' buffer only
//! when a split needs more. That a failed call waits for the device first
//! is pinned with the poisoning it leads to, in the algebra's tests.

use super::*;
use crate::engine::{
    cuda_algebra::{
        CudaAlgebra,
        tests::{
            Move, Split, c, failed_at, one_qubit_gate, one_qubit_split, random_pair, read, upload,
        },
    },
    native_calls::{fake_libraries::FakeLibraries, tensor_calls::SvdTruncation},
    tests::algebra_on_the_fake as algebra,
};
use canonical_mps::site_algebra::SiteAlgebra;

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
    let Err(CudaAlgebraError::ExecutionAndCleanupFailed { execution, cleanup }) = failed else {
        panic!("both failures, not {failed:?}");
    };
    assert!(
        matches!(*execution, CudaAlgebraError::InvalidNativeResult { .. }),
        "{execution:?}"
    );
    assert!(
        matches!(
            *cleanup,
            CudaAlgebraError::NativeCallFailed {
                operation: "destroy_svd_info",
                ..
            }
        ),
        "{cleanup:?}"
    );
}

/// A call's allocation whose allocator hands out the buffer of a tensor
/// the call reads, as only a faulty allocator would: the call is refused
/// before it writes that buffer, and the tensor keeps its buffer and its
/// values. Cases: each step of a call that allocates (an output, an
/// intermediate, the grown singular values' buffer, or the grown
/// scratch), and the first allocation of each method, as every method
/// allocates through its call's scope.
#[test]
fn an_allocation_returning_an_input_tensors_buffer_is_refused() {
    type Call<'c> =
        &'c dyn Fn(&CudaAlgebra, &DeviceTensor, &DeviceTensor) -> Result<(), CudaAlgebraError>;
    let (a, b) = random_pair(2, 3, 2, 5);
    let split = Split::untruncated(&a, &b, [2, 3, 2]);
    let moved = Move::to_the_right(&a, &b, [2, 3, 2]);
    let gated = Move {
        gate: Some([c(0.0, 1.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)]),
        ..Move::to_the_right(&a, &b, [2, 3, 2])
    };
    let calls: [(&str, Call); 8] = [
        ("an output", &|algebra, left, right| {
            algebra.in_call_scope(&[left, right], |scope| scope.output(4).map(drop))
        }),
        ("an intermediate", &|algebra, left, right| {
            algebra.in_call_scope(&[left, right], |scope| scope.intermediate(4).map(drop))
        }),
        ("the singular values", &|algebra, left, right| {
            algebra.in_call_scope(&[left, right], |scope| scope.singular_values(8).map(drop))
        }),
        ("the scratch", &|algebra, left, right| {
            algebra.in_call_scope(&[left, right], |scope: &mut CallScope<'_>| {
                scope.lend_workspace()
            })
        }),
        ("a gate split", &|algebra, left, right| {
            algebra.gate_split(&split.call(left, right)).map(drop)
        }),
        ("a move of the centre", &|algebra, left, right| {
            algebra.qr_move(&moved.call(left, right)).map(drop)
        }),
        ("a gated move", &|algebra, left, right| {
            algebra.qr_move(&gated.call(left, right)).map(drop)
        }),
        ("a duplicate", &|algebra, _, right| {
            algebra.duplicate(right, b.len()).map(drop)
        }),
    ];
    for (call_name, call) in calls {
        let (algebra, libraries) = algebra();
        let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
        libraries.alias_allocation(1);
        let refused = call(&algebra, &left, &right);
        assert!(
            matches!(refused, Err(CudaAlgebraError::InvalidNativeResult { .. })),
            "{call_name}: {refused:?}"
        );
        assert_eq!(read(&algebra, &right, b.len()), b, "{call_name}");
        drop((left, right, algebra));
        assert_eq!(libraries.live_objects(), 0, "{call_name}");
    }
}

/// The singular values' buffer a gate split writes is kept across calls
/// and grown only when a split needs more: calls asking for 8 values, 8
/// again, 7 (the same buffer, at an exact fit, as two values fill an
/// element), 9 (a new buffer, the smaller freed), then 2 (the larger
/// kept).
#[test]
fn the_singular_values_buffer_grows_only_when_a_split_needs_more() {
    let (algebra, libraries) = algebra();
    let buffer_for = |count: usize| {
        algebra
            .in_call_scope(&[], |scope| scope.singular_values(count))
            .unwrap_or_else(|error| panic!("a buffer for {count} values: {error}"))
    };
    let first = buffer_for(8);
    let live = libraries.live_objects();
    assert_eq!(buffer_for(8), first, "the same count");
    assert_eq!(buffer_for(7), first, "an exact fit");
    let grown = buffer_for(9);
    assert_ne!(grown, first, "more");
    assert_eq!(
        libraries.live_objects(),
        live,
        "the smaller buffer is freed"
    );
    assert_eq!(buffer_for(2), grown, "fewer");
}

/// A call that fails at any of its native calls fails with that call's
/// failure and leaves no object or buffer it made behind. Cases: a gate
/// split, after two of the same shape that succeeded (so the algebra holds
/// its scratch already); a move of the centre of a new shape, after one of
/// another shape (so the algebra holds a network already), which leaves
/// the networks it prepared, which the algebra keeps (each a network and
/// its path), and no network half made, so the same move then succeeds.
#[test]
fn a_failed_call_releases_what_it_made() {
    a_failed_split_releases_what_it_made();
    a_failed_move_releases_what_it_made();
}

fn a_failed_split_releases_what_it_made() {
    let (a, b) = random_pair(2, 2, 2, 16);
    let split = Split::untruncated(&a, &b, [2, 2, 2]);
    for operation in [
        "create_tensor_descriptor",
        "create_svd_config",
        "configure_svd",
        "create_svd_info",
        "copy_to_device",
        "synchronize_copies",
        "gate_split_workspace_sizes",
        "workspace_size",
        "set_workspace",
        "allocate",
        "gate_split",
        "synchronize_stream",
        "svd_truncation",
    ] {
        let (algebra, libraries) = algebra();
        let pair = [upload(&algebra, &a), upload(&algebra, &b)];
        let call = split.call(&pair[0], &pair[1]);
        drop(algebra.gate_split(&call).expect("the first split"));
        let live = libraries.live_objects();
        let per_split = calls_to(&libraries, operation, || {
            drop(algebra.gate_split(&call).expect("the second split"));
        });
        assert!(per_split > 0, "a split calls {operation}");
        fail_the_next_calls_last(&libraries, operation, per_split);
        let failed = algebra.gate_split(&call).map(drop);
        assert_eq!(
            failed_at(&failed),
            Some(operation),
            "split, {operation}: {failed:?}"
        );
        assert_eq!(libraries.live_objects(), live, "split, {operation}");
    }
}

fn a_failed_move_releases_what_it_made() {
    let (a, b) = random_pair(2, 2, 2, 30);
    let (c, d) = random_pair(2, 3, 2, 32);
    let moved = |a, b, m| Move {
        gate: Some(one_qubit_gate(34)),
        ..Move::to_the_right(a, b, [2, m, 2])
    };
    let warm = |algebra: &CudaAlgebra| {
        moved(&a, &b, 2).on(algebra).expect("the first move");
    };
    let failing = moved(&c, &d, 3);
    for operation in [
        "allocate",
        "copy_to_device",
        "synchronize_copies",
        "create_tensor_descriptor",
        "create_contraction_network",
        "create_optimizer_info",
        "optimize_contraction",
        "contraction_workspace_sizes",
        "workspace_size",
        "set_workspace",
        "prepare_contraction",
        "contract",
        "qr_workspace_sizes",
        "tensor_qr",
        "synchronize_stream",
    ] {
        // The calls a move makes, counted on another algebra.
        let per_move = {
            let (algebra, libraries) = algebra();
            warm(&algebra);
            calls_to(&libraries, operation, || {
                drop(failing.on(&algebra).expect("the move"));
            })
        };
        assert!(per_move > 0, "a move calls {operation}");
        let (algebra, libraries) = algebra();
        warm(&algebra);
        let live = libraries.live_objects();
        let prepared = libraries.count("prepare_contraction");
        fail_the_next_calls_last(&libraries, operation, per_move);
        let failed = failing.on(&algebra).map(drop);
        let failed_preparation = usize::from(operation == "prepare_contraction");
        let prepared = libraries.count("prepare_contraction") - prepared - failed_preparation;
        assert_eq!(
            failed_at(&failed),
            Some(operation),
            "move, {operation}: {failed:?}"
        );
        assert_eq!(
            libraries.live_objects(),
            live + 2 * prepared,
            "move, {operation}"
        );
        let retried = failing.on(&algebra).map(drop);
        assert!(retried.is_ok(), "move, {operation}, the retry: {retried:?}");
    }
}

/// The calls to `operation` that `call` makes.
fn calls_to(libraries: &FakeLibraries, operation: &'static str, call: impl FnOnce()) -> usize {
    let before = libraries.count(operation);
    call();
    libraries.count(operation) - before
}

/// Makes the last call to `operation` of the next call fail, a call that
/// makes `per_call` of them, so that every object it made before is
/// released.
fn fail_the_next_calls_last(libraries: &FakeLibraries, operation: &'static str, per_call: usize) {
    libraries.fail(operation, libraries.count(operation) + per_call);
}
