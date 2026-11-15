// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the native errors: a call to functions the library lacks names
//! them; a cleanup never hides the cause of a failure, and a failed release
//! skips no other release.

use crate::LibraryFault;
use crate::native_calls::error::{NativeError, combine_execution_and_cleanup, first_failure};

fn failed(operation: &'static str) -> NativeError {
    NativeError::NativeCallFailed {
        component: "CUDA Runtime",
        operation,
        status: 1,
        message: "out of memory".to_string(),
    }
}

/// A cuTensorNet without the tensor API still runs the engine; a call to
/// that API fails naming every function missing, so the user knows what
/// to install.
#[test]
fn a_call_to_missing_functions_names_each_one() {
    let error = NativeError::missing_functions(
        "cuTensorNet",
        &[
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetTensorQR",
            },
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetNetworkContract",
            },
        ],
    );
    assert_eq!(
        error.to_string(),
        "cuTensorNet does not export cutensornetTensorQR, cutensornetNetworkContract"
    );
}

/// A compute fails, and freeing its scratch then fails too: the caller
/// learns both, the compute's failure as the execution's.
#[test]
fn an_execution_and_a_cleanup_that_both_fail_are_both_kept() {
    let combined = combine_execution_and_cleanup::<()>(Err(failed("compute")), Err(failed("free")));
    let Err(NativeError::ExecutionAndCleanupFailed { execution, cleanup }) = combined else {
        panic!("both failures are kept: {combined:?}");
    };
    assert!(matches!(
        *execution,
        NativeError::NativeCallFailed {
            operation: "compute",
            ..
        }
    ));
    assert!(matches!(
        *cleanup,
        NativeError::NativeCallFailed {
            operation: "free",
            ..
        }
    ));
}

/// Closing a state releases its objects one by one; when the second and
/// the third fail, the second's failure is the one reported.
#[test]
fn a_cleanup_reports_its_first_failed_release() {
    let released = first_failure([Ok(()), Err(failed("destroy")), Err(failed("free"))]);
    assert!(matches!(
        released,
        Err(NativeError::NativeCallFailed {
            operation: "destroy",
            ..
        })
    ));
}

/// A failed release does not stop the cleanup: the releases after it are
/// still made, so they leak nothing.
#[test]
fn a_failed_release_skips_no_later_release() {
    let mut attempted = 0;
    let releases = [Err(failed("destroy")), Ok(()), Ok(())]
        .into_iter()
        .inspect(|_| attempted += 1);
    first_failure(releases).expect_err("the first release fails");
    assert_eq!(attempted, 3);
}
