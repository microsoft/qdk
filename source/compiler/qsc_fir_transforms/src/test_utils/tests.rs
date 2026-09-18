// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::any::Any;

use super::*;

fn panic_message(panic: Box<dyn Any + Send>) -> String {
    match panic.downcast::<String>() {
        Ok(message) => *message,
        Err(panic) => match panic.downcast::<&str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "(non-string panic payload)".to_string(),
        },
    }
}

/// The error-returning runner must still surface fatal backstops after
/// convergence failures are deferred.
#[test]
fn staged_runner_with_errors_returns_defunctionalization_diagnostics() {
    let source = r#"
        operation ApplyTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            for op in firstOps { op(q); }
            for op in secondOps { op(q); }
        }
        operation ForwardTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            ApplyTwoArrays(firstOps, secondOps, q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ForwardTwoArrays([X, Y], [Z, H], q);
        }
    "#;

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::Full);

    assert!(
        !result.errors.is_empty(),
        "expected defunctionalization diagnostics to be returned"
    );
    let messages = result
        .errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        messages.contains(
            "higher-order function forwards more than one callable array, which is not supported"
        ),
        "unexpected diagnostics: {messages}"
    );
}

/// The checked runner must still panic for fatal backstops after convergence
/// failures are deferred.
#[test]
fn checked_staged_runner_panics_on_unexpected_defunctionalization_diagnostics() {
    let source = r#"
        operation ApplyTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            for op in firstOps { op(q); }
            for op in secondOps { op(q); }
        }
        operation ForwardTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            ApplyTwoArrays(firstOps, secondOps, q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ForwardTwoArrays([X, Y], [Z, H], q);
        }
    "#;

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = compile_and_run_pipeline_to(source, PipelineStage::Full);
    }))
    .expect_err("checked staged runner should panic on unexpected diagnostics");
    let message = panic_message(panic);
    assert!(
        message.contains("compile_and_run_pipeline_to produced FIR transform pipeline errors"),
        "unexpected panic: {message}"
    );
    assert!(
        message.contains(
            "higher-order function forwards more than one callable array, which is not supported"
        ),
        "unexpected panic: {message}"
    );
}

#[test]
fn reachable_callable_details_report_body_shape() {
    let source = r#"
        namespace Test {
            function Helper(x : Int) : Int { x + 1 }

            @EntryPoint()
            function Main() : Int {
                Helper(2)
            }
        }
    "#;

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let summary = extract_reachable_callable_details(&store, pkg_id);

    assert!(
        summary.contains("callable Helper: input_ty=Int, output_ty=Int"),
        "unexpected summary: {summary}"
    );
    assert!(
        summary.contains("callable Main: input_ty=Unit, output_ty=Int"),
        "unexpected summary: {summary}"
    );
    assert!(
        summary.contains("body: block_ty=Int"),
        "unexpected summary: {summary}"
    );

    assert_callable_body_terminal_expr_matches_block_type(&store, pkg_id, "Helper");
    assert_callable_body_terminal_expr_matches_block_type(&store, pkg_id, "Main");
}
