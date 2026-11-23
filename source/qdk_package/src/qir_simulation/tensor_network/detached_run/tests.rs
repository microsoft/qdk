// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which Python exception a run thread that panicked raises
//! ([`panicked_run`]), a defect the package's Python tests cannot cause.

use super::{RunFailure, panicked_run};

/// A run thread that panics (a defect) raises `RuntimeError` with the
/// panic's message, rather than a `PanicException` from the thread scope,
/// whether the message is a literal or formatted (an `expect` on an error,
/// an index out of bounds, a `panic!` with arguments: most defects), which
/// reach the thread's join as a `&str` and a `String` payload.
#[test]
fn a_run_that_panics_is_a_runtime_error_with_its_message() {
    fn literal() {
        panic!("a defect of the run");
    }
    fn formatted() {
        let shot = 3;
        panic!("a defect of shot {shot}");
    }
    let cases: [(&str, fn(), &str); 2] = [
        ("literal", literal, "a defect of the run"),
        ("formatted", formatted, "a defect of shot 3"),
    ];
    for (case, defect, expected) in cases {
        let failure = std::thread::scope(|scope| panicked_run(scope.spawn(defect)));
        assert!(
            matches!(&failure, RunFailure::Runtime(message) if message.contains(expected)),
            "{case}: a run that panics is a RuntimeError with the panic's message: {failure:?}"
        );
    }
}
