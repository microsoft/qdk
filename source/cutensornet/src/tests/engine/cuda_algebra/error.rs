// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the algebra's cleanups: a failed release skips no other
//! release, and a cleanup returns its first failed release.

use super::*;
use crate::engine::cuda_algebra::error::first_failure;

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

/// Dropping an algebra after a split releases its workspace, its scratch,
/// its buffers and its device session; when the first buffer's free fails,
/// every later release is still made, so that buffer alone leaks.
#[test]
fn a_failed_release_at_drop_skips_no_later_release() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(1, 1, 1, 4);
    one_qubit_split(&a, &b)
        .on(&algebra)
        .expect("a split, so that the algebra holds buffers");
    libraries.fail("free", libraries.count("free") + 1);
    drop(algebra);
    assert_eq!(libraries.live_objects(), 1, "only the failed free leaks");
}
