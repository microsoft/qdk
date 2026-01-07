// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_struct_constructor_with_classical_values() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        new Complex { Real = 0.0, Imag = 0.0 }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Classical
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_struct_constructor_with_a_dynamic_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use q = Qubit();
        let r = M(q) == Zero ? 0.0 | 1.0;
        new Complex { Real = r, Imag = 0.0 }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble | UseOfDynamicUdt)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_struct_copy_constructor_with_classical_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        let c = new Complex { Real = 0.0, Imag = 0.0 };
        new Complex { ...c, Real = 1.0 }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Classical
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_struct_copy_constructor_with_dynamic_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        let c = new Complex { Real = 0.0, Imag = 0.0 };
        use q = Qubit();
        let i = M(q) == Zero ? 0.0 | 1.0;
        new Complex { ...c, Imag = i }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble | UseOfDynamicUdt)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_struct_dynamic_constructor_overwritten_with_classic_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use q = Qubit();
        let i = M(q) == Zero ? 0.0 | 1.0;
        let c = new Complex { Real = 0.0, Imag = i };
        new Complex { ...c, Imag = 0.0 }"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble | UseOfDynamicUdt)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}
