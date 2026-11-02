// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn evaluation_order_short_circuit_rhs_assignments_have_dynamic_scope() {
    for statement in [
        "let _ = (M(q) == One) and { set v = 0; false };",
        "let _ = (M(q) == Zero) or { set v = 0; true };",
        "mutable b = M(q) == One; set b and= { set v = 0; false };",
        "mutable b = M(q) == Zero; set b or= { set v = 0; true };",
    ] {
        let mut context = CompilationContext::default();
        context.update(&format!(
            "use q = Qubit(); mutable v = 4; {statement} v == 4"
        ));
        check_last_statement_compute_properties(
            context.get_compute_properties(),
            &expect![[r#"
                ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                        value_kind: Variable
                    dynamic_param_applications: <empty>"#]],
        );
    }
}

#[test]
fn check_rca_for_bin_op_with_classical_lhs_and_classical_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"1 + 1"#);
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
fn check_rca_for_bin_op_with_dynamic_lhs_and_classical_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        M(q) != Zero"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_bin_op_with_classical_lhs_and_dynamic_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        One == M(q)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_bin_op_with_dynamic_lhs_and_dynamic_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use (a, b) = (Qubit(), Qubit());
        let (c, d) = (M(a) == Zero, M(b) == Zero);
        c and d"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_nested_bin_ops_with_classic_operands() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        Sin(PI() / 2.0) ^ 2.0 + Cos(PI() / 2.0) ^ 2.0"#,
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
fn check_rca_for_nested_bin_ops_with_a_dynamic_operand() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let i = M(q) == Zero ? 0 | 1;
        i * 1 / 1"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_exp_op_with_classical_lhs_and_classical_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"2 ^ 3"#);
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
fn check_rca_for_exp_op_with_dynamic_lhs_and_classical_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"{
            use q = Qubit();
            let i = M(q) == Zero ? 0 | 1;
            i ^ 2
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_exp_op_with_classical_lhs_and_dynamic_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"{
            use q = Qubit();
            let i = M(q) == Zero ? 0 | 1;
            2 ^ i
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicExponent | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_exp_op_with_dynamic_lhs_and_dynamic_rhs() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"{
            use (a, b) = (Qubit(), Qubit());
            let (c, d) = (M(a) == Zero, M(b) == Zero);
            let i = c ? 0 | 1;
            let j = d ? 0 | 1;
            i ^ j
        }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicExponent | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}
