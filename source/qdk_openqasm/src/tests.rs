// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::Expect;
use miette::Report;

use crate::io::InMemorySourceResolver;
use crate::semantic::AnalysisResult;
use crate::semantic::parse_source;
use std::sync::Arc;

/// Serializes panic-hook swaps performed by [`assert_panics_with`] so that
/// concurrently running tests don't observe (or restore) each other's
/// temporary silent hook.
static PANIC_HOOK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `operation`, asserting that it panics with a message containing
/// `expected_substring`.
///
/// The default panic hook is suppressed for the duration of the call, so an
/// expected panic does not clutter test output with `thread '...' panicked`
/// banners or backtraces. Prefer this over `#[should_panic]` for tests that
/// deliberately trigger invariant panics.
pub(crate) fn assert_panics_with(expected_substring: &str, operation: impl FnOnce()) {
    let _hook_guard = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    std::panic::set_hook(previous_hook);

    let payload =
        result.expect_err("expected the operation to panic, but it completed without panicking");
    let message = match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "(non-string panic payload)".to_string(),
        },
    };
    assert!(
        message.contains(expected_substring),
        "panic message did not contain the expected substring.\n  expected substring: {expected_substring}\n  actual message: {message}"
    );
}

pub(crate) fn parse<S: Into<Arc<str>>>(source: S) -> miette::Result<AnalysisResult, Vec<Report>> {
    let source = source.into();
    let name: Arc<str> = "Test.qasm".into();
    let sources = [(name.clone(), source.clone())];
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let res = parse_source(source, name, &mut resolver);
    if res.source.has_errors() {
        let errors = res
            .all_errors()
            .into_iter()
            .map(|e| Report::new(e.clone()))
            .collect();
        return Err(errors);
    }
    Ok(res)
}

pub fn check_qasm<S: Into<Arc<str>>>(source: S, expect: &Expect) {
    match parse(source) {
        Ok(res) => {
            // syntax succeeded, check for semantic errors
            if res.has_errors() {
                let buffer = res
                    .errors
                    .into_iter()
                    .map(Report::new)
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                expect.assert_eq(&buffer);
            } else {
                panic!("Expected errors but parsing succeeded with program: {res:?}");
            }
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}
