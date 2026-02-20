// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_classical_int_assign_to_local() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable i = 0;
        set i = 1;
        i"#,
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
fn check_rca_for_dynamic_result_assign_to_local() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = Zero;
        set r = M(q);
        r"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_bool_assign_to_local() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        use q = Qubit();
        mutable b = false;
        set b = ResultAsBool(M(q));
        b"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_int_assign_to_local() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        mutable i = 0;
        set i = ResultArrayAsInt(results);
        i"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_double_assign_to_local() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        let i = ResultArrayAsInt(results);
        mutable d = 0.0;
        set d = IntAsDouble(i);
        d"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicDouble)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn chec_rca_for_assign_call_result_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo() : (Int, Int) {
            return (1,2);
        }
        mutable a = 1;
        mutable b = 2;
        set (a, b) = Foo();
        "#,
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
fn chec_rca_for_assign_var_binded_to_call_result_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo() : (Int, Int) {
            return (1,2);
        }
        let x = Foo();
        mutable a = 1;
        mutable b = 2;
        set (a, b) = x;
        "#,
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
fn chec_rca_for_assign_tuple_var_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = (1, (2, 3));
        mutable a = 4;
        mutable b = (5, 6);
        set (a, b) = x;
        "#,
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
fn check_rca_for_assign_classical_call_result_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(a : Int, b : Int) : (Int, Int) {
            return (b, a);
        }
        mutable a = 1;
        mutable b = 2;
        set (a, b) = Foo(a, b);
        a
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Classical
                dynamic_param_applications: <empty>"#]],
    );
    compilation_context.update(
        r#"
        b
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Classical
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_assign_dynamic_call_result_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(a : Int, b : Int) : (Int, Int) {
            return (b, a);
        }
        use q = Qubit();
        let r = MResetZ(q);
        mutable a = r == Zero ? 0 | 1;
        mutable b = 2;
        set (a, b) = Foo(a, b);
        a
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
    compilation_context.update(
        r#"
        b
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_assign_dynamic_static_mix_call_result_to_tuple_of_vars() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : (Int[], Result[]) {
            ([1, 2], [MResetZ(q)])
        }
        use q = Qubit();
        mutable (a, b) = Foo(q);
        set (a, b) = Foo(q);
        a
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
    compilation_context.update(
        r#"
        b
        "#,
    );
    check_last_statement_compute_properties(
        compilation_context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_classical_integer_assigned_updated_with_classical_integer() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        mutable i = 0;
        set i += 1;
        i"#,
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
fn check_rca_for_mutable_classical_integer_assigned_updated_with_dynamic_integer() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        mutable i = 0;
        set i += ResultArrayAsInt(results);
        i"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_dynamic_integer_assigned_updated_with_classical_integer() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        mutable i = ResultArrayAsInt(results);
        set i += 1;
        i"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_dynamic_integer_assigned_updated_with_dynamic_integer() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        mutable i = ResultArrayAsInt(results);
        set i += ResultArrayAsInt(results);
        i"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_dynamic_result_assigned_updated_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = Zero;
        set r = M(q);
        if r == Zero {
            set r = One;
        }
        r
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicResult)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_dynamic_result_bound_to_dynamic_result() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let r = M(q) == One ? Zero | One;
        r
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicResult)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_dynamic_result_bound_to_result_from_classical_conditional() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let r = if One == One {
            M(q)
        } else {
            use q2 = Qubit();
            M(q2)
        };
        r
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_dynamic_result_bound_to_call_with_dynamic_args() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Check(r : Int) : Result {
            if r == 1 {
                Zero
            } else {
                One
            }
        }
        use q = Qubit();
        let r = Check(M(q) == One ? 1 | 0);
        r
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicResult)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_tuple_assigned_updated_in_static_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable x = (1, 2);
        if false {
            set x = (2, 3);
        }
        x
        "#,
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
fn check_rca_for_mutable_tuple_assigned_updated_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable x = (1, 2);
        if M(q) == One {
            set x = (2, 3);
        }
        x
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicInt | UseOfDynamicTuple)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_tuple_bound_to_dynamic_tuple() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let x = M(q) == One ? (1, 2) | (2, 3);
        x
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicTuple)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_tuple_bound_to_tuple_from_classical_conditional() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let x = if One == One {
            (1, 2)
        } else {
            (2, 3)
        };
        x
        "#,
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
