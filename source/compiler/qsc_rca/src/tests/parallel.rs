// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    CompilationContext, check_callable_compute_properties, check_last_statement_compute_properties,
};
use expect_test::expect;

#[test]
fn check_rca_for_parallel_expr_with_static_body() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let e = parallel { };
        e"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_parallel_within_with_static_limit_and_body() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let e = parallel within 4 { };
        e"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_parallel_with_dynamic_operations_no_branching() {
    // A parallel body that allocates a qubit, applies gates, and measures — making the overall
    // operation dynamic — but contains no conditional branching and therefore
    // does NOT produce UseOfDynamicBranchingInParallelExpr.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            let _ = parallel {
                use p = Qubit();
                H(p);
                M(p)
            };
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_within_with_dynamic_operations_no_branching() {
    // Same validation as above but using `parallel within` with a static limit.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            let _ = parallel within 4 {
                use p = Qubit();
                H(p);
                M(p)
            };
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_with_dynamic_if_in_body() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            parallel {
                if M(q) == Zero {
                    H(q);
                }
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_with_short_circuit_bool_in_body() {
    // Short-circuiting `&&`/`||` with a variable LHS incurs dynamic branching in code gen.
    // When inside a parallel expression, this triggers UseOfDynamicBranchingInParallelExpr.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            parallel {
                let b = (M(q) == Zero) and (M(q) == One);
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_with_while_loop_with_dynamic_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            parallel {
                while M(q) == Zero {
                    H(q);
                }
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_within_with_dynamic_limit() {
    // The UseOfDynamicLimitInParallelExpr runtime feature is stored on the limit expression's
    // compute kind by the RCA, but is not propagated to the callable-level compute properties.
    // The callable-level features reflect only the dynamic values used in the body (Bool and Int
    // from the conditional). The UseOfDynamicLimitInParallelExpr flag is checked and surfaced as
    // an error by the capabilities check pass (see capabilitiesck tests).
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            let n = M(q) == Zero ? 2 | 4;
            parallel within n { }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_nested_parallel_with_dynamic_if_in_inner_body() {
    // Dynamic branching in the inner parallel propagates out to the outer parallel's compute kind
    // since the outer parallel's compute kind is derived from its body.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            use q = Qubit();
            parallel {
                parallel {
                    if M(q) == Zero {
                        H(q);
                    }
                }
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_calling_operation_that_branches_dynamically() {
    // Bar measures a qubit and branches on the result, making it dynamic with UseOfDynamicBool.
    // When Foo calls Bar inside a parallel expression, the RCA detects that the call involves a
    // dynamic bool and adds UseOfDynamicBranchingInParallelExpr to Foo's compute properties.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Bar(q : Qubit) : Unit {
            if M(q) == Zero {
                H(q);
            }
        }
        operation Foo() : Unit {
            use q = Qubit();
            parallel {
                Bar(q);
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Bar",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                        value_kind: Constant
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] ElementParamApplication:
                            constant: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                                value_kind: Constant
                            variable: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicQubit)
                                value_kind: Constant
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_within_calling_operation_that_branches_dynamically() {
    // Same as above but using `parallel within` with a static limit.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Bar(q : Qubit) : Unit {
            if M(q) == Zero {
                H(q);
            }
        }
        operation Foo() : Unit {
            use q = Qubit();
            parallel within 4 {
                Bar(q);
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Bar",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                        value_kind: Constant
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] ElementParamApplication:
                            constant: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                                value_kind: Constant
                            variable: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicQubit)
                                value_kind: Constant
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBranchingInParallelExpr)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_parallel_with_dynamic_arg_to_rotation_does_not_branch() {
    // A dynamic Double computed outside the parallel expression can be freely used as an
    // argument to a gate inside it. Passing a dynamic value to a call does not incur branching,
    // so UseOfDynamicBranchingInParallelExpr must NOT appear in the compute properties.
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Unit {
            import Std.Convert.*;
            use q = Qubit();
            let angle = M(q) == Zero ? 1.0 | 2.0;
            parallel {
                use p = Qubit();
                Rx(angle, p);
            }
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        package_store_compute_properties,
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}
