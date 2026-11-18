// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the algebra's cleanups: a failed release skips no other
//! release and poisons nothing, and a cleanup returns its first failed
//! release.

use super::*;
use crate::engine::cuda_algebra::{NETWORKS_KEPT, error::first_failure};

fn failed(operation: &'static str) -> CudaAlgebraError {
    CudaAlgebraError::NativeCallFailed {
        component: "CUDA Runtime",
        operation,
        status: 1,
        message: "out of memory".to_string(),
    }
}

/// A cleanup makes several releases (a call's scope, or the algebra's at
/// its drop, which only debug builds report); when the second and the
/// third fail, it returns the second's failure. A drop's report is not
/// observable, so this pins the rule every cleanup returns by.
#[test]
fn first_failure_returns_the_earliest_failed_release() {
    let released = first_failure([Ok(()), Err(failed("destroy")), Err(failed("free"))]);
    assert!(matches!(
        released,
        Err(CudaAlgebraError::NativeCallFailed {
            operation: "destroy",
            ..
        })
    ));
}

/// A release that fails with the device idle: at the algebra's drop (a
/// buffer's free), in a split (a descriptor's destroy), or in a move when
/// the cache of networks starts again (a network's destroy). Every other
/// release is still made, and the object is leaked, never retried. A
/// method whose release fails returns the failure and poisons nothing: the
/// same method then runs. The fake refuses to destroy a handle while an
/// object of it lives, so a leaked descriptor or network leaks the handle
/// at the session's close too; a buffer belongs to no handle.
#[test]
fn a_failed_release_leaks_its_object_and_poisons_nothing() {
    struct Case {
        release: &'static str,
        /// The calls made before the release is made to fail.
        before: fn(&CudaAlgebra),
        /// The method whose release fails; none for the algebra's drop.
        method: Option<fn(&CudaAlgebra) -> Result<(), CudaAlgebraError>>,
        /// The objects left once the algebra is dropped.
        left: usize,
    }
    fn split(algebra: &CudaAlgebra) -> Result<(), CudaAlgebraError> {
        let (a, b) = random_pair(1, 1, 1, 4);
        one_qubit_split(&a, &b).on(algebra).map(drop)
    }
    // A move to the right without a gate, of the `shape`-th new shape: it
    // contracts one network, of extents `[m, m]` and `[m, 2, r]` (here
    // `m <= 2l`), as in `the_networks_kept_are_bounded`.
    fn move_of_shape(algebra: &CudaAlgebra, shape: usize) -> Result<(), CudaAlgebraError> {
        let (m, r) = (shape / 17 + 1, shape % 17 + 1);
        let (a, b) = random_pair(16, m, r, 29);
        let moved = Move {
            a: &a,
            b: &b,
            l: 16,
            m,
            r,
            gate: None,
            centre: CentreSide::Right,
        };
        moved.on(algebra).map(drop)
    }
    let cases = [
        Case {
            release: "free",
            before: |algebra| split(algebra).expect("a split, so that the algebra holds buffers"),
            method: None,
            left: 1,
        },
        Case {
            release: "destroy_tensor_descriptor",
            before: |algebra| split(algebra).expect("a split that sizes the scratch"),
            method: Some(split),
            left: 2,
        },
        Case {
            release: "destroy_contraction_network",
            before: |algebra| {
                for shape in 0..NETWORKS_KEPT {
                    move_of_shape(algebra, shape).expect("a move whose network is kept");
                }
            },
            method: Some(|algebra| move_of_shape(algebra, NETWORKS_KEPT)),
            left: 2,
        },
    ];
    for case in cases {
        let release = case.release;
        let (algebra, libraries) = algebra();
        (case.before)(&algebra);
        libraries.fail(release, libraries.count(release) + 1);
        if let Some(method) = case.method {
            let failed = method(&algebra);
            let operation = match &failed {
                Err(CudaAlgebraError::NativeCallFailed { operation, .. }) => Some(*operation),
                _ => None,
            };
            assert_eq!(operation, Some(release), "{failed:?}");
            method(&algebra).unwrap_or_else(|error| panic!("{release}: the next call: {error}"));
        }
        drop(algebra);
        assert_eq!(libraries.live_objects(), case.left, "{release}");
    }
}
