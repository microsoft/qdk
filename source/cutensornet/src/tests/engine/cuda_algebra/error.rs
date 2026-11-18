// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the algebra's cleanups: a cleanup reports its first failed
//! release, and a failed release skips no other release.

use crate::engine::cuda_algebra::error::{CudaAlgebraError, first_failure};

fn failed(operation: &'static str) -> CudaAlgebraError {
    CudaAlgebraError::NativeCallFailed {
        component: "CUDA Runtime",
        operation,
        status: 1,
        message: "out of memory".to_string(),
    }
}

/// Dropping the algebra releases what it keeps one by one (its networks,
/// its workspace, its memory, its buffers); when the second and the third
/// release fail, the second's failure is the one reported.
#[test]
fn a_cleanup_reports_its_first_failed_release() {
    let released = first_failure([Ok(()), Err(failed("destroy")), Err(failed("free"))]);
    assert!(matches!(
        released,
        Err(CudaAlgebraError::NativeCallFailed {
            operation: "destroy",
            ..
        })
    ));
}

/// A failed release does not stop the algebra's cleanup: the releases
/// after it are still made, so they leak nothing.
#[test]
fn a_failed_release_skips_no_later_release() {
    let mut attempted = 0;
    let releases = [Err(failed("destroy")), Ok(()), Ok(())]
        .into_iter()
        .inspect(|_| attempted += 1);
    first_failure(releases).expect_err("the first release fails");
    assert_eq!(attempted, 3);
}
