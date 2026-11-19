// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the rule every cleanup of the engine follows: it returns the
//! first of its failed releases.

use crate::engine::cuda_algebra_error::{CudaAlgebraError, first_failure};

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
/// observable, so this pins the rule every cleanup follows.
#[test]
fn a_cleanup_returns_its_first_failed_release() {
    let released = first_failure([Ok(()), Err(failed("destroy")), Err(failed("free"))]);
    assert!(matches!(
        released,
        Err(CudaAlgebraError::NativeCallFailed {
            operation: "destroy",
            ..
        })
    ));
}
