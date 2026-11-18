// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The scope of one call of the CUDA algebra: what a failed call releases,
//! that it waits for its copies first, that its cleanup's failure never
//! hides its own, and that it never writes a tensor it reads.

use super::*;

/// A failed duplicate whose wait for its copy failed: the call waits for
/// the copy again before it returns, so that the tensor it copied may be
/// freed, which the fake refuses while a copy may still read it.
#[test]
fn a_failed_call_waits_for_its_copies() {
    let (algebra, libraries) = algebra();
    let source = upload(&algebra, &random(4, 13));
    libraries.fail(
        "synchronize_copies",
        libraries.count("synchronize_copies") + 1,
    );
    let failed = algebra.duplicate(&source, 4);
    assert!(
        matches!(
            failed,
            Err(CudaAlgebraError::NativeCallFailed {
                operation: "synchronize_copies",
                ..
            })
        ),
        "{failed:?}"
    );
    drop((source, algebra));
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

/// A call whose allocator hands out the buffer of a tensor the call reads,
/// as only a faulty allocator would, whichever of the call's allocations
/// does: the call is refused before it writes that buffer, and the tensor
/// keeps its buffer and its values.
#[test]
fn an_allocation_returning_an_input_tensors_buffer_is_refused() {
    let gate = [c(0.0, 1.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)];
    // Each call, and which of its allocations returns the right tensor's
    // buffer.
    for (case, occurrence) in [
        ("a split's U", 1),
        ("a split's grown singular values", 3),
        ("a split's grown scratch", 1),
        ("a move's Q", 1),
        ("a gated move's gated site", 1),
        ("a duplicate", 1),
    ] {
        let (algebra, libraries) = algebra();
        // A smaller split first, which sizes the scratch, so that every
        // allocation counted below is the call's.
        let (small_a, small_b) = random_pair(1, 1, 1, 4);
        one_qubit_split(&small_a, &small_b)
            .on(&algebra)
            .expect("the smaller split");
        let (a, b) = random_pair(2, 3, 2, 5);
        let (left, right) = (upload(&algebra, &a), upload(&algebra, &b));
        if case == "a split's grown scratch" {
            libraries.set_workspace_bytes(1 << 16);
        }
        libraries.alias_allocation(occurrence);
        let moved = |gate| Move {
            a: &a,
            b: &b,
            l: 2,
            m: 3,
            r: 2,
            gate,
            centre: CentreSide::Right,
        };
        let split = Split {
            l: 2,
            m: 3,
            r: 2,
            ..one_qubit_split(&a, &b)
        };
        let refused = match case {
            "a move's Q" => algebra.qr_move(&moved(None).call(&left, &right)).map(drop),
            "a gated move's gated site" => algebra
                .qr_move(&moved(Some(gate)).call(&left, &right))
                .map(drop),
            "a duplicate" => algebra.duplicate(&right, b.len()).map(drop),
            _ => algebra.gate_split(&split.call(&left, &right)).map(drop),
        };
        assert!(
            matches!(refused, Err(CudaAlgebraError::InvalidNativeResult { .. })),
            "{case}: {refused:?}"
        );
        assert_eq!(read(&algebra, &right, b.len()), b, "{case}");
        drop((left, right, algebra));
        assert_eq!(libraries.live_objects(), 0, "{case}");
    }
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
    let split = Split {
        l: 2,
        m: 2,
        r: 2,
        ..one_qubit_split(&a, &b)
    };
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
        let before = libraries.count(operation);
        drop(algebra.gate_split(&call).expect("the second split"));
        let per_split = libraries.count(operation) - before;
        assert!(per_split > 0, "a split calls {operation}");
        // The split's last call to `operation`, so that every object made
        // before it is released.
        libraries.fail(operation, libraries.count(operation) + per_split);
        let failed = algebra.gate_split(&call).map(drop);
        assert!(
            matches!(
                &failed,
                Err(CudaAlgebraError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "split, {operation}: {failed:?}"
        );
        assert_eq!(libraries.live_objects(), live, "split, {operation}");
    }
}

fn a_failed_move_releases_what_it_made() {
    let (a, b) = random_pair(2, 2, 2, 30);
    let (c, d) = random_pair(2, 3, 2, 32);
    let moved = |a, b, m| Move {
        a,
        b,
        l: 2,
        m,
        r: 2,
        gate: Some(one_qubit_gate(34)),
        centre: CentreSide::Right,
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
        // The move's last call to `operation`, so that every object made
        // before it is released: counted on another algebra.
        let per_move = {
            let (algebra, libraries) = algebra();
            warm(&algebra);
            let before = libraries.count(operation);
            failing.on(&algebra).expect("the move");
            libraries.count(operation) - before
        };
        assert!(per_move > 0, "a move calls {operation}");
        let (algebra, libraries) = algebra();
        warm(&algebra);
        let live = libraries.live_objects();
        let prepared = libraries.count("prepare_contraction");
        libraries.fail(operation, libraries.count(operation) + per_move);
        let failed = failing.on(&algebra).map(drop);
        let failed_preparation = usize::from(operation == "prepare_contraction");
        let prepared = libraries.count("prepare_contraction") - prepared - failed_preparation;
        assert!(
            matches!(
                &failed,
                Err(CudaAlgebraError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
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
