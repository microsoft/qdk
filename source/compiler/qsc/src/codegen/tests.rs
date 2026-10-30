// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

mod adaptive_profile;
mod adaptive_ri_profile;
mod adaptive_rif_profile;
mod base_profile;
mod callable_evaluation;

use std::{io::Cursor, rc::Rc, sync::Arc};

use expect_test::{Expect, expect};
use miette::Report;
use qsc_data_structures::{
    functors::FunctorApp,
    language_features::LanguageFeatures,
    source::SourceMap,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_eval::output::CursorReceiver;
use qsc_eval::val::Value;
use qsc_fir::fir::{
    ExprKind as FirExprKind, ItemKind as FirItemKind, PackageLookup, Res as FirRes, StoreItemId,
};
use qsc_frontend::compile::parse_all;
use qsc_hir::hir::{ItemKind, PackageId};
use qsc_passes::{PackageType, lower_hir_to_fir};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::codegen::qir::{
    CallableArgsBackend, get_qir, get_qir_from_ast, get_rir, prepare_codegen_fir_from_callable_args,
};

use super::qir::seed_entry_with_callables;

fn format_interpret_errors(errors: Vec<crate::interpret::Error>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{:?}", Report::new(error)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn source_map_from_source(source: &str) -> SourceMap {
    SourceMap::new([("test.qs".into(), source.into())], None)
}

fn parse_source_to_ast(source: &str) -> (qsc_ast::ast::Package, SourceMap) {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (ast_package, errors) = parse_all(&sources, language_features);

    if errors.is_empty() {
        (ast_package, sources)
    } else {
        let diagnostics = errors
            .into_iter()
            .map(|error| format!("{:?}", Report::new(error)))
            .collect::<Vec<_>>()
            .join("\n\n");

        panic!("Failed to parse AST test source:\n{diagnostics}");
    }
}

fn compile_source_to_qir(source: &str, capabilities: TargetCapabilityFlags) -> String {
    match compile_source_to_qir_result(source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();

    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    )
}

fn qir_entry_body(qir: &str) -> &str {
    qir.split_once("define i64 @ENTRYPOINT__main()")
        .expect("entry point should exist")
        .1
        .split_once("{\n")
        .expect("entry body should exist")
        .1
        .split_once("\n}")
        .expect("entry body should terminate")
        .0
}

/// Lower profiles inline preparation without changing SSA IDs. Check the gate
/// and its position before removing it to share the unprepared snapshot.
/// Full Adaptive emits a separate call and needs its own prepared-entry snapshot.
fn assert_evaluation_order_qir(
    qir: &str,
    profile: Profile,
    prepared: bool,
    ssa: &Expect,
    adaptive: &Expect,
    prepared_adaptive: &Expect,
) {
    let mut entry = qir_entry_body(qir).to_string();
    if prepared && profile != Profile::Adaptive {
        let gate = "  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))\n";
        assert_eq!(entry.matches(gate).count(), 1, "{profile:?}:\n{entry}");
        assert!(
            entry.contains(&format!("{gate}  call void @__quantum__qis__mresetz__body")),
            "preparation must immediately precede measurement, {profile:?}:\n{entry}"
        );
        entry = entry.replacen(gate, "", 1);
    }
    let snapshot = if profile == Profile::Adaptive {
        if prepared {
            prepared_adaptive
        } else {
            adaptive
        }
    } else {
        ssa
    };
    snapshot.assert_eq(&entry);
}

#[test]
fn callable_assignment_expression_cleanup_generates_expected_qir_on_every_profile() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        operation Main() : Result {
            mutable f = A;
            let ignored = (set f = B);
            use q = Qubit();
            if f(10) == 12 { X(q); }
            MResetZ(q)
        }
    "#;
    let expected = r#"
        operation Main() : Result {
            use q = Qubit();
            X(q);
            MResetZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        assert_eq!(
            compile_source_to_qir(source, profile.into()),
            compile_source_to_qir(expected, profile.into()),
            "{profile:?}: removing the assignment must preserve the selected callable",
        );
    }
}

#[test]
fn evaluation_order_logical_and_rhs_assignment_remains_conditional() {
    check_conditional_rhs_assignment(
        "let _ = (MResetZ(q) == One) and { set v = 0; false };",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_2
            block_2:
              %var_8 = phi i64 [4, %block_0], [0, %block_1]
              %var_6 = icmp eq i64 %var_8, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_6, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i64
              %var_5 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 4, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 0, ptr %var_0
              br label %block_2
            block_2:
              %var_8 = load i64, ptr %var_0
              store i64 %var_8, ptr %var_5
              %var_10 = load i64, ptr %var_5
              %var_6 = icmp eq i64 %var_10, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_6, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_1 = alloca i64
              %var_6 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              store i64 4, ptr %var_1
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 0, ptr %var_1
              br label %block_2
            block_2:
              %var_9 = load i64, ptr %var_1
              store i64 %var_9, ptr %var_6
              %var_11 = load i64, ptr %var_6
              %var_7 = icmp eq i64 %var_11, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_7, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_logical_or_rhs_assignment_remains_conditional() {
    check_conditional_rhs_assignment(
        "let _ = (MResetZ(q) == Zero) or { set v = 0; true };",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_2, label %block_1
            block_1:
              br label %block_2
            block_2:
              %var_8 = phi i64 [4, %block_0], [0, %block_1]
              %var_6 = icmp eq i64 %var_8, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_6, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i64
              %var_5 = alloca i64
              %var_7 = alloca i1
              call void @__quantum__rt__initialize(ptr null)
              store i64 4, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_2, label %block_1
            block_1:
              store i64 0, ptr %var_0
              br label %block_2
            block_2:
              %var_10 = load i64, ptr %var_0
              store i64 %var_10, ptr %var_5
              %var_12 = load i64, ptr %var_5
              %var_6 = icmp eq i64 %var_12, 4
              store i1 %var_6, ptr %var_7
              %var_14 = load i1, ptr %var_7
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_14, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_1 = alloca i64
              %var_6 = alloca i64
              %var_8 = alloca i1
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              store i64 4, ptr %var_1
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_3 = icmp eq i1 %var_2, false
              br i1 %var_3, label %block_2, label %block_1
            block_1:
              store i64 0, ptr %var_1
              br label %block_2
            block_2:
              %var_11 = load i64, ptr %var_1
              store i64 %var_11, ptr %var_6
              %var_13 = load i64, ptr %var_6
              %var_7 = icmp eq i64 %var_13, 4
              store i1 %var_7, ptr %var_8
              %var_15 = load i1, ptr %var_8
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_15, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_compound_and_rhs_assignment_remains_conditional() {
    check_conditional_rhs_assignment(
        "mutable b = MResetZ(q) == One; set b and= { set v = 0; false };",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_2
            block_2:
              %var_9 = phi i64 [4, %block_0], [0, %block_1]
              %var_7 = icmp eq i64 %var_9, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_7, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i64
              %var_3 = alloca i1
              %var_5 = alloca i1
              %var_6 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 4, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 false, ptr %var_5
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 0, ptr %var_0
              store i1 false, ptr %var_5
              br label %block_2
            block_2:
              %var_10 = load i1, ptr %var_5
              store i1 %var_10, ptr %var_3
              %var_12 = load i64, ptr %var_0
              store i64 %var_12, ptr %var_6
              %var_14 = load i64, ptr %var_6
              %var_7 = icmp eq i64 %var_14, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_7, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_1 = alloca i64
              %var_4 = alloca i1
              %var_6 = alloca i1
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              store i64 4, ptr %var_1
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 false, ptr %var_6
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 0, ptr %var_1
              store i1 false, ptr %var_6
              br label %block_2
            block_2:
              %var_11 = load i1, ptr %var_6
              store i1 %var_11, ptr %var_4
              %var_13 = load i64, ptr %var_1
              store i64 %var_13, ptr %var_7
              %var_15 = load i64, ptr %var_7
              %var_8 = icmp eq i64 %var_15, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_8, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_compound_or_rhs_assignment_remains_conditional() {
    check_conditional_rhs_assignment(
        "mutable b = MResetZ(q) == Zero; set b or= { set v = 0; true };",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_2, label %block_1
            block_1:
              br label %block_2
            block_2:
              %var_9 = phi i64 [4, %block_0], [0, %block_1]
              %var_7 = icmp eq i64 %var_9, 4
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_7, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i64
              %var_3 = alloca i1
              %var_5 = alloca i1
              %var_6 = alloca i64
              %var_8 = alloca i1
              call void @__quantum__rt__initialize(ptr null)
              store i64 4, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              store i1 true, ptr %var_5
              br i1 %var_2, label %block_2, label %block_1
            block_1:
              store i64 0, ptr %var_0
              store i1 true, ptr %var_5
              br label %block_2
            block_2:
              %var_13 = load i1, ptr %var_5
              store i1 %var_13, ptr %var_3
              %var_15 = load i64, ptr %var_0
              store i64 %var_15, ptr %var_6
              %var_17 = load i64, ptr %var_6
              %var_7 = icmp eq i64 %var_17, 4
              store i1 %var_7, ptr %var_8
              %var_19 = load i1, ptr %var_8
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_19, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_1 = alloca i64
              %var_4 = alloca i1
              %var_6 = alloca i1
              %var_7 = alloca i64
              %var_9 = alloca i1
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              store i64 4, ptr %var_1
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_3 = icmp eq i1 %var_2, false
              store i1 true, ptr %var_6
              br i1 %var_3, label %block_2, label %block_1
            block_1:
              store i64 0, ptr %var_1
              store i1 true, ptr %var_6
              br label %block_2
            block_2:
              %var_14 = load i1, ptr %var_6
              store i1 %var_14, ptr %var_4
              %var_16 = load i64, ptr %var_1
              store i64 %var_16, ptr %var_7
              %var_18 = load i64, ptr %var_7
              %var_8 = icmp eq i64 %var_18, 4
              store i1 %var_8, ptr %var_9
              %var_20 = load i1, ptr %var_9
              call void @__quantum__rt__bool_record_output(i1 zeroext %var_20, ptr @0)
              ret i64 0"#]],
    );
}

fn check_conditional_rhs_assignment(
    statement: &str,
    ssa: &Expect,
    adaptive: &Expect,
    prepared_adaptive: &Expect,
) {
    for prepare in ["", "X(q);"] {
        let source = format!(
            "operation Main() : Bool {{
                use q = Qubit();
                {prepare}
                mutable v = 4;
                {statement}
                v == 4
            }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Bool(prepare.is_empty())
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert_evaluation_order_qir(
                &qir,
                profile,
                !prepare.is_empty(),
                ssa,
                adaptive,
                prepared_adaptive,
            );
        }
    }
}

#[test]
fn evaluation_order_static_short_circuit_and_eager_equality_are_preserved() {
    for (statement, expected) in [
        ("let _ = false and { set v = 0; true };", 4),
        ("let _ = true or { set v = 0; false };", 4),
        ("let _ = true and { set v = 0; true };", 0),
        ("let _ = false or { set v = 0; false };", 0),
        ("let _ = (MResetZ(q) == One) == { set v = 0; false };", 0),
        ("let _ = (MResetZ(q) == One) != { set v = 0; false };", 0),
    ] {
        let source = format!(
            "operation Main() : Int {{
                use q = Qubit();
                mutable v = 4;
                {statement}
                v
            }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Int(expected)
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert!(
                qir.contains(&format!(
                    "call void @__quantum__rt__int_record_output(i64 {expected},"
                )),
                "{profile:?}, {statement}: {qir}"
            );
        }
    }
}

fn check_unit_comparison_qir(
    logical: &str,
    comparison: &str,
    comparison_result: bool,
    expected: [(bool, i64); 2],
) {
    let comparison_expression =
        format!("((set trace = 10 * trace + 1) {comparison} (set trace = 10 * trace + 2))");
    let reference_expression = format!(
        "{{ set trace = 10 * trace + 1; set trace = 10 * trace + 2; {comparison_result} }}"
    );
    for (prepare, (expected_comparison, expected_trace)) in ["", "X(q);"].into_iter().zip(expected)
    {
        let source = |expression: &str| {
            format!(
                "operation Main() : (Bool, Int) {{
                    use q = Qubit();
                    {prepare}
                    mutable trace = 0;
                    let condition = MResetZ(q) == One;
                    let comparison = condition {logical} {expression};
                    (comparison, trace)
                }}"
            )
        };
        let actual_source = source(&comparison_expression);
        let reference_source = source(&reference_expression);
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &actual_source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Tuple(
                vec![Value::Bool(expected_comparison), Value::Int(expected_trace)].into(),
                None,
            ),
            "{logical}, {comparison}, prepare={prepare:?}: source result"
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            assert_eq!(
                compile_source_to_qir(&actual_source, profile.into()),
                compile_source_to_qir(&reference_source, profile.into()),
                "{profile:?}, {logical}, {comparison}, prepare={prepare:?}: operand effects must remain ordered and conditional"
            );
        }
    }
}

#[test]
fn unit_equality_in_and_preserves_conditional_operand_effects() {
    check_unit_comparison_qir("and", "==", true, [(false, 0), (true, 12)]);
}

#[test]
fn unit_inequality_in_and_preserves_conditional_operand_effects() {
    check_unit_comparison_qir("and", "!=", false, [(false, 0), (false, 12)]);
}

#[test]
fn unit_equality_in_or_preserves_conditional_operand_effects() {
    check_unit_comparison_qir("or", "==", true, [(true, 12), (true, 0)]);
}

#[test]
fn unit_inequality_in_or_preserves_conditional_operand_effects() {
    check_unit_comparison_qir("or", "!=", false, [(false, 12), (true, 0)]);
}

#[test]
fn unit_comparisons_preserve_quantum_effects_on_every_profile() {
    for (comparison, expected_gate) in [("==", "X(q);"), ("!=", "")] {
        let source = format!(
            "operation Main() : Result {{
                use q = Qubit();
                if X(q) {comparison} Reset(q) {{ X(q); }}
                MResetZ(q)
            }}"
        );
        let reference = format!(
            "operation Main() : Result {{
                use q = Qubit();
                X(q);
                Reset(q);
                {expected_gate}
                MResetZ(q)
            }}"
        );
        for profile in [
            Profile::Base,
            Profile::AdaptiveRI,
            Profile::AdaptiveRIF,
            Profile::Adaptive,
        ] {
            assert_eq!(
                compile_source_to_qir(&source, profile.into()),
                compile_source_to_qir(&reference, profile.into()),
                "{profile:?}, {comparison}: both Unit operands must execute before branching"
            );
        }
    }
}

#[test]
fn unwrapped_unit_comparisons_match_interpreter_and_qir() {
    for effects in ["", "X(q); Reset(q);"] {
        for (comparison, expected_result, expected_gate) in
            [("==", true, "X(q);"), ("!=", false, "")]
        {
            let source = format!(
                "newtype UnitBox = Unit;
                 operation Main() : Result {{
                     use q = Qubit();
                     let wrapped = UnitBox();
                     if ({{ {effects} wrapped! }}) {comparison} () {{ X(q); }}
                     MResetZ(q)
                 }}"
            );
            let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
            eval_fragments(&mut interpreter, &source);
            assert_eq!(
                eval_fragments(&mut interpreter, "Main()"),
                Value::Result(qsc_eval::val::Result::Val(expected_result)),
                "{comparison}, effects={effects:?}: compare the unwrapped payload"
            );
            let reference = format!(
                "operation Main() : Result {{
                    use q = Qubit();
                    {effects}
                    {expected_gate}
                    MResetZ(q)
                }}"
            );
            for profile in [
                Profile::Base,
                Profile::AdaptiveRI,
                Profile::AdaptiveRIF,
                Profile::Adaptive,
            ] {
                assert_eq!(
                    compile_source_to_qir(&source, profile.into()),
                    compile_source_to_qir(&reference, profile.into()),
                    "{profile:?}, {comparison}, effects={effects:?}: preserve effects and comparison result"
                );
            }
        }
    }
}

#[test]
fn evaluation_order_short_circuit_double_assignment_then_arithmetic() {
    let template = r#"
        operation Main() : Double {
            use q = Qubit();
            mutable v = 4.0;
            let _ = (MResetZ(q) == One) and { set v = 0.0; false };
            v / 2.0
        }
    "#;
    let ssa = expect![[r#"
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          br label %block_2
        block_2:
          %var_8 = phi double [4.0, %block_0], [0.0, %block_1]
          %var_6 = fdiv double %var_8, 2.0
          call void @__quantum__rt__double_record_output(double %var_6, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0"#]];
    let adaptive = expect![[r#"
        block_0:
          %var_0 = alloca double
          %var_5 = alloca double
          call void @__quantum__rt__initialize(ptr null)
          store double 4.0, ptr %var_0
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store double 0.0, ptr %var_0
          br label %block_2
        block_2:
          %var_8 = load double, ptr %var_0
          store double %var_8, ptr %var_5
          %var_10 = load double, ptr %var_5
          %var_6 = fdiv double %var_10, 2.0
          call void @__quantum__rt__double_record_output(double %var_6, ptr @0)
          ret i64 0"#]];
    let prepared_adaptive = expect![[r#"
        block_0:
          %var_1 = alloca double
          %var_6 = alloca double
          call void @__quantum__rt__initialize(ptr null)
          call void @X(ptr inttoptr (i64 0 to ptr))
          store double 4.0, ptr %var_1
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          store double 0.0, ptr %var_1
          br label %block_2
        block_2:
          %var_9 = load double, ptr %var_1
          store double %var_9, ptr %var_6
          %var_11 = load double, ptr %var_6
          %var_7 = fdiv double %var_11, 2.0
          call void @__quantum__rt__double_record_output(double %var_7, ptr @0)
          ret i64 0"#]];
    for (prepare, result) in [("", 2.0), ("X(q);", 0.0)] {
        let source = template.replace("use q = Qubit();", &format!("use q = Qubit(); {prepare}"));
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Double(result)
        );
        for profile in [Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert_evaluation_order_qir(
                &qir,
                profile,
                !prepare.is_empty(),
                &ssa,
                &adaptive,
                &prepared_adaptive,
            );
        }
    }
}

#[test]
fn evaluation_order_array_append_reads_target_after_rhs() {
    let source = r#"
        operation Main() : Int {
            use q = Qubit();
            mutable values = [4];
            set values += { set values = [7]; [if MResetZ(q) == Zero { 1 } else { 2 }] };
            values[0]
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
    eval_fragments(&mut interpreter, source);
    assert_eq!(eval_fragments(&mut interpreter, "Main()"), Value::Int(7));
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("call void @__quantum__rt__int_record_output(i64 7,"),
            "{profile:?}: append must read the reassigned target:\n{qir}"
        );
    }
}

#[test]
fn evaluation_order_array_concatenation_snapshots_left_operand_before_rhs_mutation() {
    check_array_operand_snapshot(
        "(values + { set values w/= 0 <- 0; [] })[0]",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_5 = phi i64 [4, %block_1], [5, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i64
              %var_3 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_2
              br label %block_3
            block_2:
              store i64 5, ptr %var_2
              br label %block_3
            block_3:
              %var_5 = load i64, ptr %var_2
              store i64 %var_5, ptr %var_3
              %var_7 = load i64, ptr %var_3
              call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_3 = alloca i64
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_3
              br label %block_3
            block_2:
              store i64 5, ptr %var_3
              br label %block_3
            block_3:
              %var_6 = load i64, ptr %var_3
              store i64 %var_6, ptr %var_4
              %var_8 = load i64, ptr %var_4
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_array_indexing_snapshots_array_before_index_mutation() {
    check_array_operand_snapshot(
        "values[{ set values w/= 0 <- 0; 0 }]",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_5 = phi i64 [4, %block_1], [5, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i64
              %var_3 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_2
              br label %block_3
            block_2:
              store i64 5, ptr %var_2
              br label %block_3
            block_3:
              %var_5 = load i64, ptr %var_2
              store i64 %var_5, ptr %var_3
              %var_7 = load i64, ptr %var_3
              call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_3 = alloca i64
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_3
              br label %block_3
            block_2:
              store i64 5, ptr %var_3
              br label %block_3
            block_3:
              %var_6 = load i64, ptr %var_3
              store i64 %var_6, ptr %var_4
              %var_8 = load i64, ptr %var_4
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_array_repetition_snapshots_value_before_size_mutation() {
    check_array_operand_snapshot(
        "[values, size = { set values w/= 0 <- 0; 2 }][0][0]",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_5 = phi i64 [4, %block_1], [5, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i64
              %var_3 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_2
              br label %block_3
            block_2:
              store i64 5, ptr %var_2
              br label %block_3
            block_3:
              %var_5 = load i64, ptr %var_2
              store i64 %var_5, ptr %var_3
              %var_7 = load i64, ptr %var_3
              call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_3 = alloca i64
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_3
              br label %block_3
            block_2:
              store i64 5, ptr %var_3
              br label %block_3
            block_3:
              %var_6 = load i64, ptr %var_3
              store i64 %var_6, ptr %var_4
              %var_8 = load i64, ptr %var_4
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_array_literal_snapshots_element_before_later_element_mutation() {
    check_array_operand_snapshot(
        "[values, { set values w/= 0 <- 0; [] }][0][0]",
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_5 = phi i64 [4, %block_1], [5, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i64
              %var_3 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_2
              br label %block_3
            block_2:
              store i64 5, ptr %var_2
              br label %block_3
            block_3:
              %var_5 = load i64, ptr %var_2
              store i64 %var_5, ptr %var_3
              %var_7 = load i64, ptr %var_3
              call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_3 = alloca i64
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %var_2 = icmp eq i1 %var_1, false
              br i1 %var_2, label %block_1, label %block_2
            block_1:
              store i64 4, ptr %var_3
              br label %block_3
            block_2:
              store i64 5, ptr %var_3
              br label %block_3
            block_3:
              %var_6 = load i64, ptr %var_3
              store i64 %var_6, ptr %var_4
              %var_8 = load i64, ptr %var_4
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0"#]],
    );
}

fn check_array_operand_snapshot(
    expression: &str,
    ssa: &Expect,
    adaptive: &Expect,
    prepared_adaptive: &Expect,
) {
    for (prepare, result) in [("", 4), ("X(q);", 5)] {
        let source = format!(
            "operation Main() : Int {{
                use q = Qubit();
                {prepare}
                mutable values = [if MResetZ(q) == Zero {{ 4 }} else {{ 5 }}];
                {expression}
            }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Int(result)
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert_evaluation_order_qir(
                &qir,
                profile,
                !prepare.is_empty(),
                ssa,
                adaptive,
                prepared_adaptive,
            );
        }
    }
}

/// The source interpreter supplies one oracle; the complete QIR entry snapshot
/// separately connects measurement branches, saved values and effects to output.
fn check_evaluation_order_callable_residue(
    body: &str,
    expected: [i64; 2],
    prepared_adaptive: &Expect,
    ssa: &Expect,
    adaptive: &Expect,
) {
    for (prepare, expected) in ["", "X(q);"].into_iter().zip(expected) {
        let source = format!(
            "function F(x : Int) : Int {{ x + 1 }}
             function G(x : Int) : Int {{ x + 2 }}
             operation Main() : Int {{
                 use q = Qubit();
                 {prepare}
                 let b = MResetZ(q) == One;
                 {body}
             }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Int(expected)
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert_evaluation_order_qir(
                &qir,
                profile,
                !prepare.is_empty(),
                ssa,
                adaptive,
                prepared_adaptive,
            );
        }
    }
}

/// The hundreds encode F(10) or G(10), and the low digits count the reached RHS
/// effect. Nesting that effect inside the replacement must match a separate
/// statement, without requiring runtime storage for the consumed callable.
#[test]
fn evaluation_order_effectful_callable_and_replacement() {
    for replacement in [
        "set f = { set marker += 7; G };",
        "set marker += 7; set f = G;",
    ] {
        check_evaluation_order_callable_residue(
            &format!(
                "mutable marker = 0;
         mutable f = {{ F }};
         let _ = b and {{ {replacement} false }};
         f(10) * 100 + marker"
            ),
            [1100, 1207],
            &expect![[r#"
            block_0:
              %var_3 = alloca i1
              %var_4 = alloca i64
              %var_5 = alloca i1
              %var_7 = alloca i64
              %var_8 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_1, ptr %var_3
              store i64 0, ptr %var_4
              %var_14 = load i1, ptr %var_3
              store i1 %var_14, ptr %var_5
              %var_16 = load i1, ptr %var_5
              br i1 %var_16, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_4
              br label %block_2
            block_2:
              %var_17 = load i1, ptr %var_3
              br i1 %var_17, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_7
              br label %block_5
            block_4:
              store i64 11, ptr %var_7
              br label %block_5
            block_5:
              %var_19 = load i64, ptr %var_7
              store i64 %var_19, ptr %var_8
              %var_21 = load i64, ptr %var_8
              %var_9 = mul i64 %var_21, 100
              %var_22 = load i64, ptr %var_4
              %var_11 = add i64 %var_9, %var_22
              call void @__quantum__rt__int_record_output(i64 %var_11, ptr @0)
              ret i64 0"#]],
            &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_0, label %block_1, label %block_2
            block_1:
              br label %block_2
            block_2:
              %var_12 = phi i64 [0, %block_0], [7, %block_1]
              br i1 %var_0, label %block_3, label %block_4
            block_3:
              br label %block_5
            block_4:
              br label %block_5
            block_5:
              %var_13 = phi i64 [12, %block_3], [11, %block_4]
              %var_8 = mul i64 %var_13, 100
              %var_10 = add i64 %var_8, %var_12
              call void @__quantum__rt__int_record_output(i64 %var_10, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
            &expect![[r#"
            block_0:
              %var_2 = alloca i1
              %var_3 = alloca i64
              %var_4 = alloca i1
              %var_6 = alloca i64
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_0, ptr %var_2
              store i64 0, ptr %var_3
              %var_13 = load i1, ptr %var_2
              store i1 %var_13, ptr %var_4
              %var_15 = load i1, ptr %var_4
              br i1 %var_15, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_3
              br label %block_2
            block_2:
              %var_16 = load i1, ptr %var_2
              br i1 %var_16, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_6
              br label %block_5
            block_4:
              store i64 11, ptr %var_6
              br label %block_5
            block_5:
              %var_18 = load i64, ptr %var_6
              store i64 %var_18, ptr %var_7
              %var_20 = load i64, ptr %var_7
              %var_8 = mul i64 %var_20, 100
              %var_21 = load i64, ptr %var_3
              %var_10 = add i64 %var_8, %var_21
              call void @__quantum__rt__int_record_output(i64 %var_10, ptr @0)
              ret i64 0"#]],
        );
    }
}

#[test]
fn evaluation_order_effectful_callable_or_replacement() {
    check_evaluation_order_callable_residue(
        "mutable marker = 0;
         mutable f = { F };
         let _ = b or { set f = { set marker += 7; G }; false };
         f(10) * 100 + marker",
        [1207, 1100],
        &expect![[r#"
            block_0:
              %var_3 = alloca i1
              %var_4 = alloca i64
              %var_5 = alloca i1
              %var_7 = alloca i64
              %var_8 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_1, ptr %var_3
              store i64 0, ptr %var_4
              %var_14 = load i1, ptr %var_3
              store i1 %var_14, ptr %var_5
              %var_16 = load i1, ptr %var_5
              br i1 %var_16, label %block_2, label %block_1
            block_1:
              store i64 7, ptr %var_4
              br label %block_2
            block_2:
              %var_18 = load i1, ptr %var_3
              br i1 %var_18, label %block_3, label %block_4
            block_3:
              store i64 11, ptr %var_7
              br label %block_5
            block_4:
              store i64 12, ptr %var_7
              br label %block_5
            block_5:
              %var_20 = load i64, ptr %var_7
              store i64 %var_20, ptr %var_8
              %var_22 = load i64, ptr %var_8
              %var_9 = mul i64 %var_22, 100
              %var_23 = load i64, ptr %var_4
              %var_11 = add i64 %var_9, %var_23
              call void @__quantum__rt__int_record_output(i64 %var_11, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_0, label %block_2, label %block_1
            block_1:
              br label %block_2
            block_2:
              %var_13 = phi i64 [0, %block_0], [7, %block_1]
              %var_12 = phi i1 [true, %block_0], [false, %block_1]
              br i1 %var_0, label %block_3, label %block_4
            block_3:
              br label %block_5
            block_4:
              br label %block_5
            block_5:
              %var_14 = phi i64 [11, %block_3], [12, %block_4]
              %var_8 = mul i64 %var_14, 100
              %var_10 = add i64 %var_8, %var_13
              call void @__quantum__rt__int_record_output(i64 %var_10, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i1
              %var_3 = alloca i64
              %var_4 = alloca i1
              %var_6 = alloca i64
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_0, ptr %var_2
              store i64 0, ptr %var_3
              %var_13 = load i1, ptr %var_2
              store i1 %var_13, ptr %var_4
              %var_15 = load i1, ptr %var_4
              br i1 %var_15, label %block_2, label %block_1
            block_1:
              store i64 7, ptr %var_3
              br label %block_2
            block_2:
              %var_17 = load i1, ptr %var_2
              br i1 %var_17, label %block_3, label %block_4
            block_3:
              store i64 11, ptr %var_6
              br label %block_5
            block_4:
              store i64 12, ptr %var_6
              br label %block_5
            block_5:
              %var_19 = load i64, ptr %var_6
              store i64 %var_19, ptr %var_7
              %var_21 = load i64, ptr %var_7
              %var_8 = mul i64 %var_21, 100
              %var_22 = load i64, ptr %var_3
              %var_10 = add i64 %var_8, %var_22
              call void @__quantum__rt__int_record_output(i64 %var_10, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_effectful_callable_and_assign_replacement() {
    check_evaluation_order_callable_residue(
        "mutable marker = 0;
         mutable f = { F };
         mutable flag = b;
         set flag and= { set f = { set marker += 7; G }; false };
         f(10) * 100 + marker",
        [1100, 1207],
        &expect![[r#"
            block_0:
              %var_0 = alloca i1
              %var_5 = alloca i64
              %var_6 = alloca i1
              %var_7 = alloca i1
              %var_8 = alloca i1
              %var_9 = alloca i64
              %var_10 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_0
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_5
              store i1 %var_2, ptr %var_0
              %var_17 = load i1, ptr %var_0
              store i1 %var_17, ptr %var_7
              store i1 false, ptr %var_8
              %var_20 = load i1, ptr %var_7
              br i1 %var_20, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_5
              store i1 false, ptr %var_8
              br label %block_2
            block_2:
              %var_21 = load i1, ptr %var_8
              store i1 %var_21, ptr %var_6
              %var_23 = load i1, ptr %var_0
              br i1 %var_23, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_9
              br label %block_5
            block_4:
              store i64 11, ptr %var_9
              br label %block_5
            block_5:
              %var_25 = load i64, ptr %var_9
              store i64 %var_25, ptr %var_10
              %var_27 = load i64, ptr %var_10
              %var_11 = mul i64 %var_27, 100
              %var_28 = load i64, ptr %var_5
              %var_13 = add i64 %var_11, %var_28
              call void @__quantum__rt__int_record_output(i64 %var_13, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_2
            block_2:
              %var_14 = phi i64 [0, %block_0], [7, %block_1]
              br i1 %var_1, label %block_3, label %block_4
            block_3:
              br label %block_5
            block_4:
              br label %block_5
            block_5:
              %var_15 = phi i64 [12, %block_3], [11, %block_4]
              %var_10 = mul i64 %var_15, 100
              %var_12 = add i64 %var_10, %var_14
              call void @__quantum__rt__int_record_output(i64 %var_12, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i1
              %var_4 = alloca i64
              %var_5 = alloca i1
              %var_6 = alloca i1
              %var_7 = alloca i1
              %var_8 = alloca i64
              %var_9 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_4
              store i1 %var_1, ptr %var_0
              %var_16 = load i1, ptr %var_0
              store i1 %var_16, ptr %var_6
              store i1 false, ptr %var_7
              %var_19 = load i1, ptr %var_6
              br i1 %var_19, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_4
              store i1 false, ptr %var_7
              br label %block_2
            block_2:
              %var_20 = load i1, ptr %var_7
              store i1 %var_20, ptr %var_5
              %var_22 = load i1, ptr %var_0
              br i1 %var_22, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_8
              br label %block_5
            block_4:
              store i64 11, ptr %var_8
              br label %block_5
            block_5:
              %var_24 = load i64, ptr %var_8
              store i64 %var_24, ptr %var_9
              %var_26 = load i64, ptr %var_9
              %var_10 = mul i64 %var_26, 100
              %var_27 = load i64, ptr %var_4
              %var_12 = add i64 %var_10, %var_27
              call void @__quantum__rt__int_record_output(i64 %var_12, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_effectful_callable_or_assign_replacement() {
    check_evaluation_order_callable_residue(
        "mutable marker = 0;
         mutable f = { F };
         mutable flag = b;
         set flag or= { set f = { set marker += 7; G }; false };
         f(10) * 100 + marker",
        [1207, 1100],
        &expect![[r#"
            block_0:
              %var_0 = alloca i1
              %var_5 = alloca i64
              %var_6 = alloca i1
              %var_7 = alloca i1
              %var_8 = alloca i1
              %var_9 = alloca i64
              %var_10 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_0
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_5
              store i1 %var_2, ptr %var_0
              %var_17 = load i1, ptr %var_0
              store i1 %var_17, ptr %var_7
              store i1 true, ptr %var_8
              %var_20 = load i1, ptr %var_7
              br i1 %var_20, label %block_2, label %block_1
            block_1:
              store i64 7, ptr %var_5
              store i1 false, ptr %var_8
              br label %block_2
            block_2:
              %var_23 = load i1, ptr %var_8
              store i1 %var_23, ptr %var_6
              %var_25 = load i1, ptr %var_0
              br i1 %var_25, label %block_3, label %block_4
            block_3:
              store i64 11, ptr %var_9
              br label %block_5
            block_4:
              store i64 12, ptr %var_9
              br label %block_5
            block_5:
              %var_27 = load i64, ptr %var_9
              store i64 %var_27, ptr %var_10
              %var_29 = load i64, ptr %var_10
              %var_11 = mul i64 %var_29, 100
              %var_30 = load i64, ptr %var_5
              %var_13 = add i64 %var_11, %var_30
              call void @__quantum__rt__int_record_output(i64 %var_13, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_1, label %block_2, label %block_1
            block_1:
              br label %block_2
            block_2:
              %var_15 = phi i64 [0, %block_0], [7, %block_1]
              %var_14 = phi i1 [true, %block_0], [false, %block_1]
              br i1 %var_1, label %block_3, label %block_4
            block_3:
              br label %block_5
            block_4:
              br label %block_5
            block_5:
              %var_16 = phi i64 [11, %block_3], [12, %block_4]
              %var_10 = mul i64 %var_16, 100
              %var_12 = add i64 %var_10, %var_15
              call void @__quantum__rt__int_record_output(i64 %var_12, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_0 = alloca i1
              %var_4 = alloca i64
              %var_5 = alloca i1
              %var_6 = alloca i1
              %var_7 = alloca i1
              %var_8 = alloca i64
              %var_9 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_0
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_4
              store i1 %var_1, ptr %var_0
              %var_16 = load i1, ptr %var_0
              store i1 %var_16, ptr %var_6
              store i1 true, ptr %var_7
              %var_19 = load i1, ptr %var_6
              br i1 %var_19, label %block_2, label %block_1
            block_1:
              store i64 7, ptr %var_4
              store i1 false, ptr %var_7
              br label %block_2
            block_2:
              %var_22 = load i1, ptr %var_7
              store i1 %var_22, ptr %var_5
              %var_24 = load i1, ptr %var_0
              br i1 %var_24, label %block_3, label %block_4
            block_3:
              store i64 11, ptr %var_8
              br label %block_5
            block_4:
              store i64 12, ptr %var_8
              br label %block_5
            block_5:
              %var_26 = load i64, ptr %var_8
              store i64 %var_26, ptr %var_9
              %var_28 = load i64, ptr %var_9
              %var_10 = mul i64 %var_28, 100
              %var_29 = load i64, ptr %var_4
              %var_12 = add i64 %var_10, %var_29
              call void @__quantum__rt__int_record_output(i64 %var_12, ptr @0)
              ret i64 0"#]],
    );
}

/// Calling `f` must see the replacement, but `saved` must still call the
/// original function. The low digits expose skipped or duplicated RHS effects.
#[test]
fn evaluation_order_effectful_callable_saved_snapshot_replacement() {
    check_evaluation_order_callable_residue(
        "mutable marker = 0;
         mutable f = { F };
         let current = f;
         let saved = (x -> current(x));
         let _ = b and { set f = { set marker += 7; G }; false };
         f(10) * 10000 + saved(10) * 100 + marker",
        [111_100, 121_107],
        &expect![[r#"
            block_0:
              %var_3 = alloca i1
              %var_4 = alloca i64
              %var_5 = alloca i1
              %var_7 = alloca i64
              %var_8 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_1, ptr %var_3
              store i64 0, ptr %var_4
              %var_16 = load i1, ptr %var_3
              store i1 %var_16, ptr %var_5
              %var_18 = load i1, ptr %var_5
              br i1 %var_18, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_4
              br label %block_2
            block_2:
              %var_19 = load i1, ptr %var_3
              br i1 %var_19, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_7
              br label %block_5
            block_4:
              store i64 11, ptr %var_7
              br label %block_5
            block_5:
              %var_21 = load i64, ptr %var_7
              store i64 %var_21, ptr %var_8
              %var_23 = load i64, ptr %var_8
              %var_9 = mul i64 %var_23, 10000
              %var_11 = add i64 %var_9, 1100
              %var_24 = load i64, ptr %var_4
              %var_13 = add i64 %var_11, %var_24
              call void @__quantum__rt__int_record_output(i64 %var_13, ptr @0)
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              br i1 %var_0, label %block_1, label %block_2
            block_1:
              br label %block_2
            block_2:
              %var_14 = phi i64 [0, %block_0], [7, %block_1]
              br i1 %var_0, label %block_3, label %block_4
            block_3:
              br label %block_5
            block_4:
              br label %block_5
            block_5:
              %var_15 = phi i64 [12, %block_3], [11, %block_4]
              %var_8 = mul i64 %var_15, 10000
              %var_10 = add i64 %var_8, 1100
              %var_12 = add i64 %var_10, %var_14
              call void @__quantum__rt__int_record_output(i64 %var_12, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
        &expect![[r#"
            block_0:
              %var_2 = alloca i1
              %var_3 = alloca i64
              %var_4 = alloca i1
              %var_6 = alloca i64
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_0, ptr %var_2
              store i64 0, ptr %var_3
              %var_15 = load i1, ptr %var_2
              store i1 %var_15, ptr %var_4
              %var_17 = load i1, ptr %var_4
              br i1 %var_17, label %block_1, label %block_2
            block_1:
              store i64 7, ptr %var_3
              br label %block_2
            block_2:
              %var_18 = load i1, ptr %var_2
              br i1 %var_18, label %block_3, label %block_4
            block_3:
              store i64 12, ptr %var_6
              br label %block_5
            block_4:
              store i64 11, ptr %var_6
              br label %block_5
            block_5:
              %var_20 = load i64, ptr %var_6
              store i64 %var_20, ptr %var_7
              %var_22 = load i64, ptr %var_7
              %var_8 = mul i64 %var_22, 10000
              %var_10 = add i64 %var_8, 1100
              %var_23 = load i64, ptr %var_3
              %var_12 = add i64 %var_10, %var_23
              call void @__quantum__rt__int_record_output(i64 %var_12, ptr @0)
              ret i64 0"#]],
    );
}

#[test]
fn evaluation_order_callable_transparent_initializers_reach_qir() {
    for initializer in ["{ F }", "{{ F }}", "if true { F } else { G }"] {
        check_evaluation_order_callable_residue(
            &format!("mutable f = {initializer}; let _ = b and {{ set f = G; false }}; f(10)"),
            [11, 12],
            &expect![[r#"
                block_0:
                  %var_3 = alloca i1
                  %var_4 = alloca i1
                  %var_6 = alloca i64
                  %var_7 = alloca i64
                  call void @__quantum__rt__initialize(ptr null)
                  call void @X(ptr inttoptr (i64 0 to ptr))
                  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                  %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                  store i1 %var_1, ptr %var_3
                  %var_9 = load i1, ptr %var_3
                  store i1 %var_9, ptr %var_4
                  %var_11 = load i1, ptr %var_4
                  br i1 %var_11, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_12 = load i1, ptr %var_3
                  br i1 %var_12, label %block_3, label %block_4
                block_3:
                  store i64 12, ptr %var_6
                  br label %block_5
                block_4:
                  store i64 11, ptr %var_6
                  br label %block_5
                block_5:
                  %var_14 = load i64, ptr %var_6
                  store i64 %var_14, ptr %var_7
                  %var_16 = load i64, ptr %var_7
                  call void @__quantum__rt__int_record_output(i64 %var_16, ptr @0)
                  ret i64 0"#]],
            &expect![[r#"
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  br i1 %var_0, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  br i1 %var_0, label %block_3, label %block_4
                block_3:
                  br label %block_5
                block_4:
                  br label %block_5
                block_5:
                  %var_7 = phi i64 [12, %block_3], [11, %block_4]
                  call void @__quantum__rt__int_record_output(i64 %var_7, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0"#]],
            &expect![[r#"
                block_0:
                  %var_2 = alloca i1
                  %var_3 = alloca i1
                  %var_5 = alloca i64
                  %var_6 = alloca i64
                  call void @__quantum__rt__initialize(ptr null)
                  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                  %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                  store i1 %var_0, ptr %var_2
                  %var_8 = load i1, ptr %var_2
                  store i1 %var_8, ptr %var_3
                  %var_10 = load i1, ptr %var_3
                  br i1 %var_10, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_11 = load i1, ptr %var_2
                  br i1 %var_11, label %block_3, label %block_4
                block_3:
                  store i64 12, ptr %var_5
                  br label %block_5
                block_4:
                  store i64 11, ptr %var_5
                  br label %block_5
                block_5:
                  %var_13 = load i64, ptr %var_5
                  store i64 %var_13, ptr %var_6
                  %var_15 = load i64, ptr %var_6
                  call void @__quantum__rt__int_record_output(i64 %var_15, ptr @0)
                  ret i64 0"#]],
        );
    }
}

#[test]
fn evaluation_order_callable_captured_snapshot_reaches_qir() {
    for snapshot in [
        "let current = f; let saved = (x -> current(x));",
        "let first = f; let current = first; let saved = (x -> current(x));",
        "let current = f; let saved = { let next = current; (x -> next(x)) };",
    ] {
        check_evaluation_order_callable_residue(
            &format!(
                "mutable f = F;
                 {snapshot}
                 let _ = b and {{ set f = G; false }};
                 f(10) * 100 + saved(10)"
            ),
            [1111, 1211],
            &expect![[r#"
                block_0:
                  %var_3 = alloca i1
                  %var_4 = alloca i1
                  %var_6 = alloca i64
                  %var_7 = alloca i64
                  call void @__quantum__rt__initialize(ptr null)
                  call void @X(ptr inttoptr (i64 0 to ptr))
                  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                  %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                  store i1 %var_1, ptr %var_3
                  %var_12 = load i1, ptr %var_3
                  store i1 %var_12, ptr %var_4
                  %var_14 = load i1, ptr %var_4
                  br i1 %var_14, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_15 = load i1, ptr %var_3
                  br i1 %var_15, label %block_3, label %block_4
                block_3:
                  store i64 12, ptr %var_6
                  br label %block_5
                block_4:
                  store i64 11, ptr %var_6
                  br label %block_5
                block_5:
                  %var_17 = load i64, ptr %var_6
                  store i64 %var_17, ptr %var_7
                  %var_19 = load i64, ptr %var_7
                  %var_8 = mul i64 %var_19, 100
                  %var_10 = add i64 %var_8, 11
                  call void @__quantum__rt__int_record_output(i64 %var_10, ptr @0)
                  ret i64 0"#]],
            &expect![[r#"
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  br i1 %var_0, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  br i1 %var_0, label %block_3, label %block_4
                block_3:
                  br label %block_5
                block_4:
                  br label %block_5
                block_5:
                  %var_11 = phi i64 [12, %block_3], [11, %block_4]
                  %var_7 = mul i64 %var_11, 100
                  %var_9 = add i64 %var_7, 11
                  call void @__quantum__rt__int_record_output(i64 %var_9, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0"#]],
            &expect![[r#"
                block_0:
                  %var_2 = alloca i1
                  %var_3 = alloca i1
                  %var_5 = alloca i64
                  %var_6 = alloca i64
                  call void @__quantum__rt__initialize(ptr null)
                  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                  %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                  store i1 %var_0, ptr %var_2
                  %var_11 = load i1, ptr %var_2
                  store i1 %var_11, ptr %var_3
                  %var_13 = load i1, ptr %var_3
                  br i1 %var_13, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_14 = load i1, ptr %var_2
                  br i1 %var_14, label %block_3, label %block_4
                block_3:
                  store i64 12, ptr %var_5
                  br label %block_5
                block_4:
                  store i64 11, ptr %var_5
                  br label %block_5
                block_5:
                  %var_16 = load i64, ptr %var_5
                  store i64 %var_16, ptr %var_6
                  %var_18 = load i64, ptr %var_6
                  %var_7 = mul i64 %var_18, 100
                  %var_9 = add i64 %var_7, 11
                  call void @__quantum__rt__int_record_output(i64 %var_9, ptr @0)
                  ret i64 0"#]],
        );
    }
}

#[test]
fn evaluation_order_array_updates_evaluate_index_then_value_then_target() {
    for (update, expected_trace) in [
        (
            "let updated = { set trace = 10 * trace + 3; values }
                w/ { set trace = 10 * trace + 1; set values = [6, 7]; 0 }
                <- { set trace = 10 * trace + 2; MResetZ(q); set values = [8, 9]; 11 };",
            123,
        ),
        (
            "mutable updated = values;
                set updated w/= { set trace = 10 * trace + 1; set updated = [6, 7]; 0 }
                <- { set trace = 10 * trace + 2; MResetZ(q); set updated = [8, 9]; 11 };",
            12,
        ),
    ] {
        let source = format!(
            "operation Main() : (Int[], Int) {{
                use q = Qubit();
                mutable trace = 0;
                mutable values = [4, 5];
                {update}
                (updated, trace)
            }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Tuple(
                vec![
                    Value::Array(vec![Value::Int(11), Value::Int(9)].into()),
                    Value::Int(expected_trace),
                ]
                .into(),
                None,
            ),
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            let outputs = qir_entry_body(&qir)
                .lines()
                .filter(|line| line.contains("_record_output("))
                .map(|line| {
                    line.split_once('(')
                        .expect("output call")
                        .1
                        .split(',')
                        .next()
                        .expect("output operand")
                })
                .collect::<Vec<_>>();
            assert_eq!(
                outputs,
                [
                    "i64 2",
                    "i64 2",
                    "i64 11",
                    "i64 9",
                    &format!("i64 {expected_trace}")
                ],
                "{profile:?}, {update}: incorrect updated contents or effect order:\n{qir}",
            );
            assert_eq!(
                qir_entry_body(&qir)
                    .matches("call void @__quantum__qis__mresetz__body(")
                    .count(),
                1,
                "{profile:?}: replacement measurement must execute once:\n{qir}",
            );
        }
    }
}

#[test]
fn evaluation_order_array_construction_and_update_operands_follow_source_order() {
    for (expression, expected) in [
        (
            "[{ set trace = 10 * trace + 1; MResetZ(q) }, { set trace = 10 * trace + 2; MResetZ(q) }]",
            12,
        ),
        (
            "[{ set trace = 10 * trace + 1; MResetZ(q) }, size = { set trace = 10 * trace + 2; 2 }]",
            12,
        ),
        (
            "{ set trace = 10 * trace + 1; [MResetZ(q)] } + { set trace = 10 * trace + 2; [MResetZ(q)] }",
            12,
        ),
        (
            "{ set trace = 10 * trace + 3; [MResetZ(q)] } w/ { set trace = 10 * trace + 1; 0 } <- { set trace = 10 * trace + 2; Zero }",
            123,
        ),
    ] {
        let source = format!(
            "operation Main() : Int {{
                use q = Qubit();
                mutable trace = 0;
                let _ = {expression};
                trace
            }}"
        );
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Int(expected)
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert!(
                qir.contains(&format!(
                    "call void @__quantum__rt__int_record_output(i64 {expected},"
                )),
                "{profile:?}, {expression}: operands executed out of order:\n{qir}"
            );
        }
    }
}

#[test]
fn early_return_skips_later_loop_conditional() {
    let source = r#"
        operation Main() : Int {
            if true { return 1; }
            if true { return 2; } else { for i in [0] { return 3; } }
            return 4;
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRI, Profile::AdaptiveRIF] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("call void @__quantum__rt__int_record_output(i64 1,"),
            "{profile:?}: {qir}"
        );
    }
}

#[test]
fn functored_local_after_early_return_generates_qir() {
    for initializer in ["U", "[U, V][1]"] {
        for invocation in [
            "f()",
            "Adjoint f()",
            "Adjoint Adjoint f()",
            "Controlled f([], ())",
            "Controlled Adjoint f([], ())",
            "Controlled Controlled f([], ([], ()))",
        ] {
            let source = format!(
                r#"
                operation U() : Unit is Adj + Ctl {{}}
                operation V() : Unit is Adj + Ctl {{}}
                operation Main() : Unit {{
                    if false {{ return (); }}
                    let f = {initializer};
                    {invocation};
                }}
                "#
            );
            for profile in [
                Profile::Base,
                Profile::AdaptiveRI,
                Profile::AdaptiveRIF,
                Profile::Adaptive,
            ] {
                compile_source_to_qir(&source, profile.into());
            }
        }
    }
}

#[test]
fn nested_callable_selectors_generate_correct_qir_on_every_profile() {
    let source = r#"
        operation A(q : Qubit[]) : Unit { X(q[0]); }
        operation B(q : Qubit[]) : Unit { X(q[1]); }
        function Choose(flag : Bool, a : Qubit[] => Unit, b : Qubit[] => Unit) : Qubit[] => Unit {
            if flag { a } else { b }
        }
        operation Main() : Result[] {
            use q = Qubit[2];
            let f = Choose(1 < 2, Choose(1 < 2, A, B), B);
            f(q);
            MResetEachZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let qir = compile_source_to_qir(source, profile.into());
        if profile == Profile::Adaptive {
            assert_eq!(qir.matches("call void @X(").count(), 1, "{qir}");
            assert!(
                qir.contains("call void @X(ptr inttoptr (i64 0 to ptr))"),
                "{qir}"
            );
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body").count(),
                1,
                "{qir}"
            );
        } else {
            assert_x_targets(&qir, &[0]);
        }
    }
}

/// The outer true branch uses the inner selection; inner false must select B.
#[test]
fn nested_callable_selectors_inner_false_selects_second_operation() {
    let source = r#"
        operation A(q : Qubit[]) : Unit { X(q[0]); }
        operation B(q : Qubit[]) : Unit { X(q[1]); }
        function Choose(flag : Bool, a : Qubit[] => Unit, b : Qubit[] => Unit) : Qubit[] => Unit {
            if flag { a } else { b }
        }
        operation Main() : Result[] {
            use q = Qubit[2];
            let f = Choose(1 < 2, Choose(1 > 2, A, B), B);
            f(q);
            MResetEachZ(q)
        }
    "#;
    assert_nested_callable_selector_on_every_profile(source, 1);
}

/// Even though the inner selection produces A, outer false must select B.
#[test]
fn nested_callable_selectors_outer_false_selects_second_operation() {
    let source = r#"
        operation A(q : Qubit[]) : Unit { X(q[0]); }
        operation B(q : Qubit[]) : Unit { X(q[1]); }
        function Choose(flag : Bool, a : Qubit[] => Unit, b : Qubit[] => Unit) : Qubit[] => Unit {
            if flag { a } else { b }
        }
        operation Main() : Result[] {
            use q = Qubit[2];
            let f = Choose(1 > 2, Choose(1 < 2, A, B), B);
            f(q);
            MResetEachZ(q)
        }
    "#;
    assert_nested_callable_selector_on_every_profile(source, 1);
}

/// Read the outer flag before the inner argument sets it false: select A.
/// The later conditional B gate also detects a dropped mutation, which would
/// otherwise leave the selected target unchanged but execute an extra X.
#[test]
fn nested_callable_selectors_snapshot_preserves_argument_effects() {
    let source = r#"
        operation A(q : Qubit[]) : Unit { X(q[0]); }
        operation B(q : Qubit[]) : Unit { X(q[1]); }
        function Choose(flag : Bool, a : Qubit[] => Unit, b : Qubit[] => Unit) : Qubit[] => Unit {
            if flag { a } else { b }
        }
        operation Main() : Result[] {
            use q = Qubit[2];
            mutable flag = true;
            let f = Choose(flag, Choose({ set flag = false; 1 < 2 }, A, B), B);
            f(q);
            if flag { B(q); }
            MResetEachZ(q)
        }
    "#;
    assert_nested_callable_selector_on_every_profile(source, 0);
}

fn assert_nested_callable_selector_on_every_profile(source: &str, target: usize) {
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let context = format!("{profile:?}: {source}");
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let expected = Value::Array(
            (0..2)
                .map(|index| Value::Result(qsc_eval::val::Result::Val(index == target)))
                .collect::<Vec<_>>()
                .into(),
        );
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            expected,
            "{context}: source must select only qubit {target}"
        );
        let qir = compile_source_to_qir(source, profile.into());
        if profile == Profile::Adaptive {
            assert_eq!(qir.matches("call void @X(").count(), 1, "{context}:\n{qir}");
            assert!(
                qir.contains(&format!("call void @X(ptr inttoptr (i64 {target} to ptr))")),
                "{context}: expected X on qubit {target}:\n{qir}"
            );
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body").count(),
                1,
                "{context}:\n{qir}"
            );
        } else {
            assert_x_targets_with_context(&qir, &[target], &context);
        }
    }
}

#[test]
fn expanding_polymorphic_recursion_reports_source_errors_on_every_compile_profile() {
    let sources = [
        // Array growth with a captured callable used to reach a closure-layout panic.
        r#"operation Rec<'T>(t : 'T, op : 'T => Unit, n : Int) : Unit {
            if n == 0 { op(t); } else { Rec([t], ts => op(ts[0]), n - 1); }
        }
        operation Main() : Result { use q = Qubit(); Rec(q, X, 1); MResetZ(q) }"#,
        // Tuple duplication grows exponentially, despite the finite runtime depth.
        r#"function F<'T>(x : 'T, n : Int) : Int {
            if n == 0 { 0 } else { 1 + F((x, x), n - 1) }
        }
        operation Main() : Result {
            use q = Qubit();
            if F(1, 2) == 2 { X(q); }
            MResetZ(q)
        }"#,
    ];
    for source in sources {
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Result(qsc_eval::val::Result::Val(true)),
            "runtime termination is independent of the static expansion limit"
        );
        for profile in [
            Profile::Base,
            Profile::AdaptiveRI,
            Profile::AdaptiveRIF,
            Profile::Adaptive,
        ] {
            let capabilities = profile.into();
            let mut interpreter = interpreter_with_capabilities(capabilities);
            eval_fragments(&mut interpreter, source);
            for result in [
                compile_source_to_qir_result(source, capabilities),
                interpreter.qirgen("Main()"),
            ] {
                let errors = result.expect_err("expanding specialization must not produce QIR");
                assert_eq!(errors.len(), 1, "{profile:?}: {errors:?}");
                assert!(matches!(
                    errors[0],
                    crate::interpret::Error::FirTransform(_)
                ));
                let label = miette::Diagnostic::labels(&errors[0])
                    .and_then(|mut labels| labels.next())
                    .expect("expansion error must have a source label");
                assert!(!label.is_empty());
                assert!(label.offset() + label.len() <= source.len());
                assert!(miette::Diagnostic::source_code(&errors[0]).is_some());
                let diagnostic = format_interpret_errors(errors);
                assert!(
                    diagnostic.contains("Qdk.Qsc.Monomorphize.TypeExpansionLimit"),
                    "{diagnostic}"
                );
                assert!(
                    diagnostic.contains("type expansion exceeds the compiler limit here"),
                    "{diagnostic}"
                );
            }
            // A failed compilation must not poison the interpreter's original store.
            eval_fragments(
                &mut interpreter,
                "operation Safe() : Result { use q = Qubit(); X(q); MResetZ(q) }",
            );
            assert!(interpreter.qirgen("Safe()").is_ok());
        }
    }
}

#[test]
fn finite_recursive_type_cycle_generates_qir_for_all_entry_surfaces() {
    let source = r#"
        operation Pick<'T, 'U>(x : 'T, y : 'U, n : Int, q : Qubit) : Unit {
            if n == 0 { X(q); } else { Pick(y, x, n - 1, q); }
        }
        operation Main() : Result {
            use q = Qubit();
            Pick(2, true, 3, q);
            MResetZ(q)
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
    eval_fragments(&mut interpreter, source);
    // This is the source oracle. The QIR assertions below check typed
    // compilation and output plumbing, not execution of the emitted program.
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()"),
        Value::Result(qsc_eval::val::Result::Val(true))
    );
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let capabilities = profile.into();
        let mut interpreter = interpreter_with_capabilities(capabilities);
        eval_fragments(&mut interpreter, source);
        let (store, package, items) =
            compile_and_locate_items(source, &[("Main", true)], capabilities);
        for qir in [
            compile_source_to_qir(source, capabilities),
            interpreter
                .qirgen("Main()")
                .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors))),
            callable_args_to_qir(&store, package, items["Main"], &Value::unit(), capabilities),
        ] {
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body").count(),
                1,
                "{profile:?}: {qir}"
            );
            assert_eq!(
                qir.matches("call void @__quantum__rt__result_record_output")
                    .count(),
                1,
                "{qir}"
            );
        }
    }
    let mut interpreter = interpreter_with_capabilities(Profile::AdaptiveRIF.into());
    eval_fragments(&mut interpreter, source);
    let circuit = interpreter
        .circuit(
            crate::interpret::CircuitEntryPoint::EntryExpr("Main()".into()),
            crate::interpret::CircuitGenerationMethod::Static,
            qsc_circuit::TracerConfig {
                max_operations: qsc_circuit::TracerConfig::DEFAULT_MAX_OPERATIONS,
                max_loop_iterations: None,
                source_locations: false,
                group_by_scope: false,
                prune_classical_qubits: false,
            },
        )
        .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)));
    let gates: Vec<_> = circuit
        .component_grid
        .iter()
        .flat_map(|column| &column.components)
        .filter_map(|operation| match operation {
            qsc_circuit::Operation::Unitary(gate) => Some(gate.gate.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(gates, ["X"], "{circuit}");
}

#[test]
fn recursive_callable_factory_is_inlined_in_adaptive_qir() {
    let source = r#"
        namespace Test {
            operation Flip(q : Qubit) : Unit { X(q); }

            function Make(n : Int) : Qubit => Unit {
                if n == 0 {
                    Flip
                } else {
                    let next = Make(n - 1);
                    next
                }
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Make(2)(q);
                MResetZ(q)
            }
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        let capabilities = profile.into();
        let mut interpreter = interpreter_with_capabilities(capabilities);
        eval_fragments(&mut interpreter, source);
        let incremental_qir = interpreter
            .qirgen("Test.Main()")
            .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)));
        for qir in [compile_source_to_qir(source, capabilities), incremental_qir] {
            assert!(!qir.contains("@Make"), "factory must be inlined: {qir}");
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body").count(),
                1,
                "{qir}"
            );
            assert!(
                qir.contains("call void @__quantum__rt__result_record_output"),
                "{qir}"
            );
        }
    }
}

#[test]
fn controlled_noop_tuple_alias_matches_literal_input_in_base_qir() {
    let source = |call| {
        format!(
            r#"
            operation Target() : Unit is Ctl {{}}
            @EntryPoint(Base)
            operation Main() : Unit {{ {call} }}
            "#
        )
    };
    let literal =
        compile_source_to_qir(&source("Controlled Target([], ());"), Profile::Base.into());
    let qir = compile_source_to_qir(
        &source("let args : (Qubit[], Unit) = ([], ()); Controlled Target(args);"),
        Profile::Base.into(),
    );
    assert_eq!(qir, literal);
    assert!(!qir.contains("call void @__quantum__qis__"), "{qir}");
}

/// RCA must accept literal, aliased, and opaque controlled arguments without changing
/// code generation. Empty controls leave two X gates on one qubit; the pair also lets
/// the simulator verify that the allocated qubit is released in the zero state.
#[test]
fn controlled_tuple_values_and_aliases_match_literal_inputs_in_base_qir() {
    let source = |call| {
        format!(
            r#"
            function MakeArgs() : (Qubit[], Unit) {{ ([], ()) }}
            operation Target() : Unit is Ctl {{
                body (...) {{
                    use q = Qubit();
                    X(q);
                    X(q);
                }}
                controlled (controls, ...) {{
                    use q = Qubit();
                    Controlled X(controls, q);
                    Controlled X(controls, q);
                }}
            }}
            @EntryPoint() operation Main() : Unit {{ {call} }}
        "#
        )
    };
    let literal =
        compile_source_to_qir(&source("Controlled Target([], ());"), Profile::Base.into());
    assert_eq!(
        literal
            .matches("call void @__quantum__qis__x__body")
            .count(),
        2
    );
    assert!(
        literal.contains(r#""required_num_qubits"="1""#),
        "{literal}"
    );

    for call in [
        "let args : (Qubit[], Unit) = ([], ()); Controlled Target(args);",
        "let args : (Qubit[], Unit) = ([], ()); let alias = args; Controlled Target(alias);",
        "mutable args : (Qubit[], Unit) = ([], ()); Controlled Target(args);",
        "Controlled Target({ let args : (Qubit[], Unit) = ([], ()); args });",
        "Controlled Target(MakeArgs());",
        "let packed : (Int, (Qubit[], Unit), Bool) = (1, ([], ()), true); let (_, args, _) = packed; Controlled Target(args);",
        "let inner : (Qubit[], Unit) = ([], ()); let args : (Qubit[], (Qubit[], Unit)) = ([], inner); Controlled Controlled Target(args);",
        "mutable args : (Qubit[], (Qubit[], Unit)) = ([], ([], ())); Controlled Controlled Target(args);",
    ] {
        let source = source(call);
        let qir = compile_source_to_qir(&source, Profile::Base.into());
        assert_eq!(qir, literal, "{call}");
        let mut interpreter = interpreter_with_capabilities(TargetCapabilityFlags::all());
        eval_fragments(&mut interpreter, &source.replace("@EntryPoint()", ""));
        assert_eq!(eval_fragments(&mut interpreter, "Main()"), Value::unit());
    }
}

#[test]
fn dump_operation_is_codegen_noop_across_restricted_profiles() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                Std.Diagnostics.DumpOperation(1, qs => H(qs[0]));
            }
        }
    "#;

    for profile in [Profile::Base, Profile::AdaptiveRI, Profile::AdaptiveRIF] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            !qir.contains("DumpOperation"),
            "expected DumpOperation to emit no call, declaration, or symbol for {profile:?}:\n{qir}"
        );
    }
}

#[test]
fn codegen_noop_intrinsic_preserves_effectful_arguments() {
    let source = r#"
        namespace Test {
            operation CountAfterMeasurement(q : Qubit) : Int {
                M(q);
                1
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Std.Diagnostics.DumpOperation(
                    CountAfterMeasurement(q),
                    qs => H(qs[0])
                );
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::AdaptiveRI.into());
    assert!(
        qir.contains("__quantum__qis__m__body"),
        "expected the effectful DumpOperation argument to remain in generated QIR:\n{qir}"
    );
}

#[test]
fn fact_is_codegen_noop_after_simulatable_intrinsic_collapse() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                Std.Diagnostics.Fact(false, "message");
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert!(
        !qir.contains("Fact"),
        "expected Fact to emit no QIR symbol:\n{qir}"
    );
}

/// Compiles `lib_source` as a separate library package, then generates QIR for
/// `user_source` with that library as a dependency. The library's namespaces are
/// visible to the user program without an alias, so user code can reference them
/// directly (for example `import Lib.*;`). This exercises codegen for foreign
/// callable references without requiring them to remain standalone IR functions.
fn compile_source_to_qir_with_library(
    lib_source: &str,
    user_source: &str,
    capabilities: TargetCapabilityFlags,
) -> String {
    match compile_source_to_qir_with_library_result(lib_source, user_source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_with_library_result(
    lib_source: &str,
    user_source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);

    // Compile the library as its own package depending on core + std.
    let lib_sources = SourceMap::new([("lib.qs".into(), lib_source.into())], None);
    let lib_dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];
    let (lib_unit, lib_errors) = crate::compile::compile(
        &store,
        &lib_dependencies,
        lib_sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(
        lib_errors.is_empty(),
        "library compilation failed: {lib_errors:?}"
    );
    let lib_id = store.insert(lib_unit);

    // Generate QIR for the user program, which depends on core + std + library.
    let user_sources = source_map_from_source(user_source);
    let user_dependencies: Vec<(PackageId, Option<Arc<str>>)> =
        vec![(std_id, None), (lib_id, None)];
    get_qir(
        user_sources,
        language_features,
        capabilities,
        store,
        &user_dependencies,
    )
}

#[test]
fn package_aware_foreign_projected_dynamic_call_resolves_to_qir() {
    // A foreign library operation stores a loop-reassigned local (`op`, provably
    // `X` after the loop) in a struct field and calls it through a field
    // projection (`config.Apply(q)`). Defunctionalization over-approximates the
    // projected callee to dynamic and defers the convergence failure; partial
    // evaluation then resolves it to the concrete `X` global, so the whole
    // program lowers to clean Base-profile QIR instead of failing to compile.
    let lib_source = r#"
        namespace ForeignLib {
            struct Config {
                Count : Int,
                Apply : Qubit => Unit,
                Keep : Qubit => Unit,
            }

            operation InvokeDynamic(q : Qubit) : Unit {
                mutable op = H;
                for _ in 0..3 {
                    op = X;
                }
                let config = new Config {
                    Count = 1,
                    Apply = op,
                    Keep = q2 => {
                        H(q2);
                        S(q2);
                    },
                };
                config.Apply(q);
            }

            export InvokeDynamic;
        }
    "#;
    let user_source = r#"
        import ForeignLib.*;

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            InvokeDynamic(q);
        }
    "#;

    let qir =
        compile_source_to_qir_with_library(lib_source, user_source, TargetCapabilityFlags::empty());
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]]
    .assert_eq(&qir);
}

fn compile_source_to_qir_from_ast(source: &str, capabilities: TargetCapabilityFlags) -> String {
    match compile_source_to_qir_from_ast_result(source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR from AST for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_from_ast_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let (ast_package, sources) = parse_source_to_ast(source);
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> =
        vec![(PackageId::CORE, None), (std_id, None)];

    get_qir_from_ast(
        &mut store,
        &dependencies,
        ast_package,
        sources,
        capabilities,
    )
}

fn compile_source_to_rir(source: &str, capabilities: TargetCapabilityFlags) -> Vec<String> {
    match compile_source_to_rir_result(source, capabilities) {
        Ok(rir) => rir,
        Err(errors) => panic!(
            "Failed to generate RIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_rir_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<Vec<String>, Vec<crate::interpret::Error>> {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();

    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    get_rir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    )
}

#[test]
fn code_with_errors_returns_errors() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit()
                let pi_over_two = 4.0 / 2.0;
            }
        }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Compile(
                    WithSource {
                        sources: [
                            Source {
                                name: "test.qs",
                                contents: "namespace Test {\n            @EntryPoint()\n            operation Main() : Unit {\n                use q = Qubit()\n                let pi_over_two = 4.0 / 2.0;\n            }\n        }",
                                offset: 0,
                            },
                        ],
                        error: Frontend(
                            Error(
                                Parse(
                                    Error(
                                        Token(
                                            Semi,
                                            Keyword(
                                                Let,
                                            ),
                                            Span {
                                                lo: 129,
                                                hi: 132,
                                            },
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(sources, language_features, capabilities, store, &[(std_id, None)]));
}

#[test]
fn excessive_specializations_warning_does_not_block_qir_generation() {
    let source = r#"
        namespace Test {
            operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Apply(q1 => Rx(1.0, q1), q);
                Apply(q1 => Rx(2.0, q1), q);
                Apply(q1 => Rx(3.0, q1), q);
                Apply(q1 => Rx(4.0, q1), q);
                Apply(q1 => Rx(5.0, q1), q);
                Apply(q1 => Rx(6.0, q1), q);
                Apply(q1 => Rx(7.0, q1), q);
                Apply(q1 => Rx(8.0, q1), q);
                Apply(q1 => Rx(9.0, q1), q);
                Apply(q1 => Rx(10.0, q1), q);
                Apply(q1 => Rx(11.0, q1), q);
            }
        }
    "#;

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations,
    );

    assert!(
        qir.contains("__quantum__qis__rx__body"),
        "expected QIR generation to continue through warning-only FIR transforms"
    );
}

#[test]
fn defunctionalize_operand_block_set_reaches_qir() {
    // Both programs reassign `f` to `Bar` before `f(5)` is evaluated, so the
    // reaching definition is `Bar`: `Bar(5) = 105`, plus `z = 1` gives the
    // angle `106.0`. They differ only in where `set f = Bar` sits — in an
    // operand-position block versus at the top level — and both must specialize
    // `f(5)` to `Bar` and emit `rz__body(double 106.0`.
    let operand_block = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let z = { set f = Bar; 0 } + 1;
                Rz(IntAsDouble(f(5) + z), q);
            }
        }
    "#;
    let top_level = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                set f = Bar;
                let z = 1;
                Rz(IntAsDouble(f(5) + z), q);
            }
        }
    "#;

    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let operand_block_qir = compile_source_to_qir(operand_block, capabilities);
    let top_level_qir = compile_source_to_qir(top_level, capabilities);

    assert!(
        top_level_qir.contains("__quantum__qis__rz__body(double 106.0"),
        "top-level set should specialize f(5) to Bar -> 106.0; got:\n{top_level_qir}"
    );
    assert!(
        operand_block_qir.contains("__quantum__qis__rz__body(double 106.0"),
        "operand-position set should specialize f(5) to Bar -> 106.0; got:\n{operand_block_qir}"
    );
    assert!(
        !operand_block_qir.contains("__quantum__qis__rz__body(double 6.0"),
        "operand-position set must not specialize f(5) to Foo -> 6.0; got:\n{operand_block_qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `false and { .. }` is NOT
/// executed at runtime, so `f(5)` must keep the reaching definition `Foo`
/// (`Foo(5) + 1 = 6.0`).
#[test]
fn defunctionalize_binop_andl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let b = false and { set f = Bar; true };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `and` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `and` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `true or { .. }` is NOT
/// executed at runtime, so `f(5)` must keep `Foo` -> 6.0 (same `BinOp` arm).
#[test]
fn defunctionalize_binop_orl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let b = true or { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `or` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `or` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b and= { .. }` is NOT executed when the LHS short-circuits, so `f(5)`
/// must keep `Foo` -> 6.0 (distinct `AssignOp` arm, same fork/join fix).
#[test]
fn defunctionalize_assignop_andl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                mutable b = false;
                set b and= { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `and=` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `and=` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b or= { .. }` is NOT executed when the LHS short-circuits, so `f(5)`
/// must keep `Foo` -> 6.0.
#[test]
fn defunctionalize_assignop_orl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                mutable b = true;
                set b or= { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `or=` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `or=` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// In `(new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 }`, runtime
/// evaluates the replace operand (`set f = Bar`) BEFORE the record operand
/// (containing `f(5)`), so `f(5)` specializes to `Bar(5) = 105` and the
/// subsequent `+ 1` produces the rotation angle `106.0`.
#[test]
fn defunctionalize_update_field_replace_first_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            struct Rec { A : Int, B : Int }
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let r = (new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 };
                Rz(IntAsDouble(r.A + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 106.0"),
        "UpdateField replace-then-record must specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0"),
        "UpdateField must not specialize f(5) to Foo -> 6.0; got:\n{qir}"
    );
}

/// In `[f(5), 0] w/ 1 <- { set f = Bar; 0 }`, runtime evaluates the index then
/// the replace operand (`set f = Bar`) BEFORE the container operand (containing
/// `f(5)`), so `f(5)` specializes to `Bar(5) = 105` and the subsequent `+ 1`
/// produces the rotation angle `106.0`.
#[test]
fn defunctionalize_update_index_replace_first_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let arr = [f(5), 0] w/ 1 <- { set f = Bar; 0 };
                Rz(IntAsDouble(arr[0] + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 106.0"),
        "UpdateIndex index-replace-container must specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0"),
        "UpdateIndex must not specialize f(5) to Foo -> 6.0; got:\n{qir}"
    );
}

/// `f` is reassigned to `Bar` inside the right-hand side of a short-circuiting
/// `and` whose condition is a measurement outcome, so whether the reassignment
/// happens is only known at runtime:
///
/// ```qsharp
/// mutable f = Foo;
/// let cond = MResetZ(q) == Zero;
/// let b = cond and { set f = Bar; true };  // RHS runs only when cond is true
/// Rz(IntAsDouble(f(5) + 1), q);
/// ```
///
/// Because `and` only evaluates its right-hand side when the left side is true,
/// the later `f(5)` must call:
///   * `Bar` when `cond` is true  -> `Bar(5) + 1 = 106`,
///   * `Foo` when `cond` is false -> `Foo(5) + 1 = 7`.
///
/// The two outcomes cannot be folded to a single constant, so the generated QIR
/// keeps both branches and merges them with a `phi i64 [105, ...], [6, ...]`
/// (the `+ 1` is added after the merge), then passes that runtime value to `Rz`
/// as `double %...`. This verifies the call is dispatched against the correct
/// per-branch definition of `f` and that the measurement-dependent condition
/// still produces valid QIR rather than failing codegen.
#[test]
fn defunctionalize_binop_andl_runtime_dynamic_branch_split_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x + 1 }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let cond = MResetZ(q) == Zero;
                let b = cond and { set f = Bar; true };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    // cond-true arm runs the RHS -> f = Bar -> Bar(5) = 105.
    assert!(
        qir.contains("phi i64 [105,"),
        "cond-true arm must resolve f(5) to Bar (Bar(5) = 105); got:\n{qir}"
    );
    // cond-false arm short-circuits past the RHS -> f stays Foo -> Foo(5) = 6.
    assert!(
        qir.contains(", [6,"),
        "cond-false arm must resolve f(5) to Foo (Foo(5) = 6); got:\n{qir}"
    );
    // The Rz angle is the runtime branch-split result, never a folded constant.
    assert!(
        qir.contains("__quantum__qis__rz__body(double %"),
        "Rz angle must be the runtime phi value, not a folded constant; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0")
            && !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "runtime-dynamic dispatch must not constant-fold the Rz angle; got:\n{qir}"
    );
}

/// A nested conditional that selects between callables must reference each
/// source branch condition exactly once. The outer condition here is a
/// side-effecting measurement:
///
/// ```qsharp
/// let u = if MResetZ(ctl) == One { if flag { Foo } else { Bar } } else { Baz };
/// u(target);
/// ```
///
/// The previous flat left-associated `AndL` dispatch fold referenced the outer
/// condition twice (once in the `outer and inner` guard for `Foo` and once in
/// the standalone `outer` guard for `Bar`), so the measurement was emitted
/// twice. The pipeline now normalizes the condition before defunctionalization
/// and builds nested dispatch, so the program must contain exactly one
/// `mresetz` measurement instruction.
#[test]
fn defunctionalize_nested_condition_dispatch_evaluates_measurement_once() {
    let source = r#"
        namespace Test {
            operation Foo(q : Qubit) : Unit { Rx(1.0, q); }
            operation Bar(q : Qubit) : Unit { Rx(2.0, q); }
            operation Baz(q : Qubit) : Unit { Rx(3.0, q); }
            @EntryPoint()
            operation Main() : Unit {
                use ctl = Qubit();
                use target = Qubit();
                let flag = true;
                let u = if MResetZ(ctl) == One { if flag { Foo } else { Bar } } else { Baz };
                u(target);
            }
        }
    "#;
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    let measurement_count = qir
        .matches("call void @__quantum__qis__mresetz__body")
        .count();
    assert_eq!(
        measurement_count, 1,
        "side-effecting outer condition must be measured exactly once; got:\n{qir}"
    );
}

#[test]
fn unsupported_profile_patterns_return_pass_errors() {
    let res = compile_source_to_qir_result(
        indoc::indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    use q = Qubit();
                    mutable x = 1;
                    if MResetZ(q) == One {
                        set x = 2;
                    }
                    x
                }
            }
        "#},
        TargetCapabilityFlags::Adaptive,
    );

    let errors = res.expect_err("expected capability error");
    assert!(!errors.is_empty(), "expected at least one error");
    assert!(
        errors
            .iter()
            .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
        "expected pass-derived codegen readiness errors, got {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("cannot use a dynamic integer value")),
        "expected a dynamic integer capability diagnostic, got {errors:?}"
    );
}

#[test]
fn struct_copy_preserves_source_before_mutating_replacement_in_base_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            struct Data { First : Int, Second : Int }
            @EntryPoint()
            operation Main() : Unit {
                mutable original = new Data { First = 7, Second = 2 };
                let copied = new Data {
                    ...original,
                    First = { set original = new Data { First = 13, Second = 5 }; 6 }
                };
                if copied.Second != 2 { fail "wrong copy snapshot"; }
            }
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert!(qir.contains("call void @__quantum__rt__tuple_record_output(i64 0"));
}

#[test]
fn qir_generation_succeeds_for_struct_copy_update() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                struct Point3d { X : Double, Y : Double, Z : Double }

                let point = new Point3d { X = 1.0, Y = 2.0, Z = 3.0 };
                let point2 = new Point3d { ...point, Z = 4.0 };
                let x : Double = point2.X;
            }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]]
        .assert_eq(&qir);
}

// Regression test: a callable-typed local used ONLY inside a
// live struct field must survive the defunctionalize pass. Before the
// walk_utils `Struct` recursion fix, defunctionalize's dead-callable-local
// prune skipped recursing into `Struct` field initializers, so `f` (whose
// only use is `new Holder { Cb = f }`) appeared dead and was removed, leaving
// a dangling `Var(Res::Local)` that crashed the downstream codegen pipeline.
// This exercises the full pipeline (FIR transforms -> partial eval -> QIR) and
// observes that QIR generation succeeds and produces the expected classical
// result (`h.Cb(3)` == 4).
#[test]
fn callable_local_in_struct_field_generates_qir() {
    let source = r#"
        namespace Test {
            struct Holder { Cb : (Int => Int) }
            operation Pick(arr : (Int => Int)[]) : Holder {
                let f = arr[0];
                new Holder { Cb = f }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops : (Int => Int)[] = [AddOne];
                let h = Pick(ops);
                if h.Cb(3) == 4 {
                    X(q);
                }
                MResetZ(q)
            }
            operation AddOne(x : Int) : Int { x + 1 }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn deutsch_jozsa_sample_shape_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Diagnostics.*;
            import Std.Math.*;
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Bool[] {
                let functionsToTest = [
                    SimpleConstantBoolF,
                    SimpleBalancedBoolF,
                    ConstantBoolF,
                    BalancedBoolF
                ];

                mutable results = [];
                for fn in functionsToTest {
                    let isConstant = DeutschJozsa(fn, 3);
                    set results += [isConstant];
                }

                return results;
            }

            operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                use queryRegister = Qubit[n];
                use target = Qubit();
                X(target);
                H(target);
                within {
                    for q in queryRegister {
                        H(q);
                    }
                } apply {
                    Uf(queryRegister, target);
                }

                mutable result = true;
                for q in queryRegister {
                    if MResetZ(q) == One {
                        set result = false;
                    }
                }

                Reset(target);
                return result;
            }

            operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                X(target);
            }

            operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                CX(args[0], target);
            }

            operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                for i in 0..(2^Length(args)) - 1 {
                    ApplyControlledOnInt(i, X, args, target);
                }
            }

            operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                for i in 0..2..(2^Length(args)) - 1 {
                    ApplyControlledOnInt(i, X, args, target);
                }
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive
            | TargetCapabilityFlags::IntegerComputations
            | TargetCapabilityFlags::FloatingPointComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0b\00"
        @2 = internal constant [6 x i8] c"2_a1b\00"
        @3 = internal constant [6 x i8] c"3_a2b\00"
        @4 = internal constant [6 x i8] c"4_a3b\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_9 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_9, label %block_1, label %block_2
        block_1:
          br label %block_2
        block_2:
          %var_191 = phi i1 [true, %block_0], [false, %block_1]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_11 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          br i1 %var_11, label %block_3, label %block_4
        block_3:
          br label %block_4
        block_4:
          %var_192 = phi i1 [%var_191, %block_2], [false, %block_3]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          %var_13 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
          br i1 %var_13, label %block_5, label %block_6
        block_5:
          br label %block_6
        block_6:
          %var_193 = phi i1 [%var_192, %block_4], [false, %block_5]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
          %var_26 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
          br i1 %var_26, label %block_7, label %block_8
        block_7:
          br label %block_8
        block_8:
          %var_194 = phi i1 [true, %block_6], [false, %block_7]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
          %var_28 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
          br i1 %var_28, label %block_9, label %block_10
        block_9:
          br label %block_10
        block_10:
          %var_195 = phi i1 [%var_194, %block_8], [false, %block_9]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
          %var_30 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
          br i1 %var_30, label %block_11, label %block_12
        block_11:
          br label %block_12
        block_12:
          %var_196 = phi i1 [%var_195, %block_10], [false, %block_11]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
          %var_124 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
          br i1 %var_124, label %block_13, label %block_14
        block_13:
          br label %block_14
        block_14:
          %var_197 = phi i1 [true, %block_12], [false, %block_13]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
          %var_126 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
          br i1 %var_126, label %block_15, label %block_16
        block_15:
          br label %block_16
        block_16:
          %var_198 = phi i1 [%var_197, %block_14], [false, %block_15]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
          %var_128 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
          br i1 %var_128, label %block_17, label %block_18
        block_17:
          br label %block_18
        block_18:
          %var_199 = phi i1 [%var_198, %block_16], [false, %block_17]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
          %var_182 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
          br i1 %var_182, label %block_19, label %block_20
        block_19:
          br label %block_20
        block_20:
          %var_200 = phi i1 [true, %block_18], [false, %block_19]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 10 to %Result*))
          %var_184 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 10 to %Result*))
          br i1 %var_184, label %block_21, label %block_22
        block_21:
          br label %block_22
        block_22:
          %var_201 = phi i1 [%var_200, %block_20], [false, %block_21]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 11 to %Result*))
          %var_186 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 11 to %Result*))
          br i1 %var_186, label %block_23, label %block_24
        block_23:
          br label %block_24
        block_24:
          %var_202 = phi i1 [%var_201, %block_22], [false, %block_23]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_193, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_196, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_199, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_202, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare zeroext i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="12" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn simple_phase_estimation_sample_shape_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Main() : Result[] {
                use state = Qubit();
                use phase = Qubit[3];

                X(state);

                let oracle = ApplyOperationPowerCA(_, qs => U(qs[0]), _);
                ApplyQPE(oracle, [state], phase);

                let results = MeasureEachZ(phase);

                Reset(state);
                ResetAll(phase);

                Std.Arrays.Reversed(results)
            }

            operation U(q : Qubit) : Unit is Ctl + Adj {
                Rz(Std.Math.PI() / 3.0, q);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive
            | TargetCapabilityFlags::IntegerComputations
            | TargetCapabilityFlags::FloatingPointComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.39269908169872414, %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.39269908169872414, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.39269908169872414, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn explicit_return_tuple_keeps_dynamic_integer_output() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : (Int, Bool) {
                use q = Qubit();
                mutable a = 0;
                if MResetZ(q) == Zero {
                    a = 1;
                } else {
                    a = 2;
                }

                use p = Qubit();
                return (a, MResetZ(p) == One);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_t\00"
        @1 = internal constant [6 x i8] c"1_t0i\00"
        @2 = internal constant [6 x i8] c"2_t1b\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_2 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          %var_3 = icmp eq i1 %var_2, false
          br i1 %var_3, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_13 = phi i64 [1, %block_1], [2, %block_2]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_6 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__int_record_output(i64 %var_13, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_6, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare zeroext i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        declare void @__quantum__rt__int_record_output(i64, i8*)

        declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn result_array_helper_return_survives_adaptive_codegen_prep() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Result[] {
                use register = Qubit[2];
                return MResetZ2Register(register);
            }

            operation MResetZ2Register(register : Qubit[]) : Result[] {
                return [MResetZ(register[0]), MResetZ(register[1])];
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn higher_order_closure_captures_are_threaded_into_specialized_calls() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Canon.*;
            import Std.Measurement.*;

            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Result[] {
                op(register);
                return MResetEachZ(register);
            }

            @EntryPoint()
            operation Main() : Result[] {
                use register = Qubit[2];
                return ApplyOp(register => Shifted(1, register), register);
            }

            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn two_callable_hof_closure_preserves_array_arg_threading() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Arrays.*;
            import Std.Canon.*;
            import Std.Convert.*;
            import Std.Measurement.*;

            operation Outer(Ufstar : (Qubit[] => Unit), Ug : (Qubit[] => Unit), n : Int) : Result[] {
                use qubits = Qubit[n];
                Ug(qubits);
                return MResetEachZ(qubits);
            }

            operation Empty(register : Qubit[]) : Unit {
            }

            operation ShiftedSimple(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }

            @EntryPoint()
            operation Main() : Result[] {
                let bits = [true, false];
                let shift = BoolArrayAsInt(bits);
                let n = Length(bits);
                return Outer(Empty, register => ShiftedSimple(shift, register), n);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn seed_entry_with_callables_preserves_callable_source_packages() {
    let capabilities = TargetCapabilityFlags::all();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];
    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        source_map_from_source("namespace Test { operation Destination() : Unit {} }"),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let destination_hir_package = store.insert(unit);
    let (mut fir_store, destination_package, _) = lower_hir_to_fir(&store, destination_hir_package);
    let foreign_package = qsc_lowerer::map_hir_package_to_fir(std_id);

    let expected_spans: FxHashMap<StoreItemId, qsc_fir::fir::PackageSpan> = fir_store
        .get(foreign_package)
        .items
        .values()
        .filter_map(|item| match &item.kind {
            FirItemKind::Callable(decl) if decl.span.package == foreign_package => Some((
                StoreItemId {
                    package: foreign_package,
                    item: item.id,
                },
                decl.span,
            )),
            FirItemKind::Callable(_) | FirItemKind::Ty(..) => None,
        })
        .take(2)
        .collect();
    assert_eq!(expected_spans.len(), 2, "expected two foreign callables");
    let callables: FxHashSet<_> = expected_spans.keys().copied().collect();

    seed_entry_with_callables(&mut fir_store, destination_package, &callables);

    let destination = fir_store.get(destination_package);
    let entry = destination
        .entry
        .expect("plural seed should create an entry expression");
    let FirExprKind::Tuple(entry_exprs) = &destination.get_expr(entry).kind else {
        panic!("two callable seeds should create a tuple entry");
    };
    assert_eq!(entry_exprs.len(), expected_spans.len());
    for expr_id in entry_exprs {
        let expr = destination.get_expr(*expr_id);
        let FirExprKind::Var(FirRes::Item(item), _) = &expr.kind else {
            panic!("seeded entry child should be a global callable variable");
        };
        let callable = StoreItemId {
            package: item.package,
            item: item.item,
        };
        assert_eq!(expr.span, expected_spans[&callable]);
        assert_ne!(expr.span.package, destination_package);
    }
}

#[test]
fn callable_args_with_arrow_input_survives_dce() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation MyOp(q : Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    // Find ApplyOp and MyOp by name in the HIR package.
    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_op_local = None;
    let mut my_op_local = None;
    for (local_id, item) in hir_package.items.iter() {
        if let ItemKind::Callable(decl) = &item.kind {
            if decl.name.name.as_ref() == "ApplyOp" {
                apply_op_local = Some(local_id);
            } else if decl.name.name.as_ref() == "MyOp" {
                my_op_local = Some(local_id);
            }
        }
    }
    let apply_op_local = apply_op_local.expect("ApplyOp should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_op_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_op_local,
    };

    // Construct Value::Global for MyOp using FIR StoreItemId.
    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    // The synthetic Call path makes ApplyOp entry-reachable. Defunc specializes
    // it to ApplyOp{MyOp}, and the pipeline transforms it fully into a
    // self-contained entry that is consumed directly.
    let (codegen_fir, _backend) =
        prepare_codegen_fir_from_callable_args(&store, apply_op_hir_id, &my_op_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with arrow-input should survive DCE, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("QIR generation from arrow-input callable should succeed: {e:?}"));

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
    .assert_eq(&qir);
}

#[test]
fn callable_args_with_udt_wrapped_arrow_survives_dce() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Unit {
                use q = Qubit();
                cfg::Op(q);
            }
            operation MyOp(q: Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    // Build a Config UDT value: Config(MyOp, 42)
    // UDT values are Value::Tuple(Rc<[Value]>, Option<Rc<StoreItemId>>)
    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(42)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let result =
        prepare_codegen_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities);
    match result {
        Ok(_) => {}
        Err(errors) => panic!(
            "callable-args with UDT-wrapped arrow should survive DCE, got: {}",
            format_interpret_errors(errors)
        ),
    }
}

#[test]
fn callable_with_udt_wrapped_arrow_generates_qir_via_callable_args() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Op(q);
                MResetZ(q)
            }
            operation MyOp(q: Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(42)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let (codegen_fir, _backend) =
        prepare_codegen_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with UDT-wrapped arrow should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("QIR generation from UDT-wrapped arrow should succeed: {e:?}"));

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn callable_with_nested_udt_wrapped_arrow_generates_qir_via_callable_args() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpWrapper = (Op: Qubit => Unit);
            newtype Config = (Inner: OpWrapper, Count: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Inner::Op(q);
                MResetZ(q)
            }
            operation MyOp(q: Qubit) : Unit { X(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(5)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store,
        apply_hir_id,
        &config_value,
        capabilities,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "callable-args with nested UDT-wrapped arrow should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| {
        panic!("QIR generation from nested UDT-wrapped arrow should succeed: {e:?}")
    });

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]].assert_eq(&qir);
}

// ---------------------------------------------------------------------------
// Synthetic-path and fallback-path coverage for callable-args codegen
// ---------------------------------------------------------------------------

/// Matches a callable's actual name against a requested lookup name. The `.lambda` sentinel
/// matches any lifted lambda, whose real name carries a volatile `.lambda_<item-id>` suffix.
fn callable_name_matches(actual: &str, requested: &str) -> bool {
    if requested == ".lambda" {
        actual.starts_with(".lambda")
    } else {
        actual == requested
    }
}

/// Helper: compile a lib package, locate named items, and return (`store`, `package_id`, `items_map`).
/// `item_names` maps display names to a bool: true = Callable, false = Ty (UDT).
fn compile_and_locate_items(
    source: &str,
    item_names: &[(&str, bool)],
    capabilities: TargetCapabilityFlags,
) -> (
    crate::PackageStore,
    PackageId,
    FxHashMap<String, qsc_hir::hir::LocalItemId>,
) {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];
    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut found = FxHashMap::default();
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) => {
                for &(name, is_callable) in item_names {
                    if is_callable && callable_name_matches(decl.name.name.as_ref(), name) {
                        found.insert(name.to_string(), local_id);
                    }
                }
            }
            ItemKind::Ty(name, _) => {
                for &(item_name, is_callable) in item_names {
                    if !is_callable && name.name.as_ref() == item_name {
                        found.insert(item_name.to_string(), local_id);
                    }
                }
            }
            _ => {}
        }
    }
    for &(name, _) in item_names {
        assert!(
            found.contains_key(name),
            "{name} should exist in HIR package"
        );
    }
    (store, package_id, found)
}

/// Maps a HIR local item ID in the test package to its corresponding FIR item ID.
fn fir_id_for(
    package_id: PackageId,
    local_id: qsc_hir::hir::LocalItemId,
) -> qsc_fir::fir::StoreItemId {
    qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(local_id),
    }
}

/// Builds a HIR item ID from a test package ID and local item ID.
fn hir_id_for(package_id: PackageId, local_id: qsc_hir::hir::LocalItemId) -> qsc_hir::hir::ItemId {
    qsc_hir::hir::ItemId {
        package: package_id,
        item: local_id,
    }
}

/// Prepares callable arguments for codegen, then emits QIR with `fir_to_qir` for
/// `SyntheticEntry` or `fir_to_qir_from_callable` for `ReinvokeOriginal`.
fn callable_args_to_qir(
    store: &crate::PackageStore,
    package_id: PackageId,
    target_local: qsc_hir::hir::LocalItemId,
    args: &Value,
    capabilities: TargetCapabilityFlags,
) -> String {
    let target_hir = hir_id_for(package_id, target_local);
    let (codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(store, target_hir, args, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "prepare_codegen_fir_from_callable_args failed: {}",
                    format_interpret_errors(errors)
                )
            });
    match backend {
        CallableArgsBackend::SyntheticEntry => {
            let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
            qsc_codegen::qir::fir_to_qir(
                &codegen_fir.fir_store,
                capabilities,
                &codegen_fir.compute_properties,
                &entry,
            )
            .unwrap_or_else(|e| panic!("fir_to_qir failed: {e:?}"))
        }
        CallableArgsBackend::ReinvokeOriginal { callable, args } => {
            qsc_codegen::qir::fir_to_qir_from_callable(
                &codegen_fir.fir_store,
                capabilities,
                &codegen_fir.compute_properties,
                callable,
                args,
            )
            .unwrap_or_else(|e| panic!("fir_to_qir_from_callable failed: {e:?}"))
        }
    }
}

fn eval_fragments(interpreter: &mut crate::interpret::Interpreter, source: &str) -> Value {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    interpreter
        .eval_fragments(&mut receiver, source)
        .unwrap_or_else(|errors| {
            panic!("eval_fragments failed: {}", format_interpret_errors(errors))
        })
}

fn interpreter_with_capabilities(
    capabilities: TargetCapabilityFlags,
) -> crate::interpret::Interpreter {
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies = &[(std_id, None)];
    crate::interpret::Interpreter::new(
        SourceMap::default(),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        dependencies,
        Default::default(),
    )
    .expect("interpreter should be created")
}

fn runtime_argument_global(interpreter: &crate::interpret::Interpreter, name: &str) -> Value {
    interpreter
        .user_globals()
        .into_iter()
        .chain(interpreter.source_globals())
        .find(|global| global.name.as_ref() == name)
        .expect("source global")
        .value
}

#[test]
fn runtime_callable_argument_generic_bindings_are_consistent_and_scoped() {
    let source = r#"
        namespace Test {
            function Identity<'T>(x : 'T) : 'T { x }
            operation Wrong(f : Int -> Double) : Unit { let _ = f(0); }
            operation Pair<'T>(value : 'T, f : 'T -> 'T) : Unit { let _ = f(value); }
            operation Independent<'T>(value : 'T, f : Int -> Int) : Unit { let _ = (value, f(0)); }
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let identity = runtime_argument_global(&interpreter, "Identity");
        let errors = interpreter
            .qirgen("Test.Wrong(Test.Identity)")
            .expect_err("source typing must reject inconsistent generic arguments");
        assert!(format!("{errors:?}").contains("TyMismatch"));

        let pair = runtime_argument_global(&interpreter, "Pair");
        let args = Value::Tuple(vec![Value::Int(1), identity.clone()].into(), None);
        interpreter
            .qirgen_from_callable(&pair, args)
            .expect("shared type requirements should infer Int");

        let independent = runtime_argument_global(&interpreter, "Independent");
        let args = Value::Tuple(vec![Value::Double(0.5), identity].into(), None);
        interpreter
            .qirgen_from_callable(&independent, args)
            .expect("distinct declarations' type parameters must not conflict");
    }
}

#[test]
fn runtime_callable_argument_valid_values_match_source_qir() {
    let source = r#"
        namespace Test {
            function Good(x : Int) : Int { x }
            function Identity<'T>(x : 'T) : 'T { x }
            function Add(x : Int, y : Int) : Int { x + y }
            operation Run(f : Int -> Int) : Result {
                use q = Qubit();
                if f(0) == 0 { X(q); }
                MResetZ(q)
            }
            operation Each(fs : (Int -> Int)[]) : Result {
                use q = Qubit();
                for f in fs { if f(0) == 0 { X(q); } }
                MResetZ(q)
            }
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        for (target, argument) in [
            ("Test.Run", "Test.Good"),
            ("Test.Run", "Test.Identity"),
            ("Test.Run", "Test.Add(0, _)"),
            ("Test.Each", "[]"),
            ("Test.Each", "[Test.Identity]"),
        ] {
            let source_call = format!("{target}({argument})");
            let target = eval_fragments(&mut interpreter, target);
            let args = match argument {
                "Test.Identity" => runtime_argument_global(&interpreter, "Identity"),
                "[]" => Value::Array(Vec::new().into()),
                "[Test.Identity]" => {
                    Value::Array(vec![runtime_argument_global(&interpreter, "Identity")].into())
                }
                _ => eval_fragments(&mut interpreter, argument),
            };
            let qir = interpreter
                .qirgen_from_callable(&target, args)
                .expect("valid callable arguments must still compile");
            let source_qir = interpreter.qirgen(&source_call).expect("source invocation");
            assert_eq!(qir, source_qir, "{profile:?}: {source_call}");
        }
    }
}

#[test]
fn runtime_callable_argument_functor_shapes_match_source_qir() {
    let source = r#"
        namespace Test {
            operation Extra(q : Qubit) : Unit is Adj + Ctl { X(q); }
            operation Run(f : Qubit => Unit is Adj) : Result {
                use q = Qubit();
                f(q);
                MResetZ(q)
            }
            operation RunControlled(f : (Qubit[], Qubit) => Unit) : Result {
                use q = Qubit();
                f([], q);
                MResetZ(q)
            }
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        for (target, argument) in [
            ("Test.Run", "Test.Extra"),
            ("Test.Run", "Adjoint Test.Extra"),
            ("Test.RunControlled", "Controlled Test.Extra"),
        ] {
            let source_call = format!("{target}({argument})");
            let target = eval_fragments(&mut interpreter, target);
            let args = eval_fragments(&mut interpreter, argument);
            let qir = interpreter
                .qirgen_from_callable(&target, args)
                .expect("supported extra functors and runtime control layers must remain valid");
            let source_qir = interpreter.qirgen(&source_call).expect("source invocation");
            assert_eq!(qir, source_qir, "{profile:?}: {source_call}");
        }
    }
}

#[test]
fn runtime_controlled_generic_callable_matches_source_qir() {
    check_runtime_controlled_generic_callable("Controlled", 1, true);
}

#[test]
fn runtime_controlled_adjoint_generic_callable_matches_source_qir() {
    check_runtime_controlled_generic_callable("Controlled Adjoint", 1, false);
}

#[test]
fn runtime_nested_controlled_generic_callable_matches_source_qir() {
    check_runtime_controlled_generic_callable("Controlled Controlled", 2, true);
}

#[test]
fn runtime_nested_controlled_adjoint_generic_callable_matches_source_qir() {
    check_runtime_controlled_generic_callable("Controlled Controlled Adjoint", 2, false);
}

fn check_runtime_controlled_generic_callable(
    functor: &str,
    controlled_depth: usize,
    expected_result: bool,
) {
    let mut input = "(Int, Int, Qubit)".to_string();
    let mut argument = "(7, 8, q)".to_string();
    for index in (0..controlled_depth).rev() {
        input = format!("(Qubit[], {input})");
        argument = format!("([controls[{index}]], {argument})");
    }
    for required_functors in ["", "is Adj + Ctl"] {
        let source = format!(
            r#"
            operation Generic<'T: Eq>(left : 'T, right : 'T, q : Qubit) : Unit is Adj + Ctl {{
                if left != right {{ S(q); }}
            }}
            operation Run(f : {input} => Unit {required_functors}) : Result {{
                use controls = Qubit[{controlled_depth}];
                for control in controls {{ X(control); }}
                use q = Qubit();
                H(q);
                S(q);
                f{argument};
                H(q);
                for control in controls {{ Reset(control); }}
                MResetZ(q)
            }}
            "#
        );
        for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
            let context = format!("{profile:?}, {functor}, required={required_functors:?}");
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(&mut interpreter, &source);
            let target = eval_fragments(&mut interpreter, "Run");
            let argument = eval_fragments(
                &mut interpreter,
                &format!("{{ let f : {input} => Unit is Adj + Ctl = {functor} Generic; f }}"),
            );
            assert!(
                matches!(argument, Value::Global(_, _)),
                "{context}: must exercise a runtime generic global"
            );
            let mut output = Cursor::new(Vec::new());
            assert_eq!(
                interpreter
                    .invoke(
                        &mut CursorReceiver::new(&mut output),
                        target.clone(),
                        argument.clone(),
                    )
                    .unwrap_or_else(|errors| {
                        panic!(
                            "{context}: source execution: {}",
                            format_interpret_errors(errors)
                        );
                    }),
                Value::Result(qsc_eval::val::Result::Val(expected_result)),
                "{context}: preserve the phase of the functored operation"
            );
            let actual = interpreter
                .qirgen_from_callable(&target, argument)
                .unwrap_or_else(|errors| {
                    panic!(
                        "{context}: runtime codegen: {}",
                        format_interpret_errors(errors)
                    );
                });
            let expected = interpreter
                .qirgen(&format!("Run({functor} Generic)"))
                .unwrap_or_else(|errors| {
                    panic!(
                        "{context}: source codegen: {}",
                        format_interpret_errors(errors)
                    );
                });
            assert_eq!(
                actual, expected,
                "{context}: preserve control layers and type arguments"
            );
        }
    }
}

#[test]
fn runtime_type_inference_resolves_functors_from_later_reader() {
    check_runtime_nested_functor_inference(
        "(((Qubit => Unit is Adj + Ctl), Qubit) => Unit is Adj + Ctl)",
        "Apply",
        "value(X, q);",
    );
}

#[test]
fn runtime_type_inference_resolves_functors_inside_array_values() {
    check_runtime_nested_functor_inference(
        "(((Qubit => Unit is Adj + Ctl), Qubit) => Unit is Adj + Ctl)[]",
        "[Apply]",
        "value[0](X, q);",
    );
}

#[test]
fn runtime_type_inference_resolves_functors_inside_tuple_values() {
    check_runtime_nested_functor_inference(
        "((((Qubit => Unit is Adj + Ctl), Qubit) => Unit is Adj + Ctl), Int)",
        "(Apply, 7)",
        "let (op, n) = value; if n == 7 { op(X, q); }",
    );
}

fn check_runtime_nested_functor_inference(value_type: &str, value: &str, invoke: &str) {
    let source = format!(
        r#"
        operation Apply(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
            op(q);
        }}
        function MakeReader() : ({value_type} => Result) {{
            value => {{
                use q = Qubit();
                {invoke}
                MResetZ(q)
            }}
        }}
        operation Forward<'T>(value : 'T, read : 'T => Result) : Result {{ read(value) }}
        operation Reverse<'T>(read : 'T => Result, value : 'T) : Result {{ read(value) }}
        "#
    );
    for (target, arguments) in [
        ("Forward", format!("({value}, MakeReader())")),
        ("Reverse", format!("(MakeReader(), {value})")),
    ] {
        check_runtime_type_inference(
            &source,
            target,
            &arguments,
            &Value::Result(qsc_eval::val::Result::Val(true)),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

#[test]
fn runtime_type_inference_keeps_functor_instantiations_independent() {
    check_runtime_type_inference(
        r#"
        operation Flip(qs : Qubit[]) : Unit is Adj + Ctl { X(qs[0]); }
        operation Apply(op : Qubit[] => Unit, qs : Qubit[]) : Unit { op(qs); }
        function AdjointReader() : ((((Qubit[] => Unit is Adj), Qubit[]) => Unit) => Result) {
            f => { use qs = Qubit[1]; f(Flip, qs); MResetZ(qs[0]) }
        }
        function ControlledReader() : ((((Qubit[] => Unit is Ctl), Qubit[]) => Unit) => Result) {
            f => { use qs = Qubit[1]; f(Flip, qs); MResetZ(qs[0]) }
        }
        operation Run<'T, 'U>(left : 'T, readLeft : 'T => Result, readRight : 'U => Result, right : 'U) : Result[] {
            [readLeft(left), readRight(right)]
        }
        "#,
        "Run",
        "(Apply, AdjointReader(), ControlledReader(), Apply)",
        &Value::Array(vec![Value::Result(qsc_eval::val::Result::Val(true)); 2].into()),
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
    );
}

#[test]
fn runtime_functor_inference_accepts_mixed_capabilities_in_both_array_orders() {
    let source = r#"
        operation Strong<'T>(value : 'T, q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Weak<'T>(value : 'T, q : Qubit) : Unit is Adj { X(q); }
        function AsAdjoint(op : (Int, Qubit) => Unit is Adj) : (Int, Qubit) => Unit is Adj { op }
        operation Run(ops : ((Int, Qubit) => Unit is Adj)[]) : Result {
            use q = Qubit();
            for op in ops { Adjoint op(7, q); }
            MResetZ(q)
        }
    "#;
    for argument in [
        "[AsAdjoint(Strong), AsAdjoint(Weak)]",
        "[AsAdjoint(Weak), AsAdjoint(Strong)]",
    ] {
        check_runtime_type_inference(
            source,
            "Run",
            argument,
            &Value::Result(qsc_eval::val::Result::Val(false)),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

#[test]
fn runtime_common_callable_type_supports_generic_arrays_in_both_orders() {
    for argument in [
        "([AsAdjoint(Strong), AsAdjoint(Weak)], MakeReader())",
        "([AsAdjoint(Weak), AsAdjoint(Strong)], MakeReader())",
    ] {
        check_runtime_common_callable_type("Run", argument);
    }
}

#[test]
fn runtime_common_callable_type_supports_scalar_arguments_in_both_orders() {
    for argument in [
        "(AsAdjoint(Strong), AsAdjoint(Weak), MakeReader())",
        "(AsAdjoint(Weak), AsAdjoint(Strong), MakeReader())",
    ] {
        check_runtime_common_callable_type("Pair", argument);
    }
}

#[test]
fn runtime_common_callable_type_supports_reader_first() {
    for argument in [
        "(MakeReader(), [AsAdjoint(Strong), AsAdjoint(Weak)])",
        "(MakeReader(), [AsAdjoint(Weak), AsAdjoint(Strong)])",
    ] {
        check_runtime_common_callable_type("ReaderFirst", argument);
    }
}

const COMMON_CALLABLE_TYPE_SOURCE: &str = r#"
    operation Strong(q : Qubit) : Unit is Adj + Ctl { X(q); }
    operation Weak(q : Qubit) : Unit is Adj { X(q); }
    operation ControlledOnly(q : Qubit) : Unit is Ctl { X(q); }
    function AsAdjoint(op : Qubit => Unit is Adj) : Qubit => Unit is Adj { op }
    function AsBody(op : Qubit => Unit) : Qubit => Unit { op }
    function MakeReader() : ((Qubit => Unit is Adj) => Result) {
        op => { use q = Qubit(); Adjoint op(q); MResetZ(q) }
    }
    function MakeBodyReader() : ((Qubit => Unit) => Result) {
        op => { use q = Qubit(); op(q); MResetZ(q) }
    }
    function MakeControlledReader() : ((Qubit => Unit is Ctl) => Result) {
        op => { use q = Qubit(); Controlled op([], q); MResetZ(q) }
    }
    function Identity<'V>(value : 'V) : 'V { value }
    operation Run<'T>(values : 'T[], read : 'T => Result) : Result[] {
        mutable results = [];
        for value in values { set results += [read(value)]; }
        results
    }
    operation Pair<'T>(left : 'T, right : 'T, read : 'T => Result) : Result[] {
        [read(left), read(right)]
    }
    operation ReaderFirst<'T>(read : 'T => Result, values : 'T[]) : Result[] {
        mutable results = [];
        for value in values { set results += [read(value)]; }
        results
    }
    operation Map<'T, 'U>(f : 'T -> 'U, value : 'T, read : 'U => Result) : Result {
        read(f(value))
    }
    operation MapReverse<'T, 'U>(value : 'T, read : 'U => Result, f : 'T -> 'U) : Result {
        read(f(value))
    }
"#;

fn check_runtime_common_callable_type(target: &str, argument: &str) {
    check_runtime_type_inference(
        COMMON_CALLABLE_TYPE_SOURCE,
        target,
        argument,
        &Value::Array(vec![Value::Result(qsc_eval::val::Result::Val(true)); 2].into()),
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
    );
}

#[test]
fn runtime_common_callable_type_can_require_no_functors() {
    for argument in [
        "([AsBody(Weak), AsBody(ControlledOnly)], MakeBodyReader())",
        "([AsBody(ControlledOnly), AsBody(Weak)], MakeBodyReader())",
    ] {
        check_runtime_common_callable_type("Run", argument);
    }
}

#[test]
fn runtime_common_callable_type_supports_factories_with_different_output_capabilities() {
    let source = r#"
        operation Strong(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Weak(q : Qubit) : Unit is Adj { X(q); }
        function StrongFactory() : Qubit => Unit is Adj + Ctl { Strong }
        function WeakFactory() : Qubit => Unit is Adj { Weak }
        function AsFactory(f : Unit -> (Qubit => Unit is Adj)) : Unit -> (Qubit => Unit is Adj) { f }
        function MakeReader() : ((Unit -> (Qubit => Unit is Adj)) => Result) {
            factory => { use q = Qubit(); let op = factory(); Adjoint op(q); MResetZ(q) }
        }
        operation Run<'T>(values : 'T[], read : 'T => Result) : Result[] {
            mutable results = [];
            for value in values { set results += [read(value)]; }
            results
        }
    "#;
    for argument in [
        "([AsFactory(StrongFactory), AsFactory(WeakFactory)], MakeReader())",
        "([AsFactory(WeakFactory), AsFactory(StrongFactory)], MakeReader())",
    ] {
        check_runtime_type_inference(
            source,
            "Run",
            argument,
            &Value::Array(vec![Value::Result(qsc_eval::val::Result::Val(true)); 2].into()),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

#[test]
fn runtime_higher_order_consumers_accept_shared_input_capabilities() {
    check_runtime_higher_order_consumers(false);
}

#[test]
fn runtime_higher_order_consumers_accept_reader_first() {
    check_runtime_higher_order_consumers(true);
}

fn check_runtime_higher_order_consumers(reader_first: bool) {
    let source = r#"
        operation Strong(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation NeedsAdjoint(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation NeedsControl(op : Qubit => Unit is Ctl, q : Qubit) : Unit {
            Controlled op([], q);
        }
        function AsConsumer(f : ((Qubit => Unit is Adj + Ctl), Qubit) => Unit) : ((Qubit => Unit is Adj + Ctl), Qubit) => Unit { f }
        function MakeReader() : ((((Qubit => Unit is Adj + Ctl), Qubit) => Unit) => Result) {
            f => { use q = Qubit(); f(Strong, q); MResetZ(q) }
        }
        operation Run<'T>(values : 'T[], read : 'T => Result) : Result[] {
            mutable results = [];
            for value in values { set results += [read(value)]; }
            results
        }
        operation Reverse<'T>(read : 'T => Result, values : 'T[]) : Result[] {
            mutable results = [];
            for value in values { set results += [read(value)]; }
            results
        }
    "#;
    for consumers in [
        "[AsConsumer(NeedsAdjoint), AsConsumer(NeedsControl)]",
        "[AsConsumer(NeedsControl), AsConsumer(NeedsAdjoint)]",
    ] {
        let (target, argument) = if reader_first {
            ("Reverse", format!("(MakeReader(), {consumers})"))
        } else {
            ("Run", format!("({consumers}, MakeReader())"))
        };
        check_runtime_type_inference(
            source,
            target,
            &argument,
            &Value::Array(vec![Value::RESULT_ONE; 2].into()),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

const CAPABILITY_CHAIN_SOURCE: &str = r#"
    operation Weak(q : Qubit) : Unit is Adj { X(q); }
    operation Strong(q : Qubit) : Unit is Ctl { X(q); }
    function Identity<'T>(value : 'T) : 'T { value }
    function Duplicate<'T>(value : 'T) : ('T, 'T) { (value, value) }
    function MakeReader() : ((Qubit => Unit is Ctl) => Result) {
        f => { use q = Qubit(); Controlled f([], q); MResetZ(q) }
    }
    function MakePairReader() : (((Qubit => Unit is Ctl), (Qubit => Unit is Ctl)) => Result) {
        (f, g) => { use q = Qubit(); Controlled f([], q); g(q); MResetZ(q) }
    }
    operation Chain<'T, 'U, 'V>(f : 'T -> 'U, g : 'U -> 'V, value : 'T, read : 'V => Result) : Result {
        read(g(f(value)))
    }
    operation Reverse<'T, 'U, 'V>(read : 'V => Result, value : 'T, g : 'U -> 'V, f : 'T -> 'U) : Result {
        read(g(f(value)))
    }
"#;

#[test]
fn runtime_capability_chain_preserves_supported_control() {
    check_runtime_capability_chain(false);
}

#[test]
fn runtime_capability_chain_with_duplication_preserves_supported_control() {
    check_runtime_capability_chain(true);
}

fn check_runtime_capability_chain(duplicate: bool) {
    let (operation, bound) = ("Strong", "Ctl");
    let (second, output_type, reader) = if duplicate {
        (
            "Duplicate",
            format!("((Qubit => Unit is {bound}), (Qubit => Unit is {bound}))"),
            "MakePairReader()",
        )
    } else {
        (
            "Identity",
            format!("(Qubit => Unit is {bound})"),
            "MakeReader()",
        )
    };
    for (target, arguments) in [
        ("Chain", format!("(first, second, {operation}, {reader})")),
        ("Reverse", format!("({reader}, {operation}, second, first)")),
    ] {
        let expression = format!(
            "{{ let first : (Qubit => Unit is {bound}) -> (Qubit => Unit is {bound}) = Identity;
                let second : (Qubit => Unit is {bound}) -> {output_type} = {second};
                {arguments}
            }}"
        );
        check_runtime_type_inference(
            CAPABILITY_CHAIN_SOURCE,
            target,
            &expression,
            &Value::Result(qsc_eval::val::Result::Val(!duplicate)),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

#[test]
fn runtime_type_inference_reads_later_array_elements() {
    let source = r#"
        operation Run<'T>(values : 'T[][], op : Qubit => Unit) : Result {
            use q = Qubit();
            if Length(values[0]) + Length(values[1]) == 1 { op(q); }
            MResetZ(q)
        }
    "#;
    for argument in ["([[], [7]], X)", "([[7], []], X)"] {
        check_runtime_type_inference(
            source,
            "Run",
            argument,
            &Value::Result(qsc_eval::val::Result::Val(true)),
            &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        );
    }
}

#[test]
fn runtime_type_inference_reads_deeply_nested_array_elements() {
    check_runtime_type_inference(
        r#"
        operation Run<'T>(values : 'T[][][], op : Qubit => Unit) : Result {
            use q = Qubit();
            if Length(values[1][1]) == 1 { op(q); }
            MResetZ(q)
        }
        "#,
        "Run",
        "([[[], []], [[], [7]]], X)",
        &Value::Result(qsc_eval::val::Result::Val(true)),
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
    );
}

#[test]
fn runtime_type_inference_preserves_callback_correlations() {
    check_runtime_type_inference(
        r#"
        function Identity<'V>(value : 'V) : 'V { value }
        operation Run<'T, 'U>(f : 'T -> 'U, value : 'T) : 'U { f(value) }
        "#,
        "Run",
        "{ let f : Int -> Int = Identity; (f, 7) }",
        &Value::Int(7),
        &[Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive],
    );
}

#[test]
fn runtime_type_inference_preserves_callback_correlations_in_reverse_order() {
    check_runtime_type_inference(
        r#"
        function Identity<'V>(value : 'V) : 'V { value }
        operation Run<'T, 'U>(value : 'T, f : 'T -> 'U) : 'U { f(value) }
        "#,
        "Run",
        "{ let f : Int -> Int = Identity; (7, f) }",
        &Value::Int(7),
        &[Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive],
    );
}

#[test]
fn runtime_type_inference_keeps_callback_instantiations_independent() {
    check_runtime_type_inference(
        r#"
        function Identity<'V>(value : 'V) : 'V { value }
        operation Run<'T, 'U>(left : 'T, right : 'U, f : 'T -> 'T, g : 'U -> 'U) : ('T, 'U) {
            (f(left), g(right))
        }
        "#,
        "Run",
        "{ let f : Int -> Int = Identity; let g : Bool -> Bool = Identity; (7, true, f, g) }",
        &Value::Tuple(vec![Value::Int(7), Value::Bool(true)].into(), None),
        &[Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive],
    );
}

fn check_runtime_type_inference(
    source: &str,
    target: &str,
    argument: &str,
    expected: &Value,
    profiles: &[Profile],
) {
    for &profile in profiles {
        let context = format!("{profile:?}, {target}, {argument}");
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(
            &eval_fragments(&mut interpreter, &format!("{target}({argument})")),
            expected,
            "{context}: source result"
        );
        let value = eval_fragments(&mut interpreter, argument);
        let actual = interpreter
            .qirgen_from_callable(&runtime_argument_global(&interpreter, target), value)
            .unwrap_or_else(|errors| {
                panic!("{context}: {}", format_interpret_errors(errors));
            });
        let reference = interpreter
            .qirgen(&format!("{target}({argument})"))
            .expect("source call has concrete type evidence");
        assert_eq!(
            actual, reference,
            "{context}: preserve inferred input and output types"
        );
    }
}

#[test]
fn runtime_callable_entry_rejects_uninferred_type_arguments() {
    use miette::Diagnostic;

    let source = r#"
        function Produce<'T>() : 'T { fail "must not run"; }
        operation Run<'T>(read : Unit -> 'T) : Result {
            use q = Qubit();
            MResetZ(q)
        }
        operation RunEmpty<'T>(values : 'T[], op : Qubit => Unit) : Result {
            use q = Qubit();
            op(q);
            MResetZ(q)
        }
        operation RunNestedEmpty<'T>(values : 'T[][], op : Qubit => Unit) : Result {
            use q = Qubit();
            op(q);
            MResetZ(q)
        }
    "#;
    for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let produce = runtime_argument_global(&interpreter, "Produce");
        let empty_and_op = Value::Tuple(
            vec![
                Value::Array(Vec::new().into()),
                eval_fragments(&mut interpreter, "X"),
            ]
            .into(),
            None,
        );
        let nested_empty_and_op = Value::Tuple(
            vec![
                Value::Array(vec![Value::Array(Vec::new().into()); 2].into()),
                eval_fragments(&mut interpreter, "X"),
            ]
            .into(),
            None,
        );
        for (name, args) in [
            ("Run", produce),
            ("RunEmpty", empty_and_op),
            ("RunNestedEmpty", nested_empty_and_op),
        ] {
            let errors = interpreter
                .qirgen_from_callable(&runtime_argument_global(&interpreter, name), args)
                .expect_err("missing type evidence must not be replaced with Unit");
            assert_eq!(errors.len(), 1, "{profile:?}, {name}: {errors:?}");
            assert_eq!(
                errors[0].code().expect("diagnostic code").to_string(),
                "Qdk.Qsc.EntryPoint.MissingTypeArguments",
                "{profile:?}, {name}"
            );
            let labels: Vec<_> = errors[0].labels().expect("callable source label").collect();
            assert_eq!(labels.len(), 1);
            let span = labels[0].inner();
            assert_eq!(&source[span.offset()..span.offset() + span.len()], name);
            assert!(errors[0].source_code().is_some());
        }
    }
}

#[test]
fn runtime_callable_entry_accepts_concrete_type_evidence() {
    let source = r#"
        function ProduceInt() : Int { 7 }
        function ReadInt(value : Int) : Int { value }
        operation Run<'T>(read : Unit -> 'T) : Result {
            let _ = read();
            use q = Qubit();
            MResetZ(q)
        }
        operation RunEmpty<'T>(values : 'T[], op : Qubit => Unit) : Result {
            use q = Qubit();
            op(q);
            MResetZ(q)
        }
        operation RunInferred<'T>(values : 'T[], read : 'T -> Int) : Result {
            use q = Qubit();
            MResetZ(q)
        }
        operation RunInferredReverse<'T>(read : 'T -> Int, values : 'T[]) : Result {
            use q = Qubit();
            MResetZ(q)
        }
    "#;
    for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        for (name, expression) in [
            ("Run", "ProduceInt"),
            ("RunEmpty", "([7], X)"),
            (
                "RunInferred",
                "({ let values : Int[] = []; values }, ReadInt)",
            ),
            (
                "RunInferredReverse",
                "(ReadInt, { let values : Int[] = []; values })",
            ),
        ] {
            let argument = eval_fragments(&mut interpreter, expression);
            let actual = interpreter
                .qirgen_from_callable(&runtime_argument_global(&interpreter, name), argument)
                .unwrap_or_else(|errors| {
                    panic!("{profile:?}, {name}: {}", format_interpret_errors(errors));
                });
            let expected = interpreter
                .qirgen(&format!("{name}({expression})"))
                .expect("source call has concrete type evidence");
            assert_eq!(
                actual, expected,
                "{profile:?}, {name}: preserve inferred types"
            );
        }
    }
}

#[test]
fn runtime_functor_generic_callable_without_bound_matches_source_qir() {
    check_runtime_functor_generic_callable("", 0, true, "", false);
}

#[test]
fn runtime_functor_generic_callable_with_adjoint_bound_matches_source_qir() {
    check_runtime_functor_generic_callable("", 0, true, "is Adj", false);
}

#[test]
fn runtime_functor_generic_callable_with_controlled_bound_matches_source_qir() {
    check_runtime_functor_generic_callable("", 0, true, "is Ctl", false);
}

#[test]
fn runtime_functor_generic_callable_with_both_bounds_matches_source_qir() {
    check_runtime_functor_generic_callable("", 0, true, "is Adj + Ctl", false);
}

#[test]
fn runtime_functor_generic_adjoint_matches_source_qir() {
    check_runtime_functor_generic_callable("Adjoint", 0, false, "is Adj + Ctl", false);
}

#[test]
fn runtime_functor_generic_controlled_matches_source_qir() {
    check_runtime_functor_generic_callable("Controlled", 1, true, "is Adj + Ctl", false);
}

#[test]
fn runtime_functor_generic_nested_controlled_adjoint_matches_source_qir() {
    check_runtime_functor_generic_callable(
        "Controlled Controlled Adjoint",
        2,
        false,
        "is Adj + Ctl",
        false,
    );
}

#[test]
fn runtime_mixed_type_and_functor_generic_callable_matches_source_qir() {
    check_runtime_functor_generic_callable("Controlled Adjoint", 1, false, "is Adj + Ctl", true);
}

fn check_runtime_functor_generic_callable(
    functor: &str,
    controlled_depth: usize,
    expected_result: bool,
    bound: &str,
    generic_type: bool,
) {
    let (generics, operand_type) = if generic_type {
        ("<'T>", "'T")
    } else {
        ("", "Qubit")
    };
    let mut input = format!("((Qubit => Unit {bound}), Qubit)");
    let mut argument = "(S, q)".to_string();
    for index in (0..controlled_depth).rev() {
        input = format!("(Qubit[], {input})");
        argument = format!("([controls[{index}]], {argument})");
    }
    let source = format!(
        r#"
        operation ApplyOp{generics}(op : {operand_type} => Unit {bound}, value : {operand_type}) : Unit {bound} {{
            op(value);
        }}
        operation Run(f : {input} => Unit) : Result {{
            use controls = Qubit[{controlled_depth}];
            for control in controls {{ X(control); }}
            use q = Qubit();
            H(q);
            S(q);
            f{argument};
            H(q);
            for control in controls {{ Reset(control); }}
            MResetZ(q)
        }}
        "#
    );
    for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
        let context = format!(
            "{profile:?}, functor={functor:?}, bound={bound:?}, generic_type={generic_type}"
        );
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, &source);
        let target = eval_fragments(&mut interpreter, "Run");
        let argument = eval_fragments(
            &mut interpreter,
            &format!("{{ let f : {input} => Unit {bound} = {functor} ApplyOp; f }}"),
        );
        assert!(
            matches!(argument, Value::Global(_, _)),
            "{context}: must exercise a runtime generic global"
        );
        let mut output = Cursor::new(Vec::new());
        assert_eq!(
            interpreter
                .invoke(
                    &mut CursorReceiver::new(&mut output),
                    target.clone(),
                    argument.clone(),
                )
                .unwrap_or_else(|errors| {
                    panic!(
                        "{context}: source execution: {}",
                        format_interpret_errors(errors)
                    );
                }),
            Value::Result(qsc_eval::val::Result::Val(expected_result)),
            "{context}: preserve the phase of the applied operation"
        );
        let actual = interpreter
            .qirgen_from_callable(&target, argument)
            .unwrap_or_else(|errors| {
                panic!(
                    "{context}: runtime codegen: {}",
                    format_interpret_errors(errors)
                );
            });
        let expected = interpreter
            .qirgen(&format!("Run({functor} ApplyOp)"))
            .unwrap_or_else(|errors| {
                panic!(
                    "{context}: source codegen: {}",
                    format_interpret_errors(errors)
                );
            });
        assert_eq!(
            actual, expected,
            "{context}: preserve implicit functor arguments"
        );
    }
}

#[test]
fn runtime_callable_argument_preserves_nominal_udt_identity() {
    let source = r#"
        namespace Test {
            struct A { Data : Int }
            struct B { Data : Int }
            struct Config { Transform : A -> A }
            newtype Scalar = Int;
            newtype PairValue = (Int, Int);
            function Identity<'T>(x : 'T) : 'T { x }
            function IdA(x : A) : A { x }
            function IdB(x : B) : B { x }
            function ToB(x : Int) : B { new B { Data = x } }
            function MakeA(x : A) : (Unit -> A) { () -> x }
            operation Run(f : A -> A) : Unit { let _ = f(new A { Data = 0 }); }
            operation Output(f : Int -> A) : Unit { let _ = f(0); }
            operation Wrapped(config : Config) : Unit {
                let _ = config.Transform(new A { Data = 0 });
            }
            operation Factory(f : Unit -> A) : Unit { let _ = f(); }
            operation Alias(f : Scalar -> Scalar) : Unit { let _ = f(Scalar(1)); }
            operation Same<'T>(left : 'T -> 'T, right : 'T -> 'T) : Unit {}
            operation TupleInput(data : (Int, Int), f : Int -> Int) : Unit {
                let (x, y) = data;
                let _ = f(x + y);
            }
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        for (target, argument) in [
            ("Test.Run", "Test.IdB"),
            ("Test.Output", "Test.ToB"),
            ("Test.Wrapped", "new Test.Config { Transform = Test.IdB }"),
            ("Test.Same", "Test.IdA, Test.IdB"),
        ] {
            let source_errors = interpreter
                .qirgen(&format!("{target}({argument})"))
                .expect_err("source type checker must reject the same nominal mismatch");
            assert!(format!("{source_errors:?}").contains("TyMismatch"));
        }
        for (target, argument) in [
            ("Test.Run", "Test.IdA"),
            ("Test.Factory", "Test.MakeA(new Test.A { Data = 0 })"),
            ("Test.Alias", "Test.Identity"),
            ("Test.TupleInput", "Test.PairValue(1, 2)!, Test.Identity"),
        ] {
            let target_value = eval_fragments(&mut interpreter, target);
            let args = if target == "Test.TupleInput" {
                Value::Tuple(
                    vec![
                        eval_fragments(&mut interpreter, "Test.PairValue(1, 2)!"),
                        runtime_argument_global(&interpreter, "Identity"),
                    ]
                    .into(),
                    None,
                )
            } else if argument == "Test.Identity" {
                runtime_argument_global(&interpreter, "Identity")
            } else {
                eval_fragments(&mut interpreter, argument)
            };
            let native = interpreter
                .qirgen_from_callable(&target_value, args)
                .expect("matching UDT declarations and erased payloads must remain valid");
            let source = interpreter
                .qirgen(&format!("{target}({argument})"))
                .expect("valid source call");
            assert_eq!(native, source, "{profile:?}: {target}({argument})");
        }
    }
}

fn generic_nominal_callback_interpreter(profile: Profile) -> crate::interpret::Interpreter {
    let mut interpreter = interpreter_with_capabilities(profile.into());
    eval_fragments(
        &mut interpreter,
        r#"
        namespace Test {
            struct A { Data : Int }
            struct B { Data : Int }
            struct HolderB { Callback : B -> B }
            newtype CallbackPair = (Int, (B -> B));
            function IdA(value : A) : A { value }
            function IdB(value : B) : B { value }
            function MakeA(offset : Int) : (A -> A) {
                value -> new A { Data = value.Data + offset }
            }
            function MakeB(offset : Int) : (B -> B) {
                value -> new B { Data = value.Data + offset }
            }
            function ReadB(callback : B -> B) : Int {
                callback(new B { Data = 4 }).Data
            }
            function ReadArrayB(callbacks : (B -> B)[]) : Int {
                callbacks[0](new B { Data = 4 }).Data
            }
            function ReadHolderB(holder : HolderB) : Int {
                holder.Callback(new B { Data = 4 }).Data
            }
            function ReadPairB(pair : (Int, (B -> B))) : Int {
                let (offset, callback) = pair;
                callback(new B { Data = 4 }).Data + offset
            }
            operation Forward<'T>(value : 'T, read : 'T -> Int) : Unit {
                let _ = read(value);
            }
            operation Reverse<'T>(read : 'T -> Int, value : 'T) : Unit {
                let _ = read(value);
            }
        }
        "#,
    );
    interpreter
}

#[test]
fn runtime_callable_argument_generic_nominal_callbacks_accept_matching_values() {
    for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
        let mut interpreter = generic_nominal_callback_interpreter(profile);
        for callback in ["Test.IdB", "Test.MakeB(0)"] {
            let value = eval_fragments(&mut interpreter, callback);
            let read = runtime_argument_global(&interpreter, "ReadB");
            for (target, args) in [
                ("Forward", vec![value.clone(), read.clone()]),
                ("Reverse", vec![read.clone(), value.clone()]),
            ] {
                interpreter
                    .qirgen_from_callable(
                        &runtime_argument_global(&interpreter, target),
                        Value::Tuple(args.into(), None),
                    )
                    .unwrap_or_else(|errors| {
                        panic!("{profile:?}: {target}, {callback}: {errors:?}");
                    });
            }
        }
    }
}

#[test]
fn runtime_callable_argument_generic_nominal_callbacks_check_deferred_aggregates() {
    for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
        let mut interpreter = generic_nominal_callback_interpreter(profile);
        for (reader, expression) in [
            ("ReadArrayB", "[Test.IdB]"),
            ("ReadHolderB", "new Test.HolderB { Callback = Test.IdB }"),
            ("ReadPairB", "Test.CallbackPair(0, Test.IdB)!"),
        ] {
            let matching = eval_fragments(&mut interpreter, expression);
            let read = runtime_argument_global(&interpreter, reader);
            for (target, reverse) in [("Forward", false), ("Reverse", true)] {
                let args = if reverse {
                    vec![read.clone(), matching.clone()]
                } else {
                    vec![matching.clone(), read.clone()]
                };
                let context = format!("{profile:?}: {target}, {reader}");
                let result = interpreter.qirgen_from_callable(
                    &runtime_argument_global(&interpreter, target),
                    Value::Tuple(args.into(), None),
                );
                result.unwrap_or_else(|errors| panic!("{context}: {errors:?}"));
            }
        }
    }
}

fn assert_x_targets(qir: &str, expected: &[usize]) {
    assert_x_targets_with_context(qir, expected, "");
}

fn assert_x_targets_with_context(qir: &str, expected: &[usize], context: &str) {
    let calls: Vec<_> = qir
        .lines()
        .filter(|line| line.contains("call void @__quantum__qis__x__body("))
        .collect();
    assert_eq!(
        calls.len(),
        expected.len(),
        "{context}: unexpected X calls:\n{qir}"
    );
    for (call, &target) in calls.iter().zip(expected) {
        assert!(
            call.contains(&format!("inttoptr (i64 {target} to %Qubit*)"))
                || (target == 0 && call.contains("%Qubit* null")),
            "{context}: expected X on qubit {target}, found {call}:\n{qir}"
        );
    }
}

fn check_nested_selector_qir(selection: &str, target: usize) {
    for invocation in ["ApplyOne(f, qs)", "f(qs)"] {
        let source = format!(
            r#"
                operation A(qs : Qubit[]) : Unit is Adj + Ctl {{ X(qs[0]); }}
                operation B(qs : Qubit[]) : Unit is Adj + Ctl {{ X(qs[2]); }}
                operation ApplyOne(op : (Qubit[] => Unit is Adj + Ctl), qs : Qubit[]) : Unit is Adj + Ctl {{
                    op(qs);
                }}
                function Choose(flag : Bool, a : (Qubit[] => Unit is Adj + Ctl), b : (Qubit[] => Unit is Adj + Ctl)) : (Qubit[] => Unit is Adj + Ctl) {{
                    if flag {{ a }} else {{ b }}
                }}
                function Make() : (Qubit[] => Unit is Adj + Ctl) {{ Choose(1 < 2, A, B) }}
                operation Main() : Result[] {{
                    use qs = Qubit[3];
                    let f = {selection};
                    {invocation};
                    MResetEachZ(qs)
                }}
                "#
        );
        for profile in [Profile::Base, Profile::AdaptiveRI, Profile::AdaptiveRIF] {
            let context = format!("{profile:?}, selection={selection}, invocation={invocation}");
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(&mut interpreter, &source);
            let qir = interpreter
                .qirgen("Main()")
                .unwrap_or_else(|errors| panic!("{context}:\n{}", format_interpret_errors(errors)));
            assert_x_targets_with_context(&qir, &[target], &context);
        }
    }
}

#[test]
fn nested_selectors_single_comparison_selects_first_operation() {
    check_nested_selector_qir("Choose(1 < 2, A, B)", 0);
}

#[test]
fn nested_selectors_nested_comparisons_select_first_operation() {
    check_nested_selector_qir("Choose(1 < 2, Choose(1 < 2, A, B), B)", 0);
}

#[test]
fn nested_selectors_inner_false_selects_second_operation() {
    check_nested_selector_qir("Choose(1 < 2, Choose(1 > 2, A, B), B)", 2);
}

#[test]
fn nested_selectors_outer_false_selects_second_operation() {
    check_nested_selector_qir("Choose(1 > 2, Choose(1 < 2, A, B), B)", 2);
}

#[test]
fn nested_selectors_boolean_literals_select_first_operation() {
    check_nested_selector_qir("Choose(true, Choose(true, A, B), B)", 0);
}

#[test]
fn nested_selectors_factory_result_selects_first_operation() {
    check_nested_selector_qir("Choose(1 < 2, Make(), B)", 0);
}

const RUNTIME_CALLABLE_CAPTURE_PROFILES: [Profile; 4] = [
    Profile::Base,
    Profile::AdaptiveRI,
    Profile::AdaptiveRIF,
    Profile::Adaptive,
];

const RUNTIME_CALLABLE_CAPTURE_SOURCE: &str = indoc::indoc! {r#"
        operation A(qs : Qubit[]) : Unit is Adj + Ctl { X(qs[0]); }
        operation B(qs : Qubit[]) : Unit is Adj + Ctl { X(qs[1]); }
        operation Seq(a : (Qubit[] => Unit is Adj + Ctl), b : (Qubit[] => Unit is Adj + Ctl), qs : Qubit[]) : Unit is Adj + Ctl {
            a(qs); b(qs);
        }
        operation ApplyOnly(a : (Qubit[] => Unit is Adj + Ctl), qs : Qubit[]) : Unit is Adj + Ctl { a(qs); }
        operation ApplyRepeated(a : (Qubit[] => Unit is Adj + Ctl), n : Int, qs : Qubit[]) : Unit is Adj + Ctl {
            for _ in 1..n { a(qs); }
        }
        function MakeSeq(a : (Qubit[] => Unit is Adj + Ctl), b : (Qubit[] => Unit is Adj + Ctl)) : (Qubit[] => Unit is Adj + Ctl) { Seq(a, b, _) }
        function MakeForward(a : (Qubit[] => Unit is Adj + Ctl), b : (Qubit[] => Unit is Adj + Ctl)) : (Qubit[] => Unit is Adj + Ctl) { qs => Seq(a, b, qs) }
        function MakeDirect(a : (Qubit[] => Unit is Adj + Ctl), b : (Qubit[] => Unit is Adj + Ctl)) : (Qubit[] => Unit is Adj + Ctl) { qs => { a(qs); b(qs); } }
        function MakeOne(a : (Qubit[] => Unit is Adj + Ctl)) : (Qubit[] => Unit is Adj + Ctl) { ApplyOnly(a, _) }
        function MakeN(a : (Qubit[] => Unit is Adj + Ctl), n : Int) : (Qubit[] => Unit is Adj + Ctl) { ApplyRepeated(a, n, _) }
        function GetA() : (Qubit[] => Unit is Adj + Ctl) { A }
        function GetB() : (Qubit[] => Unit is Adj + Ctl) { B }
        operation Run(f : (Qubit[] => Unit is Adj + Ctl)) : Result[] {
            use qs = Qubit[2];
            f(qs);
            MResetEachZ(qs)
        }
        operation RunWithCount(f : (Qubit[] => Unit is Adj + Ctl), count : Int) : Result[] {
            use qs = Qubit[2];
            for _ in 1..count { f(qs); }
            MResetEachZ(qs)
        }
        operation RunMany(fs : (Qubit[] => Unit is Adj + Ctl)[]) : Result[] {
            use qs = Qubit[2];
            for f in fs { f(qs); }
            MResetEachZ(qs)
        }
        operation RunControlledAdjoint(f : (Qubit[] => Unit is Adj + Ctl)) : Result[] {
            use control = Qubit();
            use qs = Qubit[2];
            X(control);
            Controlled Adjoint f([control], qs);
            MResetEachZ([control] + qs)
        }
    "#};

fn check_runtime_callable_captures(
    scenario: &str,
    factory: &str,
    capture_count: usize,
    entry: &str,
    profiles: &[Profile],
    targets: &[usize],
    adaptive_entry: Option<&str>,
) {
    for &profile in profiles {
        let context = format!("{scenario}, {profile:?}, factory={factory}, invocation={entry}");
        let capabilities = profile.into();
        let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
        let mut interpreter = crate::interpret::Interpreter::new(
            SourceMap::default(),
            PackageType::Lib,
            capabilities,
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .unwrap_or_else(|errors| {
            panic!("{context}: {}", format_interpret_errors(errors));
        });
        let mut eval = |source: &str| {
            let mut cursor = Cursor::new(Vec::<u8>::new());
            let mut receiver = CursorReceiver::new(&mut cursor);
            interpreter
                .eval_fragments(&mut receiver, source)
                .unwrap_or_else(|errors| {
                    panic!("{context}: {}", format_interpret_errors(errors));
                })
        };
        eval(RUNTIME_CALLABLE_CAPTURE_SOURCE);
        let callable = eval(entry);
        let closure = eval(factory);
        assert!(
            matches!(&closure, Value::Closure(value) if value.fixed_args.len() == capture_count),
            "{context}: expected {capture_count} captures: {closure:?}"
        );
        let args = match entry {
            "Run" | "RunControlledAdjoint" => closure,
            "RunWithCount" => Value::Tuple(vec![closure, Value::Int(1)].into(), None),
            "RunMany" => Value::Array(vec![closure.clone(), closure].into()),
            _ => panic!("{context}: unsupported capture test entry"),
        };
        let qir = interpreter
            .qirgen_from_callable(&callable, args)
            .unwrap_or_else(|errors| {
                panic!("{context}: {}", format_interpret_errors(errors));
            });
        if profile == Profile::Adaptive {
            let entry_body = qir_entry_body(&qir);
            let x_name = if factory.starts_with("Adjoint ") {
                "X__Adj"
            } else {
                "X"
            };
            if let Some(expected) = adaptive_entry {
                assert_eq!(entry_body, expected, "{context}: unexpected Adaptive entry");
            } else {
                let calls = entry_body
                    .lines()
                    .filter(|line| line.contains("call void @X"))
                    .map(str::trim)
                    .collect::<Vec<_>>();
                let expected = targets
                    .iter()
                    .map(|target| {
                        format!("call void @{x_name}(ptr inttoptr (i64 {target} to ptr))")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(calls, expected, "{context}: unexpected Adaptive X targets");
            }
            let (_, wrapper) = qir
                .split_once(&format!("define internal void @{x_name}(ptr "))
                .unwrap_or_else(|| panic!("{context}: X wrapper should exist:\n{qir}"));
            let (argument, body) = wrapper
                .split_once(") {\n")
                .unwrap_or_else(|| panic!("{context}: X wrapper signature should terminate"));
            let (body, _) = body
                .split_once("\n}")
                .unwrap_or_else(|| panic!("{context}: X wrapper body should terminate"));
            let (label, instructions) = body
                .split_once('\n')
                .unwrap_or_else(|| panic!("{context}: X wrapper should have instructions"));
            assert!(
                label.starts_with("block_") && label.ends_with(':'),
                "{context}: expected X wrapper block label, found {label}"
            );
            assert_eq!(
                instructions,
                format!("  call void @__quantum__qis__x__body(ptr {argument})\n  ret void"),
                "{context}: X wrapper must apply exactly one X to its argument"
            );
        } else {
            assert_x_targets_with_context(&qir, targets, &context);
        }
        if entry == "RunControlledAdjoint" {
            let controlled_calls = qir
                .lines()
                .filter(|line| line.contains("call void @__quantum__qis__cx__body("))
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n");
            let qubit_type = if profile == Profile::Adaptive {
                "ptr"
            } else {
                "%Qubit*"
            };
            let expected = [2, 1]
                .map(|target| {
                    format!(
                        "call void @__quantum__qis__cx__body({qubit_type} inttoptr (i64 0 to {qubit_type}), {qubit_type} inttoptr (i64 {target} to {qubit_type}))"
                    )
                })
                .join("\n");
            assert_eq!(
                controlled_calls, expected,
                "{context}: controlled adjoint must reverse the captured operations"
            );
        }
    }
}

#[test]
fn runtime_callable_captures_partial_application_of_returned_operations() {
    check_runtime_callable_captures(
        "partial application of returned operations",
        "MakeSeq(GetA(), GetB())",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1],
        None,
    );
}

#[test]
fn runtime_callable_captures_partial_application_of_declared_operations() {
    check_runtime_callable_captures(
        "partial application of declared operations",
        "MakeSeq(A, B)",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1],
        None,
    );
}

#[test]
fn runtime_callable_captures_adjoint_reverses_captured_operations() {
    check_runtime_callable_captures(
        "adjoint of partial application",
        "Adjoint (MakeSeq(GetA(), GetB()))",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[1, 0],
        None,
    );
}

fn check_runtime_captured_adjoint_phases(controlled: bool) {
    let (input, invocation, expected_invocation, functor, controls) = if controlled {
        (
            "(Qubit[], Qubit[])",
            "f([control], [q]); Reset(control);",
            "Controlled Adjoint H([control], q); Controlled Adjoint S([control], q); Reset(control);",
            "Controlled Adjoint",
            "use control = Qubit(); X(control);",
        )
    } else {
        (
            "Qubit[]",
            "f([q]);",
            "Adjoint H(q); Adjoint S(q);",
            "Adjoint",
            "",
        )
    };
    // The captured S-H adjoint must undo S-H preparation, including S's phase.
    // Reversing the operations without inverting S would return One, not Zero.
    let body = |invocation: &str| {
        format!(
            "{{ use q = Qubit(); {controls}
                H(q); S(q); H(q);
                {invocation}
                H(q); MResetZ(q)
            }}"
        )
    };
    for factory in ["Seq(a, b, _)", "qs => { a(qs); b(qs); }"] {
        let source = format!(
            r#"
            operation Phase(qs : Qubit[]) : Unit is Adj + Ctl {{ S(qs[0]); }}
            operation Basis(qs : Qubit[]) : Unit is Adj + Ctl {{ H(qs[0]); }}
            operation Seq(a : Qubit[] => Unit is Adj + Ctl, b : Qubit[] => Unit is Adj + Ctl, qs : Qubit[]) : Unit is Adj + Ctl {{
                a(qs); b(qs);
            }}
            function Make(a : Qubit[] => Unit is Adj + Ctl, b : Qubit[] => Unit is Adj + Ctl) : Qubit[] => Unit is Adj + Ctl {{
                {factory}
            }}
            operation Run(f : {input} => Unit is Adj + Ctl) : Result {}
            operation Expected() : Result {}
            "#,
            body(invocation),
            body(expected_invocation),
        );
        for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
            let context = format!("{profile:?}, factory={factory}, functor={functor}");
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(&mut interpreter, &source);
            let target = eval_fragments(&mut interpreter, "Run");
            let captured =
                eval_fragments(&mut interpreter, &format!("{functor} (Make(Phase, Basis))"));
            assert!(
                matches!(&captured, Value::Closure(closure) if closure.fixed_args.len() == 2),
                "{context}: must exercise two runtime callable captures"
            );
            let mut cursor = Cursor::new(Vec::new());
            assert_eq!(
                interpreter
                    .invoke(
                        &mut CursorReceiver::new(&mut cursor),
                        target.clone(),
                        captured.clone(),
                    )
                    .unwrap_or_else(|errors| {
                        panic!("{context}: {}", format_interpret_errors(errors));
                    }),
                Value::Result(qsc_eval::val::Result::Val(false)),
                "{context}: the captured adjoint must undo the preparation"
            );
            let qir = interpreter
                .qirgen_from_callable(&target, captured)
                .unwrap_or_else(|errors| {
                    panic!("{context}: {}", format_interpret_errors(errors));
                });
            let expected_qir = interpreter.qirgen("Expected()").unwrap_or_else(|errors| {
                panic!(
                    "{context}: reference QIR: {}",
                    format_interpret_errors(errors)
                );
            });
            assert_eq!(
                qir, expected_qir,
                "{context}: captured adjoints must invert each operation and reverse their order"
            );
        }
    }
}

#[test]
fn runtime_callable_captures_adjoint_inverts_noncommuting_operations() {
    check_runtime_captured_adjoint_phases(false);
}

#[test]
fn runtime_callable_captures_controlled_adjoint_inverts_noncommuting_operations() {
    check_runtime_captured_adjoint_phases(true);
}

#[test]
fn runtime_callable_captures_forwarding_lambda() {
    check_runtime_callable_captures(
        "forwarding lambda",
        "MakeForward(GetA(), GetB())",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1],
        None,
    );
}

#[test]
fn runtime_callable_captures_direct_lambda() {
    check_runtime_callable_captures(
        "direct lambda",
        "MakeDirect(GetA(), GetB())",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1],
        None,
    );
}

#[test]
fn runtime_callable_captures_single_operation() {
    check_runtime_callable_captures(
        "single captured operation",
        "MakeOne(GetA())",
        1,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0],
        None,
    );
}

#[test]
fn runtime_callable_captures_operation_and_repeat_count() {
    check_runtime_callable_captures(
        "captured operation and repeat count",
        "MakeN(GetA(), 3)",
        2,
        "Run",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 0, 0],
        Some(indoc::indoc! {r#"
            block_0:
              %var_1 = alloca i64
              %var_2 = alloca i64
              %var_5 = alloca i1
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 1, ptr %var_1
              br label %block_1
            block_1:
              %var_10 = load i64, ptr %var_1
              store i64 %var_10, ptr %var_2
              %var_12 = load i64, ptr %var_2
              %var_3 = icmp sle i64 %var_12, 3
              store i1 true, ptr %var_5
              br i1 %var_3, label %block_2, label %block_3
            block_2:
              %var_15 = load i1, ptr %var_5
              br i1 %var_15, label %block_4, label %block_5
            block_3:
              store i1 false, ptr %var_5
              br label %block_2
            block_4:
              call void @X(ptr inttoptr (i64 0 to ptr))
              %var_16 = load i64, ptr %var_1
              store i64 %var_16, ptr %var_7
              %var_18 = load i64, ptr %var_7
              %var_8 = add i64 %var_18, 1
              store i64 %var_8, ptr %var_1
              br label %block_1
            block_5:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__array_record_output(i64 2, ptr @0)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
              ret i64 0"#}),
    );
}

#[test]
fn runtime_callable_captures_controlled_adjoint_reverses_captured_operations() {
    check_runtime_callable_captures(
        "controlled adjoint of partial application",
        "MakeSeq(GetA(), GetB())",
        2,
        "RunControlledAdjoint",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0],
        None,
    );
}

#[test]
fn runtime_callable_captures_tuple_entry_with_count() {
    check_runtime_callable_captures(
        "callable and count tuple entry",
        "MakeSeq(GetA(), GetB())",
        2,
        "RunWithCount",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1],
        Some(indoc::indoc! {r#"
            block_0:
              %var_1 = alloca i64
              %var_2 = alloca i64
              %var_5 = alloca i1
              %var_7 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 1, ptr %var_1
              br label %block_1
            block_1:
              %var_10 = load i64, ptr %var_1
              store i64 %var_10, ptr %var_2
              %var_12 = load i64, ptr %var_2
              %var_3 = icmp sle i64 %var_12, 1
              store i1 true, ptr %var_5
              br i1 %var_3, label %block_2, label %block_3
            block_2:
              %var_15 = load i1, ptr %var_5
              br i1 %var_15, label %block_4, label %block_5
            block_3:
              store i1 false, ptr %var_5
              br label %block_2
            block_4:
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @X(ptr inttoptr (i64 1 to ptr))
              %var_16 = load i64, ptr %var_1
              store i64 %var_16, ptr %var_7
              %var_18 = load i64, ptr %var_7
              %var_8 = add i64 %var_18, 1
              store i64 %var_8, ptr %var_1
              br label %block_1
            block_5:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__array_record_output(i64 2, ptr @0)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
              ret i64 0"#}),
    );
}

#[test]
fn runtime_callable_captures_array_entry() {
    check_runtime_callable_captures(
        "callable array entry",
        "MakeSeq(GetA(), GetB())",
        2,
        "RunMany",
        &RUNTIME_CALLABLE_CAPTURE_PROFILES,
        &[0, 1, 0, 1],
        None,
    );
}

fn check_runtime_mixed_callable_captures(scenario: &str, make_flip: &str) {
    for functors in ["", "is Adj", "is Ctl", "is Adj + Ctl"] {
        let source = format!(
            r#"
                operation A(qs : Qubit[]) : Unit {functors} {{ X(qs[0]); }}
                operation Flip(i : Int, qs : Qubit[]) : Unit {functors} {{ X(qs[i]); }}
                operation Both(a : (Qubit[] => Unit {functors}), b : (Qubit[] => Unit {functors}), qs : Qubit[]) : Unit {functors} {{
                    a(qs); b(qs);
                }}
                function MakeBoth(a : (Qubit[] => Unit {functors}), b : (Qubit[] => Unit {functors})) : (Qubit[] => Unit {functors}) {{
                    Both(a, b, _)
                }}
                function MakeFlip(i : Int) : (Qubit[] => Unit {functors}) {{ {make_flip} }}
                operation Run(f : (Qubit[] => Unit {functors})) : Result[] {{
                    use qs = Qubit[2];
                    f(qs);
                    MResetEachZ(qs)
                }}
                "#
        );
        for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
            let context =
                format!("{scenario}, {profile:?}, functors={functors:?}, factory={make_flip}");
            let capabilities = profile.into();
            let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
            let mut interpreter = crate::interpret::Interpreter::new(
                SourceMap::default(),
                PackageType::Lib,
                capabilities,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
                Default::default(),
            )
            .unwrap_or_else(|errors| {
                panic!("{context}: {}", format_interpret_errors(errors));
            });
            let mut output = Cursor::new(Vec::new());
            let mut receiver = CursorReceiver::new(&mut output);
            interpreter
                .eval_fragments(&mut receiver, &source)
                .unwrap_or_else(|errors| {
                    panic!("{context}: {}", format_interpret_errors(errors));
                });
            let globals = interpreter.user_globals();
            let global = |name: &str| {
                globals
                    .iter()
                    .find(|g| g.name.as_ref() == name)
                    .unwrap_or_else(|| panic!("{context}: {name} should exist"))
                    .value
                    .clone()
            };
            let flip = interpreter
                .invoke(&mut receiver, global("MakeFlip"), Value::Int(1))
                .unwrap_or_else(|errors| {
                    panic!("{context}: MakeFlip: {}", format_interpret_errors(errors));
                });
            let composed = interpreter
                .invoke(
                    &mut receiver,
                    global("MakeBoth"),
                    Value::Tuple(vec![global("A"), flip].into(), None),
                )
                .unwrap_or_else(|errors| {
                    panic!("{context}: MakeBoth: {}", format_interpret_errors(errors));
                });
            let expected =
                Value::Array(vec![Value::Result(qsc_eval::val::Result::Val(true)); 2].into());
            assert_eq!(
                interpreter
                    .invoke(&mut receiver, global("Run"), composed.clone())
                    .unwrap_or_else(|errors| {
                        panic!("{context}: Run: {}", format_interpret_errors(errors));
                    }),
                expected,
                "{context}: simulator must flip both qubits"
            );
            // Preserve runtime construction: compiling the inline expression
            // would bypass the closure-value reconstruction that failed.
            let qir = interpreter
                .qirgen_from_callable(&global("Run"), composed)
                .unwrap_or_else(|errors| {
                    panic!(
                        "{context}: QIR generation: {}",
                        format_interpret_errors(errors)
                    );
                });
            if profile == Profile::Adaptive {
                let calls = qir
                    .lines()
                    .filter(|line| line.contains("call void @X("))
                    .map(str::trim)
                    .collect::<Vec<_>>();
                assert_eq!(
                    calls,
                    [
                        "call void @X(ptr inttoptr (i64 0 to ptr))",
                        "call void @X(ptr inttoptr (i64 1 to ptr))",
                    ],
                    "{context}: unexpected Adaptive X targets"
                );
                assert!(
                    qir.contains("call void @__quantum__qis__x__body("),
                    "{context}: X wrapper must call the X intrinsic"
                );
            } else {
                assert_x_targets_with_context(&qir, &[0, 1], &context);
            }
        }
    }
}

#[test]
fn runtime_mixed_callable_captures_partial_application_preserves_operations() {
    check_runtime_mixed_callable_captures("partial application", "Flip(i, _)");
}

#[test]
fn runtime_mixed_callable_captures_lambda_preserves_operations() {
    check_runtime_mixed_callable_captures("lambda", "qs => Flip(i, qs)");
}

fn check_runtime_callable_arrays(case: &str) {
    for functors in ["is Ctl", "is Adj + Ctl"] {
        for factory in ["ApplyAll(ops, _)", "q => ApplyAll(ops, q)"] {
            let source = format!(
                r#"
                operation A(q : Qubit[]) : Unit {functors} {{ X(q[0]); }}
                operation B(q : Qubit[]) : Unit is Adj + Ctl {{ X(q[1]); }}
                operation Flip(i : Int, q : Qubit[]) : Unit {functors} {{ X(q[i]); }}
                operation ApplyAll(ops : (Qubit[] => Unit {functors})[], q : Qubit[]) : Unit {functors} {{
                    for op in ops {{ op(q); }}
                }}
                function MakeAll(ops : (Qubit[] => Unit {functors})[]) : (Qubit[] => Unit {functors}) {{
                    {factory}
                }}
                function MakeFlip(i : Int) : (Qubit[] => Unit {functors}) {{ Flip(i, _) }}
                operation Run(f : Qubit[] => Unit {functors}) : Result[] {{
                    use (control, q) = (Qubit(), Qubit[2]);
                    X(control);
                    Controlled f([control], q);
                    Reset(control);
                    MResetEachZ(q)
                }}
                "#
            );
            for profile in RUNTIME_CALLABLE_CAPTURE_PROFILES {
                let context =
                    format!("shape={case}, {profile:?}, functors={functors:?}, factory={factory}");
                let capabilities = profile.into();
                let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
                let mut interpreter = crate::interpret::Interpreter::new(
                    SourceMap::default(),
                    PackageType::Lib,
                    capabilities,
                    LanguageFeatures::default(),
                    store,
                    &[(std_id, None)],
                    Default::default(),
                )
                .unwrap_or_else(|errors| {
                    panic!("{context}: {}", format_interpret_errors(errors));
                });
                let mut output = Cursor::new(Vec::new());
                interpreter
                    .eval_fragments(&mut CursorReceiver::new(&mut output), &source)
                    .unwrap_or_else(|errors| {
                        panic!("{context}: {}", format_interpret_errors(errors));
                    });
                let globals = interpreter.user_globals();
                let global = |name: &str| {
                    globals
                        .iter()
                        .find(|g| g.name.as_ref() == name)
                        .unwrap_or_else(|| panic!("{context}: {name} should exist"))
                        .value
                        .clone()
                };
                let invoke = |interpreter: &mut crate::interpret::Interpreter, name, args| {
                    let mut output = Cursor::new(Vec::new());
                    interpreter
                        .invoke(&mut CursorReceiver::new(&mut output), global(name), args)
                        .unwrap_or_else(|errors| {
                            panic!("{context}: {name}: {}", format_interpret_errors(errors));
                        })
                };
                let values = match case {
                    "globals" => vec![global("A"), global("B")],
                    "partial" => vec![
                        invoke(&mut interpreter, "MakeFlip", Value::Int(0)),
                        global("B"),
                    ],
                    "empty" => Vec::new(),
                    "nested" => ["A", "B"]
                        .map(|name| {
                            invoke(
                                &mut interpreter,
                                "MakeAll",
                                Value::Array(vec![global(name)].into()),
                            )
                        })
                        .to_vec(),
                    _ => panic!("{context}: unsupported capture-array shape"),
                };
                let composed = invoke(&mut interpreter, "MakeAll", Value::Array(values.into()));
                assert_eq!(
                    invoke(&mut interpreter, "Run", composed.clone()),
                    Value::Array(
                        vec![Value::Result(qsc_eval::val::Result::Val(case != "empty")); 2].into()
                    ),
                    "{context}: unexpected simulator results"
                );
                let qir = interpreter
                    .qirgen_from_callable(&global("Run"), composed.clone())
                    .unwrap_or_else(|errors| {
                        panic!(
                            "{context}: QIR generation: {}",
                            format_interpret_errors(errors)
                        );
                    });
                let calls = qir
                    .lines()
                    .map(str::trim)
                    .filter(|line| line.starts_with("call void @__quantum__qis__cx__body("))
                    .map(|line| line.replace("%Qubit*", "ptr"))
                    .collect::<Vec<_>>();
                if case == "empty" {
                    assert!(
                        calls.is_empty(),
                        "{context}: empty captures must apply no controlled gates"
                    );
                } else {
                    assert_eq!(
                        calls,
                        [
                            "call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))",
                            "call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))",
                        ],
                        "{context}: unexpected controlled X operands"
                    );
                }
                if case == "partial" {
                    let repeated = interpreter
                        .qirgen_from_callable(&global("Run"), composed)
                        .unwrap_or_else(|errors| {
                            panic!(
                                "{context}: repeated QIR generation: {}",
                                format_interpret_errors(errors)
                            );
                        });
                    assert_eq!(
                        qir, repeated,
                        "{context}: compilation must preserve reusable runtime values"
                    );
                }
            }
        }
    }
}

#[test]
fn runtime_callable_arrays_global_captures_preserve_declared_functors() {
    check_runtime_callable_arrays("globals");
}

#[test]
fn runtime_callable_arrays_partial_captures_preserve_declared_functors() {
    check_runtime_callable_arrays("partial");
}

#[test]
fn runtime_callable_arrays_empty_captures_preserve_declared_functors() {
    check_runtime_callable_arrays("empty");
}

#[test]
fn runtime_callable_arrays_nested_captures_preserve_declared_functors() {
    check_runtime_callable_arrays("nested");
}

#[test]
fn runtime_normalization_preserves_bounds_and_templates() {
    use qsc_fir::{
        fir::{CallableDecl, CallableImpl, Package, Pat, PatId, PatKind},
        ty::{FunctorSet, FunctorSetValue, Ty},
    };

    fn controls(decl: &CallableDecl) -> Vec<PatId> {
        let CallableImpl::Spec(specs) = &decl.implementation else {
            return Vec::new();
        };
        std::iter::once(&specs.body)
            .chain(specs.adj.iter())
            .chain(specs.ctl.iter())
            .chain(specs.ctl_adj.iter())
            .filter_map(|spec| spec.input)
            .collect()
    }

    fn patterns(package: &Package, mut roots: Vec<PatId>) -> Vec<Pat> {
        let mut result = Vec::new();
        while let Some(id) = roots.pop() {
            let pat = package.get_pat(id);
            if let PatKind::Tuple(children) = &pat.kind {
                roots.extend(children);
            }
            result.push(pat.clone());
        }
        result
    }

    let source = r#"
        operation ApplyAll(ops : (Qubit => Unit is Adj + Ctl)[], q : Qubit) : Unit is Adj + Ctl {
            for op in ops { op(q); }
        }
        function MakeAll(ops : (Qubit => Unit is Adj + Ctl)[]) : (Qubit => Unit is Adj + Ctl) {
            ApplyAll(ops, _)
        }
    "#;
    let (store, package_id, items) = compile_and_locate_items(
        source,
        &[(".lambda", true), ("ApplyAll", true)],
        Profile::AdaptiveRIF.into(),
    );
    let lambda_id = fir_id_for(package_id, items[".lambda"]);
    let generic_id = fir_id_for(package_id, items["ApplyAll"]);
    let hir_package = &store
        .get(package_id)
        .expect("source package exists")
        .package;
    // A shallow HIR clone shares mutable arrow cells, so snapshot their values.
    let original_hir = format!("{hir_package:?}");
    let (fir_store, package_id, _) = lower_hir_to_fir(&store, package_id);
    let package = fir_store.get(package_id);
    let FirItemKind::Callable(lambda) = &package.get_item(lambda_id.item).kind else {
        panic!("lifted lambda should exist");
    };
    assert!(lambda.generics.is_empty());
    let lambda_inputs = patterns(package, vec![lambda.input]);
    let control_inputs = patterns(package, controls(lambda));
    assert!(
        !control_inputs.is_empty(),
        "fixture must include control bindings"
    );
    let is_parameterized_capture = |pat: &Pat| {
        matches!(&pat.ty, Ty::Array(ty)
            if matches!(ty.as_ref(), Ty::Arrow(arrow)
                if matches!(arrow.functors, FunctorSet::Param(_))))
    };
    assert!(
        lambda_inputs.iter().any(is_parameterized_capture),
        "fixture must expose an inherited functor parameter",
    );
    let FirItemKind::Callable(generic) = &package.get_item(generic_id.item).kind else {
        panic!("generic callable should exist");
    };
    assert!(!generic.generics.is_empty());
    let mut generic_roots = controls(generic);
    generic_roots.push(generic.input);
    let generic_inputs = patterns(package, generic_roots);
    let callables = FxHashSet::from_iter([lambda_id, generic_id]);

    let normalized =
        super::qir::normalize_runtime_callable_hir(hir_package, package_id, &callables);
    assert_eq!(format!("{hir_package:?}"), original_hir);
    assert_eq!(
        normalized.items.get(items["ApplyAll"]),
        hir_package.items.get(items["ApplyAll"]),
        "preserve the entire generic declaration"
    );
    let repeated = super::qir::normalize_runtime_callable_hir(&normalized, package_id, &callables);
    assert_eq!(
        format!("{normalized:?}"),
        format!("{repeated:?}"),
        "normalization is idempotent"
    );
    let foreign_callables = callables
        .iter()
        .map(|id| StoreItemId {
            package: qsc_fir::fir::PackageId::from(0_usize),
            ..*id
        })
        .collect();
    assert!(matches!(
        super::qir::normalize_runtime_callable_hir(hir_package, package_id, &foreign_callables),
        std::borrow::Cow::Borrowed(_)
    ));

    let normalized_fir =
        qsc_lowerer::Lowerer::new().lower_package(&normalized, &fir_store, package_id);
    let package = &normalized_fir;
    for before in &lambda_inputs {
        let after = package.get_pat(before.id);
        assert_eq!(
            after.kind, before.kind,
            "preserve input bindings and structure"
        );
        assert_eq!(after.span, before.span);
        assert!(
            !is_parameterized_capture(after),
            "input must be concrete: {after:?}"
        );
        if let Ty::Array(item) = &after.ty
            && let Ty::Arrow(arrow) = item.as_ref()
        {
            assert_eq!(
                arrow.functors,
                FunctorSet::Value(FunctorSetValue::CtlAdj),
                "preserve the capture's declared functor bound"
            );
        }
        if let PatKind::Tuple(children) = &after.kind {
            let Ty::Tuple(types) = &after.ty else {
                panic!("tuple pattern must retain its tuple type");
            };
            assert_eq!(children.len(), types.len());
            for (&child, ty) in children.iter().zip(types) {
                assert_eq!(&package.get_pat(child).ty, ty);
            }
        }
    }
    for before in generic_inputs.iter().chain(&control_inputs) {
        assert_eq!(
            package.get_pat(before.id),
            before,
            "preserve generic inputs and controls"
        );
    }
}

#[test]
fn nested_callable_arrays_control_each_target() {
    let source = r#"
        operation A(q : Qubit[]) : Unit is Ctl { X(q[0]); }
        operation B(q : Qubit[]) : Unit is Ctl { X(q[1]); }
        operation ApplyAll(ops : (Qubit[] => Unit is Ctl)[], q : Qubit[]) : Unit is Ctl {
            for op in ops { op(q); }
        }
        function MakeAll(ops : (Qubit[] => Unit is Ctl)[]) : (Qubit[] => Unit is Ctl) {
            ApplyAll(ops, _)
        }
        operation Run(f : Qubit[] => Unit is Ctl) : Result[] {
            use (control, q) = (Qubit(), Qubit[2]);
            X(control);
            Controlled f([control], q);
            Reset(control);
            MResetEachZ(q)
        }
        @EntryPoint() operation Main() : Result[] {
            Run(MakeAll([MakeAll([A]), MakeAll([B])]))
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let qir = compile_source_to_qir(source, profile.into());
        let gates = qir
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("call void @__quantum__qis__cx__body("))
            .map(|line| line.replace("%Qubit*", "ptr"))
            .collect::<Vec<_>>()
            .join("\n");
        expect![[r#"
            call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))"#]]
        .assert_eq(&gates);
    }
}

// ---- Synthetic path: arrow + non-callable params (tuple input) ----

#[test]
fn synthetic_path_arrow_and_int_tuple_generates_qir() {
    // Target takes (op: Qubit => Unit, count: Int). `args` supplies both the
    // Global callable and a plain Int count in their respective tuple slots.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunOp(op : Qubit => Unit, count : Int) : Result {
                use q = Qubit();
                for _ in 0..count - 1 {
                    op(q);
                }
                MResetZ(q)
            }
            operation MyH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunOp", true), ("MyH", true)], caps);

    let my_h = Value::Global(fir_id_for(pkg, items["MyH"]), FunctorApp::default());
    let args = Value::Tuple(vec![my_h, Value::Int(3)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &args, caps);
    // The QIR must contain an h__body call from the loop body.
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected h gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__mresetz__body"),
        "expected mresetz in QIR:\n{qir}"
    );
}

// ---- Synthetic path: two callable args in a tuple ----

#[test]
fn synthetic_path_two_arrow_args_generates_qir() {
    // Target takes (op1: Qubit => Unit, op2: Qubit => Unit). Both are
    // Global values — the synthetic Call must place both at their respective
    // tuple positions.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyBoth(op1 : Qubit => Unit, op2 : Qubit => Unit) : Result {
                use q = Qubit();
                op1(q);
                op2(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("ApplyBoth", true), ("DoH", true), ("DoX", true)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![do_h, do_x].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["ApplyBoth"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_generic_target_infers_type_from_callable_arg() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyGeneric<'T>(op : 'T => Unit, value : 'T) : Result {
                op(value);
                use q = Qubit();
                return MResetZ(q);
            }
            operation DoHIfOne(n : Int) : Unit {
                if n == 1 {
                    use q = Qubit();
                    H(q);
                    Reset(q);
                }
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("ApplyGeneric", true), ("DoHIfOne", true)], caps);

    let do_h_if_one = Value::Global(fir_id_for(pkg, items["DoHIfOne"]), FunctorApp::default());
    let args = Value::Tuple(vec![do_h_if_one, Value::Int(1)].into(), None);

    let target_hir = hir_id_for(pkg, items["ApplyGeneric"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &args, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "generic target should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "lowerable generic target args should use the synthetic entry"
    );

    let qir = callable_args_to_qir(&store, pkg, items["ApplyGeneric"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected generic target to infer T = Int and emit H:\n{qir}"
    );
}

#[test]
fn callable_args_with_top_level_qubit_value_use_reinvoke_original() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyGeneric<'T>(op : 'T => Unit, value : 'T) : Unit {
                op(value);
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("ApplyGeneric", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let qubit = Rc::new(qsc_eval::val::Qubit(0));
    let args = Value::Tuple(vec![do_h, Value::Qubit((&qubit).into())].into(), None);

    let target_hir = hir_id_for(pkg, items["ApplyGeneric"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &args, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "top-level qubit callable args should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "top-level runtime qubit values must not be lowered into the synthetic entry"
    );
}

// ---- Synthetic path: arrow sandwiched between non-callable params ----

#[test]
fn synthetic_path_int_arrow_bool_tuple_generates_qir() {
    // Target takes (n: Int, op: Qubit => Unit, flag: Bool). The callable is
    // in the middle of the tuple — exercises the element-wise matching logic
    // in `build_synthetic_args`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Middle(n : Int, op : Qubit => Unit, flag : Bool) : Result {
                use q = Qubit();
                if flag {
                    for _ in 0..n - 1 { op(q); }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Middle", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![Value::Int(2), do_x, Value::Bool(true)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["Middle"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: no callable args (pure values) ----

#[test]
fn no_callable_args_takes_early_return_path() {
    // When args contain no callable values, `prepare_codegen_fir_from_callable_args`
    // takes the `concrete_callables.is_empty()` early return to `prepare_codegen_fir_from_callable`.
    // This exercises that branch.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Simple(n : Int) : Result {
                use q = Qubit();
                for _ in 0..n - 1 { H(q); }
                MResetZ(q)
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(source, &[("Simple", true)], caps);

    let args = Value::Int(3);
    let target_hir = hir_id_for(pkg, items["Simple"]);

    // Should succeed without error — takes the no-callable early path.
    let result = prepare_codegen_fir_from_callable_args(&store, target_hir, &args, caps);
    assert!(
        result.is_ok(),
        "no-callable args should succeed: {:?}",
        result.err().map(format_interpret_errors)
    );
}

// ---- Synthetic path: struct with callable and non-callable fields ----

#[test]
fn synthetic_path_struct_with_callable_field_generates_qir() {
    // `Config` is a newtype wrapping (Op: Qubit => Unit, Data: Int).
    // The synthetic Call builder resolves the UDT's pure tuple shape so defunc
    // can discover and specialize the callable field.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("Apply", true), ("DoH", true), ("Config", false)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let config = Value::Tuple(
        vec![do_h, Value::Int(42)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Config"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["Apply"], &config, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- No-callable path: struct with only non-callable fields ----

#[test]
fn struct_with_no_callable_fields_takes_early_return_path() {
    // A UDT that contains no callable fields takes the `concrete_callables.is_empty()`
    // early return.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Pair = (First: Int, Second: Int);
            operation Sum(p: Pair) : Result {
                use q = Qubit();
                let total = p::First + p::Second;
                if total > 0 { H(q); }
                MResetZ(q)
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Sum", true), ("Pair", false)], caps);

    let pair = Value::Tuple(
        vec![Value::Int(3), Value::Int(5)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Pair"]))),
    );
    let target_hir = hir_id_for(pkg, items["Sum"]);

    let result = prepare_codegen_fir_from_callable_args(&store, target_hir, &pair, caps);
    assert!(
        result.is_ok(),
        "struct with no callable fields should succeed: {:?}",
        result.err().map(format_interpret_errors)
    );
}

// ---- Synthetic path: single Global arg (not in a tuple) ----

#[test]
fn synthetic_path_single_global_arg_generates_qir() {
    // The simplest synthetic path: a single callable arg, not wrapped in a tuple.
    // `build_synthetic_args` hits the `Ty::Arrow` branch directly.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Invoke(op : Qubit => Unit) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Invoke", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());

    let qir = callable_args_to_qir(&store, pkg, items["Invoke"], &do_x, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_captureless_closure_adjoint_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Invoke(op : Qubit => Unit is Adj) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoS(q : Qubit) : Unit is Adj { S(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Invoke", true), ("DoS", true)], caps);

    let adjoint_do_s = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: Vec::<Value>::new().into(),
        id: fir_id_for(pkg, items["DoS"]),
        functor: FunctorApp {
            adjoint: true,
            controlled: 0,
        },
    }));

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let (codegen_fir, _backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &adjoint_do_s, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "adjoint captureless closure should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__s__adj"),
        "expected adjoint S gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_captureless_lambda_array_preserves_input() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation CallAll(ops : (Qubit => Unit)[]) : Result {
                use q = Qubit();
                for op in ops {
                    op(q);
                }
                MResetZ(q)
            }
        }
    "#};

    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let callable = eval_fragments(&mut interpreter, "Test.CallAll");
        let args = eval_fragments(&mut interpreter, "[q => X(q)]");
        let qir = interpreter
            .qirgen_from_callable(&callable, args)
            .unwrap_or_else(|errors| {
                panic!(
                    "lambda array failed on {profile:?}: {}",
                    format_interpret_errors(errors)
                )
            });
        assert_eq!(
            qir.matches("call void @__quantum__qis__x__body(").count(),
            1,
            "{profile:?} should preserve the captureless lambda call:\n{qir}"
        );
    }
}

#[test]
fn foreign_closure_arguments_preserve_package_identity() {
    let user_source = r#"
        namespace Test {
            function DecoyA(value : Int) : Int { value - 100 }
            function DecoyB(value : Int) : Int { value - 200 }
            function DecoyC(value : Int) : Int { value - 300 }
            function Run(actions : (Int -> Int)[]) : Int {
                mutable answer = 0;
                for action in actions { set answer += action(3); }
                answer
            }
            function Wrap(action : Int -> Int) : Int -> Int { value -> action(value) + 7 }
        }
        "#;
    for capturing in [false, true] {
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let capabilities = profile.into();
            let (mut store, user_package, items) = compile_and_locate_items(
                user_source,
                &[("Run", true), ("DecoyA", true), (".lambda", true)],
                capabilities,
            );
            let library_source = if capturing {
                "namespace Library { function Make(offset : Int) : Int -> Int { value -> value + offset } export Make; }"
            } else {
                "namespace Library { function Make() : Int -> Int { value -> value + 100 } export Make; }"
            };
            let (unit, errors) = crate::compile::compile(
                &store,
                &[],
                source_map_from_source(library_source),
                PackageType::Lib,
                capabilities,
                LanguageFeatures::default(),
            );
            assert!(errors.is_empty(), "library must compile: {errors:?}");
            let lambda = unit
                .package
                .items
                .iter()
                .find_map(|(id, item)| match &item.kind {
                    ItemKind::Callable(decl)
                        if callable_name_matches(decl.name.name.as_ref(), ".lambda") =>
                    {
                        Some(id)
                    }
                    _ => None,
                })
                .expect("library must contain a lifted lambda");
            let library_package = store.insert(unit);
            let foreign = Value::Closure(Box::new(qsc_eval::val::Closure {
                id: fir_id_for(library_package, lambda),
                fixed_args: if capturing {
                    vec![Value::Int(100)]
                } else {
                    vec![]
                }
                .into(),
                functor: FunctorApp::default(),
            }));
            let wrapped = Value::Closure(Box::new(qsc_eval::val::Closure {
                id: fir_id_for(user_package, items[".lambda"]),
                fixed_args: vec![foreign.clone()].into(),
                functor: FunctorApp::default(),
            }));
            for (values, expected) in [
                (vec![foreign.clone()], 103),
                (
                    vec![
                        foreign,
                        Value::Global(
                            fir_id_for(user_package, items["DecoyA"]),
                            FunctorApp::default(),
                        ),
                    ],
                    6,
                ),
                (vec![wrapped], 110),
            ] {
                let args = Value::Array(values.into());
                let qir =
                    callable_args_to_qir(&store, user_package, items["Run"], &args, capabilities);
                assert!(
                    qir.contains(&format!(
                        "call void @__quantum__rt__int_record_output(i64 {expected},"
                    )),
                    "foreign closure must retain its package on {profile:?}, capturing={capturing}:\n{qir}"
                );
            }
        }
    }
}

#[test]
fn foreign_operation_closure_arrays_emit_the_library_gate() {
    let user_source = r#"
        namespace Test {
            operation DecoyA(target : Qubit) : Unit { Z(target); }
            operation DecoyB(target : Qubit) : Unit { H(target); }
            operation DecoyC(target : Qubit) : Unit { I(target); }
            operation Run(actions : (Qubit => Unit)[]) : Result {
                use target = Qubit();
                for action in actions { action(target); }
                MResetZ(target)
            }
        }
        "#;
    for capturing in [false, true] {
        for profile in [
            Profile::Base,
            Profile::AdaptiveRI,
            Profile::AdaptiveRIF,
            Profile::Adaptive,
        ] {
            let capabilities = profile.into();
            let (mut store, user_package, items) =
                compile_and_locate_items(user_source, &[("Run", true)], capabilities);
            let std_id = store
                .iter()
                .map(|(id, _)| id)
                .find(|id| *id != PackageId::CORE && *id != user_package)
                .expect("the fixture store contains the standard library");
            let library_source = if capturing {
                "namespace Library { function Make(enabled : Bool) : Qubit => Unit { target => { if enabled { X(target); } } } export Make; }"
            } else {
                "namespace Library { function Make() : Qubit => Unit { target => X(target) } export Make; }"
            };
            let (unit, errors) = crate::compile::compile(
                &store,
                &[(std_id, None)],
                source_map_from_source(library_source),
                PackageType::Lib,
                capabilities,
                LanguageFeatures::default(),
            );
            assert!(errors.is_empty(), "library must compile: {errors:?}");
            let lambda = unit
                .package
                .items
                .iter()
                .find_map(|(id, item)| match &item.kind {
                    ItemKind::Callable(decl)
                        if callable_name_matches(decl.name.name.as_ref(), ".lambda") =>
                    {
                        Some(id)
                    }
                    _ => None,
                })
                .expect("library must contain a lifted lambda");
            let library_package = store.insert(unit);
            let argument = Value::Array(
                vec![Value::Closure(Box::new(qsc_eval::val::Closure {
                    id: fir_id_for(library_package, lambda),
                    fixed_args: if capturing {
                        vec![Value::Bool(true)]
                    } else {
                        vec![]
                    }
                    .into(),
                    functor: FunctorApp::default(),
                }))]
                .into(),
            );
            let qir =
                callable_args_to_qir(&store, user_package, items["Run"], &argument, capabilities);
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body(").count(),
                1,
                "{profile:?} must call the library lambda:\n{qir}"
            );
            assert!(
                !qir.contains("__quantum__qis__h__body"),
                "a same-numbered local item must not be invoked:\n{qir}"
            );
        }
    }
}

#[test]
fn synthetic_path_classical_capture_closure_generates_qir() {
    // A closure capturing a classical value (Int shift) flows through the
    // self-contained synthetic entry: the capture is materialized as a local and
    // the closure is reconstructed for FIR transforms and subsequent partial
    // evaluation to specialize and emit its gates. The target also takes a
    // plain Int argument so the synthetic entry exercises mixed closure/scalar
    // argument lowering.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Canon.*;
            import Std.Measurement.*;

            operation RunOp(op : (Qubit[] => Unit), reps : Int) : Result[] {
                use register = Qubit[2];
                for _ in 1..reps {
                    op(register);
                }
                return MResetEachZ(register);
            }

            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }

            operation MakeShift(shift : Int) : (Qubit[] => Unit) {
                return register => Shifted(shift, register);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunOp", true), ("Shifted", true), (".lambda", true)],
        caps,
    );

    // The lifted lambda's input is `(shift, register)`; capturing `shift = 1`
    // leaves the explicit `register` slot for the synthetic entry to supply.
    let shifted_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Int(1)].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![shifted_closure, Value::Int(1)].into(), None);

    // A classical capture must route through the synthetic entry, not the pin path.
    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &args, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "classical-capture closure should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "classical-capture closure should route to the synthetic entry"
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate from the captured shift in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_closure_arrays_preserve_scalar_and_tuple_inputs() {
    for (input, lambda, argument, expected) in [
        ("Int", "value -> value + 1", "3", 4),
        (
            "(Int, Int)",
            "(left, right) -> left * 10 + right",
            "(2, 3)",
            23,
        ),
        ("Unit", "() -> 7", "()", 7),
    ] {
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(
                &mut interpreter,
                &format!(
                    r#"
                    function CallAll(actions : ({input} -> Int)[]) : Int {{
                        mutable result = 0;
                        for action in actions {{ set result += action({argument}); }}
                        result
                    }}
                    "#
                ),
            );
            let callable = eval_fragments(&mut interpreter, "CallAll");
            let args = eval_fragments(&mut interpreter, &format!("[{lambda}]"));
            let qir = interpreter
                .qirgen_from_callable(&callable, args)
                .unwrap_or_else(|errors| {
                    panic!(
                        "{input} closure array failed on {profile:?}: {}",
                        format_interpret_errors(errors)
                    )
                });
            assert!(
                qir.contains(&format!(
                    "call void @__quantum__rt__int_record_output(i64 {expected},"
                )),
                "{input} closure array should return {expected} on {profile:?}:\n{qir}"
            );
        }
    }
}

#[test]
fn indexed_callable_selections_record_expected_values() {
    // Each call contributes one digit: `Inc(3)` is 4 and `Twice(3)` is 6.
    // `Pick(false, _)` ignores the index; `Pick(true, -1)` reads the last
    // element; `Choose` must call the reassigned `functions[-1]`.
    let source = indoc::indoc! {r#"
        namespace Test {
            function Inc(x : Int) : Int { x + 1 }
            function Twice(x : Int) : Int { 2 * x }
            function Pick(flag : Bool, index : Int) : Int {
                let functions = [Inc, Twice];
                let selected = if flag { functions[index] } else { Inc };
                selected(3)
            }
            function Choose(first : Int, second : Int) : Int {
                let functions = [Inc, Twice];
                mutable selected = functions[first];
                set selected = functions[second];
                selected(3)
            }
            @EntryPoint()
            operation Main() : Int {
                Pick(false, 1) * 10000 + Pick(false, 99) * 1000
                    + Pick(true, 0) * 100 + Pick(true, -1) * 10 + Choose(-2, -1)
            }
        }
    "#};

    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("call void @__quantum__rt__int_record_output(i64 44466,"),
            "{profile:?} should record [4, 4, 4, 6, 6] as 44466:\n{qir}"
        );
    }
}

#[test]
fn forwarded_callable_uses_reassigned_selection() {
    // `Choose(-2, -1)` forwards `Twice` (6) and `Choose(-1, -2)` forwards
    // `Inc` (4), so the program records 64.
    let source = indoc::indoc! {r#"
        namespace Test {
            function Inc(value : Int) : Int { value + 1 }
            function Twice(value : Int) : Int { value * 2 }
            function Apply(action : Int -> Int, value : Int) : Int { action(value) }
            function Choose(first : Int, second : Int) : Int {
                let functions = [Inc, Twice];
                mutable selected = functions[first];
                set selected = functions[second];
                Apply(selected, 3)
            }
            @EntryPoint()
            operation Main() : Int { Choose(-2, -1) * 10 + Choose(-1, -2) }
        }
    "#};
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("call void @__quantum__rt__int_record_output(i64 64,"),
            "{profile:?} should record both reaching selections as 64:\n{qir}"
        );
    }
}

#[test]
fn reassigned_global_callable_records_array_selection() {
    for invoke in ["selected(3)", "Apply(selected, 3)"] {
        let source = format!(
            r#"
            function Inc(value : Int) : Int {{ value + 1 }}
            function Twice(value : Int) : Int {{ value * 2 }}
            function Apply(action : Int -> Int, value : Int) : Int {{ action(value) }}
            function Choose(index : Int) : Int {{
                let functions = [Inc, Twice];
                mutable selected = Inc;
                set selected = functions[index];
                {invoke}
            }}
            @EntryPoint()
            operation Main() : Int {{ Choose(1) }}
            "#
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert!(
                qir.contains("call void @__quantum__rt__int_record_output(i64 6,"),
                "{profile:?} should record 6 for {invoke}:\n{qir}"
            );
        }
    }
}

#[test]
fn runtime_index_reassignment_is_rejected_on_adaptive_ri() {
    for invoke in ["selected(target);", "Apply(selected, target);"] {
        let source = format!(
            r#"
            operation Apply(action : Qubit => Unit, target : Qubit) : Unit {{ action(target); }}
            operation Choose(first : Int, second : Int, target : Qubit) : Unit {{
                let actions = [I, X];
                mutable selected = actions[first];
                set selected = actions[second];
                {invoke}
            }}
            @EntryPoint()
            operation Main() : Result {{
                use flag = Qubit();
                use target = Qubit();
                X(flag);
                let index = if MResetZ(flag) == One {{ 1 }} else {{ 0 }};
                Choose(0, index, target);
                MResetZ(target)
            }}
            "#
        );
        let errors = compile_source_to_qir_result(&source, Profile::AdaptiveRI.into())
            .expect_err("runtime indexed dispatch must retain capability diagnostics");
        let diagnostics = format_interpret_errors(errors);
        assert!(
            diagnostics.contains("Qdk.Qsc.CapabilitiesCk.UseOfDynamicIndex"),
            "expected a capability diagnostic for {invoke}:\n{diagnostics}"
        );
    }
}

#[test]
fn conservative_callable_fallbacks_preserve_results() {
    // Each body takes a conservative (non-specialized) path that must keep its
    // result: an argument that changes the returned callable's guard, a stored
    // partial application with an effectful capture, and an array slot whose
    // element depends on a runtime index.
    for (body, expected) in [
        (
            "mutable flag = true; let value = Choose(flag, { set flag = false; 0 })(3); value * 10 + (if flag { 1 } else { 0 })",
            40,
        ),
        (
            "mutable action = Offset(Mark(), _); action(3) * 100 + action(4)",
            1314,
        ),
        ("ReadSelected(0) * 1000 + ReadSelected(1)", 402_602),
    ] {
        let source = format!(
            r#"
            function Inc(value : Int) : Int {{ value + 1 }}
            function Twice(value : Int) : Int {{ value * 2 }}
            function Minus(value : Int) : Int {{ value - 1 }}
            function Choose(flag : Bool, ignored : Int) : Int -> Int {{
                if flag {{ Inc }} else {{ Twice }}
            }}
            function Mark() : Int {{ Message("capture"); 10 }}
            function Offset(offset : Int, value : Int) : Int {{ value + offset }}
            function Read(actions : (Int -> Int)[]) : Int {{
                mutable answer = 0;
                for action in actions {{ set answer = answer * 100 + action(3); }}
                answer
            }}
            function ReadSelected(index : Int) : Int {{
                let actions = [Inc, Twice];
                let choices = [actions[index], Minus];
                Read(choices)
            }}
            @EntryPoint()
            operation Main() : Int {{ {body} }}
            "#
        );
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert!(
                qir.contains(&format!(
                    "call void @__quantum__rt__int_record_output(i64 {expected},"
                )),
                "{profile:?} must preserve {expected} for {body}:\n{qir}"
            );
        }
    }
}

#[test]
fn inline_measurement_capture_is_evaluated_once() {
    // A partial application whose capture is a measurement must still be
    // specialized: declining it leaves a dynamic callable that Adaptive
    // targets reject.
    let source = r#"
        function FromFlag(flag : Bool, value : Int) : Int {
            if flag { value + 1 } else { value + 2 }
        }
        function Apply(action : Int -> Int, value : Int) : Int { action(value) }
        @EntryPoint()
        operation Main() : Int {
            use flag = Qubit();
            Apply(FromFlag(MResetZ(flag) == One, _), 3)
        }
        "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert_eq!(
            qir.matches("call void @__quantum__qis__mresetz__body(")
                .count(),
            1,
            "capture initialization must measure once on {profile:?}:\n{qir}"
        );
        assert!(
            qir.contains("call void @__quantum__rt__int_record_output(i64 %"),
            "{profile:?} must preserve the measurement-dependent result:\n{qir}"
        );
    }
}

#[test]
fn aliased_guards_select_expected_gate() {
    for (threshold, expected, excluded) in [(1, "h", "z"), (3, "z", "h")] {
        let source = format!(
            r#"
            namespace Demo {{
                function Choose(b : Bool) : (Qubit => Unit) {{
                    if b {{ H }} else {{ if b {{ X }} else {{ Z }} }}
                }}
                @EntryPoint()
                operation Main() : Result {{
                    use q = Qubit();
                    let arr = [1, 2, 3];
                    let x = Length(arr) > {threshold};
                    let f = Choose(x);
                    f(q);
                    MResetZ(q)
                }}
            }}
            "#
        );
        for profile in [
            Profile::Base,
            Profile::AdaptiveRI,
            Profile::AdaptiveRIF,
            Profile::Adaptive,
        ] {
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(&mut interpreter, &source);
            for (path, qir) in [
                ("program", compile_source_to_qir(&source, profile.into())),
                (
                    "interpreter",
                    interpreter
                        .qirgen("Demo.Main()")
                        .expect("issue 3768 must generate QIR"),
                ),
            ] {
                assert_eq!(
                    qir.matches(&format!("call void @__quantum__qis__{expected}__body("))
                        .count(),
                    1,
                    "{profile:?} {path} must select {expected} for threshold {threshold}:\n{qir}"
                );
                assert!(
                    !qir.contains("call void @__quantum__qis__x__body(")
                        && !qir.contains(&format!("call void @__quantum__qis__{excluded}__body(")),
                    "{profile:?} {path} must not select another branch:\n{qir}"
                );
            }
        }
    }
}

#[test]
fn aliased_runtime_guards_compile() {
    let source = indoc::indoc! {r#"
        namespace Demo {
            function Choose(b : Bool) : (Qubit => Unit) {
                if b { H } else { if b { X } else { Z } }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                use guard = Qubit();
                let flag = MResetZ(guard) == One;
                let action = Choose(flag);
                action(target);
                MResetZ(target)
            }
        }
    "#};
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("br i1")
                && qir.contains("call void @__quantum__qis__h__body(")
                && qir.contains("call void @__quantum__qis__z__body("),
            "{profile:?} must retain both runtime outcomes:\n{qir}"
        );
    }
}

#[test]
fn nested_runtime_guards_compile_on_adaptive_profiles() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Choose(first : Bool, second : Bool) : (Qubit => Unit) {
                if first { I } else { if second { X } else { Z } }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                use guard = Qubit();
                let first = M(guard) == One;
                let second = M(guard) == Zero;
                let selected = Choose(first, second);
                selected(target);
                Reset(guard);
                MResetZ(target)
            }
        }
    "#};

    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("call void @__quantum__qis__x__body("),
            "{profile:?} should resolve the runtime-guarded callable selection:\n{qir}"
        );
    }
}

#[test]
fn forwarded_callable_array_emits_both_operations() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunLoop(
                operations : ((Qubit, Qubit) => Unit is Adj)[],
                qubits : Qubit[]
            ) : Unit is Adj {
                for index in 0..Length(operations) - 1 {
                    operations[index](qubits[index], qubits[2]);
                }
            }
            function MakeLoop(
                operations : ((Qubit, Qubit) => Unit is Adj)[]
            ) : Qubit[] => Unit is Adj {
                RunLoop(operations, _)
            }
            operation Forward(action : Qubit[] => Unit is Adj, qubits : Qubit[]) : Unit {
                action(qubits);
            }
            operation Apply(action : Qubit[] => Unit is Adj) : Result[] {
                use qubits = Qubit[3];
                X(qubits[0]);
                X(qubits[1]);
                Forward(action, qubits);
                MResetEachZ(qubits)
            }
            operation First(control : Qubit, target : Qubit) : Unit is Adj {
                CNOT(control, target);
            }
            operation Second(control : Qubit, target : Qubit) : Unit is Adj {
                CNOT(control, target);
                X(control);
            }
            @EntryPoint()
            operation Main() : Result[] {
                Apply(MakeLoop([First, Second]))
            }
        }
    "#};

    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let qir = compile_source_to_qir(source, profile.into());
        let (cnot_call, x_call) = if profile == Profile::Adaptive {
            for name in ["First", "Second"] {
                assert_eq!(
                    qir.matches(&format!("call void @{name}(")).count(),
                    1,
                    "{qir}"
                );
            }
            ("call void @CNOT(", "call void @X(")
        } else {
            (
                "call void @__quantum__qis__cx__body(",
                "call void @__quantum__qis__x__body(",
            )
        };
        assert_eq!(
            qir.matches(cnot_call).count(),
            2,
            "{profile:?} must apply both elements:\n{qir}"
        );
        assert_eq!(
            qir.matches(x_call).count(),
            3,
            "{profile:?} must retain the second element's X:\n{qir}"
        );
    }
}

#[test]
fn synthetic_path_returned_for_each_with_generic_global_capture_generates_qir() {
    let caps = Profile::AdaptiveRI.into();
    let mut interpreter = interpreter_with_capabilities(caps);
    eval_fragments(
        &mut interpreter,
        indoc::indoc! {r#"
            import Std.Arrays.ForEach;

            function Make() : Int[][] => Int[] {
                let f = Length;
                ForEach(arr => f(arr), _)
            }

            operation Apply(op : Int[][] => Int[], arrs : Int[][]) : Int[] {
                return op(arrs);
            }
        "#},
    );

    let op = eval_fragments(&mut interpreter, "Make()");
    let apply = eval_fragments(&mut interpreter, "Apply");
    let args = Value::Tuple(
        vec![
            op,
            Value::Array(
                vec![
                    Value::Array(vec![Value::Int(1), Value::Int(2)].into()),
                    Value::Array(vec![Value::Int(1)].into()),
                ]
                .into(),
            ),
        ]
        .into(),
        None,
    );

    let qir = interpreter
        .qirgen_from_callable(&apply, args)
        .unwrap_or_else(|errors| {
            panic!(
                "returned ForEach callable should compile, got: {}",
                format_interpret_errors(errors)
            )
        });
    assert!(
        qir.contains("i64 2") && qir.contains("i64 1"),
        "expected QIR to contain the computed lengths [2, 1]:\n{qir}"
    );
}

#[test]
fn synthetic_path_returned_for_each_direct_length_closure_generates_qir() {
    let caps = Profile::AdaptiveRI.into();
    let mut interpreter = interpreter_with_capabilities(caps);
    eval_fragments(
        &mut interpreter,
        indoc::indoc! {r#"
            import Std.Arrays.ForEach;

            function Make() : Int[][] => Int[] {
                ForEach(arr => Length(arr), _)
            }

            operation Apply(op : Int[][] => Int[], arrs : Int[][]) : Int[] {
                return op(arrs);
            }
        "#},
    );

    let op = eval_fragments(&mut interpreter, "Make()");
    let apply = eval_fragments(&mut interpreter, "Apply");
    let args = Value::Tuple(
        vec![
            op,
            Value::Array(
                vec![
                    Value::Array(vec![Value::Int(1), Value::Int(2)].into()),
                    Value::Array(vec![Value::Int(1)].into()),
                ]
                .into(),
            ),
        ]
        .into(),
        None,
    );

    let qir = interpreter
        .qirgen_from_callable(&apply, args)
        .unwrap_or_else(|errors| {
            panic!(
                "returned direct ForEach callable should compile, got: {}",
                format_interpret_errors(errors)
            )
        });
    assert!(
        qir.contains("i64 2") && qir.contains("i64 1"),
        "expected QIR to contain the computed lengths [2, 1]:\n{qir}"
    );
}

#[test]
fn synthetic_path_controlled_hof_forwarded_capturing_closure_generates_qir() {
    // A capturing closure (`MakeSelectOp` captures `coeffs`) forwarded through a
    // CONTROLLED two-callable HOF (`PrepSelPrep`) and invoked under `Controlled`
    // must nest its capture inside the base input beneath the controls, preserving
    // each control layer's `(controls, input)` pair. Appending the capture as a
    // third element beside the controls and input would break the shape required
    // by `split_controls_and_input` in qsc_rca.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation SelectWithCoeffs(coeffs : Int[], systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                for _ in coeffs {
                    X(systems[0]);
                }
            }

            operation MakeSelectOp(coeffs : Int[]) : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) {
                return (systems, ancilla) => SelectWithCoeffs(coeffs, systems, ancilla);
            }

            operation PrepareIdentity(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                controlled (ctls, ...) {}
            }

            operation PrepSelPrep(
                prepareOp : (Qubit[] => Unit is Adj + Ctl),
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                systems : Qubit[],
                ancilla : Qubit[]
            ) : Unit is Adj + Ctl {
                body ... {
                    prepareOp(systems);
                    selectOp(systems, ancilla);
                    Adjoint prepareOp(systems);
                }
                controlled (ctls, ...) {
                    prepareOp(systems);
                    Controlled selectOp(ctls, (ancilla, systems));
                    Adjoint prepareOp(systems);
                }
            }

            operation RunCircuit(
                prepareOp : (Qubit[] => Unit is Adj + Ctl),
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                power : Int
            ) : Result[] {
                use systems = Qubit[1];
                use ancilla = Qubit[1];
                use control = Qubit();
                for _ in 1..power {
                    Controlled PrepSelPrep([control], (prepareOp, selectOp, systems, ancilla));
                }
                return MResetEachZ(systems + ancilla);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunCircuit", true),
            ("PrepareIdentity", true),
            ("SelectWithCoeffs", true),
            (".lambda", true),
        ],
        caps,
    );

    // prepare = Global(PrepareIdentity); select = Closure capturing coeffs = [1].
    let prepare = Value::Global(
        fir_id_for(pkg, items["PrepareIdentity"]),
        FunctorApp::default(),
    );
    let select = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Array(Rc::new(vec![Value::Int(1)]))].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![prepare, select, Value::Int(1)].into(), None);

    // The captured `coeffs = [1]` (length 1) drives exactly one `X(systems[0])`
    // inside `SelectWithCoeffs`; the assertion checks that a `cx` (controlled-X)
    // is emitted, guarding against a dropped capture. It does not count gates.
    let qir = callable_args_to_qir(&store, pkg, items["RunCircuit"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__cx__body"),
        "expected controlled X (cx) gate from the forwarded select closure in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_doubly_controlled_hof_forwarded_capturing_closure_generates_qir() {
    // The controlled==1 forwarding case above threads a capture beneath a
    // single control layer. This case forwards the same capturing closure
    // through the same two-callable HOF (`PrepSelPrep`) but invokes it under
    // `Controlled Controlled`, so the capture must nest beneath BOTH control
    // layers, preserving each layer's `(controls, input)` pair. Appending the
    // capture as a top-level sibling of the outer control register would break
    // the outermost control layer's 2-tuple shape and `split_controls_and_input` in qsc_rca
    // would reject it on the first control-layer peel.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation SelectWithCoeffs(coeffs : Int[], systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                for _ in coeffs {
                    X(systems[0]);
                }
            }

            operation MakeSelectOp(coeffs : Int[]) : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) {
                return (systems, ancilla) => SelectWithCoeffs(coeffs, systems, ancilla);
            }

            operation PrepareIdentity(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                controlled (ctls, ...) {}
            }

            operation PrepSelPrep(
                prepareOp : (Qubit[] => Unit is Adj + Ctl),
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                systems : Qubit[],
                ancilla : Qubit[]
            ) : Unit is Adj + Ctl {
                body ... {
                    prepareOp(systems);
                    selectOp(systems, ancilla);
                    Adjoint prepareOp(systems);
                }
                controlled (ctls, ...) {
                    prepareOp(systems);
                    Controlled selectOp(ctls, (ancilla, systems));
                    Adjoint prepareOp(systems);
                }
            }

            operation RunCircuit(
                prepareOp : (Qubit[] => Unit is Adj + Ctl),
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                power : Int
            ) : Result[] {
                use systems = Qubit[1];
                use ancilla = Qubit[1];
                use c1 = Qubit();
                use c2 = Qubit();
                for _ in 1..power {
                    Controlled Controlled PrepSelPrep([c2], ([c1], (prepareOp, selectOp, systems, ancilla)));
                }
                return MResetEachZ(systems + ancilla);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunCircuit", true),
            ("PrepareIdentity", true),
            ("SelectWithCoeffs", true),
            (".lambda", true),
        ],
        caps,
    );

    let prepare = Value::Global(
        fir_id_for(pkg, items["PrepareIdentity"]),
        FunctorApp::default(),
    );
    let select = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Array(Rc::new(vec![Value::Int(1)]))].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![prepare, select, Value::Int(1)].into(), None);

    // The captured `coeffs = [1]` drives one `X(systems[0])` inside
    // `SelectWithCoeffs`; applied under two control layers it lowers to a
    // doubly-controlled X (`ccx`), proving the capture was threaded beneath
    // both control layers rather than dropped or panic-silenced.
    let qir = callable_args_to_qir(&store, pkg, items["RunCircuit"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__ccx__body"),
        "expected doubly-controlled X (ccx) gate from the forwarded select closure in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_single_callable_doubly_controlled_hof_forwarded_capturing_closure_generates_qir()
{
    // Single-callable variant of the doubly-controlled forwarding case: the
    // capture is threaded through a one-callable HOF (`ApplySelect`, whose
    // controlled specialization is auto-generated) invoked under
    // `Controlled Controlled`. The number of callable parameters does not
    // change the control-layer capture placement, so this must nest the
    // capture beneath both control layers exactly like the two-callable case.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation SelectWithCoeffs(coeffs : Int[], systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                for _ in coeffs {
                    X(systems[0]);
                }
            }

            operation MakeSelectOp(coeffs : Int[]) : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) {
                return (systems, ancilla) => SelectWithCoeffs(coeffs, systems, ancilla);
            }

            operation ApplySelect(
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                systems : Qubit[],
                ancilla : Qubit[]
            ) : Unit is Adj + Ctl {
                selectOp(systems, ancilla);
            }

            operation RunCircuit(
                selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl),
                power : Int
            ) : Result[] {
                use systems = Qubit[1];
                use ancilla = Qubit[1];
                use c1 = Qubit();
                use c2 = Qubit();
                for _ in 1..power {
                    Controlled Controlled ApplySelect([c2], ([c1], (selectOp, systems, ancilla)));
                }
                return MResetEachZ(systems + ancilla);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunCircuit", true),
            ("SelectWithCoeffs", true),
            (".lambda", true),
        ],
        caps,
    );

    let select = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Array(Rc::new(vec![Value::Int(1)]))].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![select, Value::Int(1)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunCircuit"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__ccx__body"),
        "expected doubly-controlled X (ccx) gate from the forwarded select closure in QIR:\n{qir}"
    );
}

#[test]
fn deep_var_local_bound_nested_udt_callable_generates_qir() {
    // A HOF (`RunConfig`) whose single parameter is a doubly-nested UDT
    // (`Config` wrapping an `OpBox` wrapping a callable) is called with a
    // `let`-bound local built from UDT constructors. Defunctionalization must
    // deep-strip the callable field from the local's initializer at the call
    // site so the local no longer retains an arrow-typed field, otherwise the
    // nested tuple-arrow local check after tuple decomposition would fire.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            newtype OpBox = (Op : Qubit => Unit, Id : Int);
            newtype Config = (Inner : OpBox, N : Int);

            operation RunConfig(cfg : Config) : Unit {
                use q = Qubit();
                cfg::Inner::Op(q);
            }

            operation DoX(q : Qubit) : Unit {
                X(q);
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let cfg = Config(OpBox(DoX, 1), 5);
                RunConfig(cfg);
                MResetZ(q)
            }
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate from the deep-nested UDT callable in QIR:\n{qir}"
    );
}

#[test]
fn capture_carrying_deep_var_local_bound_nested_udt_callable_generates_qir() {
    // Like the deep-nested UDT case above, but the innermost callable is a
    // capturing closure (`qq => RotateBy(angle, qq)` capturing `angle`) bound
    // through the same doubly-nested UDT local. After deep-stripping the
    // callable field, the captured `angle` must be threaded to the specialized
    // callable so the `Rz` receives the captured rotation, rather than dropping
    // the capture or building a self-referential argument expression.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            newtype OpBox = (Op : Qubit => Unit, Id : Int);
            newtype Config = (Inner : OpBox, N : Int);

            operation RunConfig(cfg : Config) : Unit {
                use q = Qubit();
                cfg::Inner::Op(q);
            }

            operation RotateBy(theta : Double, q : Qubit) : Unit {
                Rz(theta, q);
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let angle = 1.5;
                let cfg = Config(OpBox(qq => RotateBy(angle, qq), 1), 5);
                RunConfig(cfg);
                MResetZ(q)
            }
        }
    "#};
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 1.5"),
        "expected Rz with the captured angle 1.5 from the deep-nested capturing closure in QIR:\n{qir}"
    );
}

#[test]
fn same_target_multi_closure_args_route_to_synthetic_entry_and_generate_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation InvokeThree(
                first : Qubit => Unit,
                second : Qubit => Unit,
                third : Qubit => Unit
            ) : Unit {
                use q = Qubit();
                first(q);
                second(q);
                third(q);
            }

            operation ApplyRz(theta : Double, q : Qubit) : Unit {
                Rz(theta, q);
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("InvokeThree", true), ("ApplyRz", true)], caps);

    let apply_rz = fir_id_for(pkg, items["ApplyRz"]);
    let first = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Double(1.0)].into(),
        id: apply_rz,
        functor: FunctorApp::default(),
    }));
    let second = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Double(2.0)].into(),
        id: apply_rz,
        functor: FunctorApp::default(),
    }));
    let third = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Double(3.0)].into(),
        id: apply_rz,
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![first, second, third].into(), None);

    let target_hir = hir_id_for(pkg, items["InvokeThree"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &args, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "same-target classical closures should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "same-target classical closures should route to the synthetic entry"
    );

    let qir = callable_args_to_qir(&store, pkg, items["InvokeThree"], &args, caps);
    let expected_calls = [
        "call void @__quantum__qis__rz__body(double 1.0,",
        "call void @__quantum__qis__rz__body(double 2.0,",
        "call void @__quantum__qis__rz__body(double 3.0,",
    ];
    let counts: Vec<_> = expected_calls
        .iter()
        .map(|call| qir.matches(call).count())
        .collect();
    assert_eq!(
        counts,
        vec![1, 1, 1],
        "expected each captured rotation exactly once in QIR:\n{qir}"
    );
    let positions: Vec<_> = expected_calls
        .iter()
        .map(|call| qir.find(call).expect("expected Rz call in QIR"))
        .collect();
    let mut sorted_positions = positions.clone();
    sorted_positions.sort_unstable();
    assert_eq!(
        positions, sorted_positions,
        "expected captured rotations to be emitted in argument order:\n{qir}"
    );
}

#[test]
fn nested_closure_arg_routes_to_synthetic_entry_and_generates_inner_effect() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation InvokeOne(op : Qubit => Unit) : Unit {
                use q = Qubit();
                op(q);
            }

            operation ApplyInner(inner : Qubit => Unit, q : Qubit) : Unit {
                inner(q);
            }

            function MakeRz(theta : Double) : Qubit => Unit {
                Rz(theta, _)
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("InvokeOne", true), ("ApplyInner", true), (".lambda", true)],
        caps,
    );

    let inner = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Double(4.0)].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let outer = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![inner].into(),
        id: fir_id_for(pkg, items["ApplyInner"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["InvokeOne"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &outer, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "nested classical closure should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "nested classical closure should route to the synthetic entry"
    );

    let qir = callable_args_to_qir(&store, pkg, items["InvokeOne"], &outer, caps);
    assert_eq!(
        qir.matches("call void @__quantum__qis__rz__body(double 4.0,")
            .count(),
        1,
        "expected one inner captured rotation in QIR:\n{qir}"
    );
}

#[test]
fn classical_capture_closure_routes_to_synthetic_entry_qubit_capture_does_not() {
    // A closure capturing a runtime qubit identity cannot be lowered to a FIR
    // literal, so it must keep the pin-based `ReinvokeOriginal` route.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation RunOp(op : (Qubit => Unit)) : Result {
                use q = Qubit();
                op(q);
                return MResetZ(q);
            }

            operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
                CNOT(control, target);
            }

            operation MakeEntangler(control : Qubit) : (Qubit => Unit) {
                return target => Entangle(control, target);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunOp", true), ("Entangle", true), (".lambda", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture closure should still produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
}

#[test]
fn synthetic_path_array_arg_preserves_element_values() {
    // A `Value::Array` argument must survive materialization on the synthetic
    // path with its element values intact (not just its length). Each nonzero
    // element drives one `op(q)` call, so the gate count checks that `[1, 0, 1]`
    // produces two calls; it does not distinguish element order or nonzero values.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunWith(op : Qubit => Unit, data : Int[]) : Result {
                use q = Qubit();
                for x in data {
                    if x != 0 {
                        op(q);
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunWith", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![
            do_x,
            Value::Array(vec![Value::Int(1), Value::Int(0), Value::Int(1)].into()),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunWith"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        2,
        "expected exactly 2 X gates (one per nonzero element) in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_empty_array_arg_does_not_panic() {
    // An empty `Value::Array` retains its element type through synthetic-entry
    // lowering, so codegen succeeds and emits no `op(q)` calls.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunWith(op : Qubit => Unit, data : Int[]) : Result {
                use q = Qubit();
                for x in data {
                    if x != 0 {
                        op(q);
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunWith", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![do_x, Value::Array(vec![].into())].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunWith"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        0,
        "expected no X gates for an empty array argument in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_nested_empty_array_arg_does_not_panic() {
    // A nested array `[[]] : Int[][]` retains its element types through
    // synthetic-entry lowering, so codegen succeeds and emits no `op(q)` calls.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunNested(op : Qubit => Unit, data : Int[][]) : Result {
                use q = Qubit();
                for inner in data {
                    for x in inner {
                        if x != 0 {
                            op(q);
                        }
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunNested", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![do_x, Value::Array(vec![Value::Array(vec![].into())].into())].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunNested"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        0,
        "expected no X gates for a nested empty array argument in QIR:\n{qir}"
    );
}

// ---- SyntheticEntry vs ReinvokeOriginal early-return-in-dynamic-branch parity ----

/// Target body shared by the early-return parity tests below. Its early `return` sits
/// inside a measurement-dependent (`MResetZ`) branch with statements after the branch,
/// which is exactly the `ReturnWithinDynamicScope` shape that the entry-reachable
/// `return_unify` pass rewrites.
const EARLY_RETURN_TARGET_BODY: &str = r#"
    operation RunOp(op : (Qubit => Unit)) : Int {
        let r = {
            use q = Qubit();
            op(q);
            MResetZ(q)
        };
        if r == One {
            return 1;
        }
        return 2;
    }
"#;

/// Source whose closure arg captures a classical value, so the arg is FIR-lowerable
/// and routes through the self-contained `SyntheticEntry` backend.
fn early_return_synthetic_entry_source() -> String {
    format!(
        r#"
namespace Test {{
    import Std.Measurement.*;
{EARLY_RETURN_TARGET_BODY}
    operation Rotate(reps : Int, target : Qubit) : Unit {{
        for _ in 1..reps {{
            X(target);
        }}
    }}

    operation MakeRotation(reps : Int) : (Qubit => Unit) {{
        return target => Rotate(reps, target);
    }}
}}
"#
    )
}

/// Source whose closure arg captures an allocated qubit (a runtime identity that is
/// NOT FIR-lowerable), forcing the pin-based `ReinvokeOriginal` backend.
fn early_return_reinvoke_original_source() -> String {
    format!(
        r#"
namespace Test {{
    import Std.Measurement.*;
{EARLY_RETURN_TARGET_BODY}
    operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {{
        CNOT(control, target);
    }}

    operation MakeEntangler(control : Qubit) : (Qubit => Unit) {{
        return target => Entangle(control, target);
    }}
}}
"#
    )
}

/// Parity regression for the `SyntheticEntry`-vs-`ReinvokeOriginal` capability behavior
/// on early-return-in-dynamic-branch closures.
///
/// Both variants pass the SAME target operation (`RunOp`), whose body early-returns
/// inside a measurement-dependent branch. Their closures perform different gates
/// and use different capture types:
///
/// * Classical capture -> FIR-lowerable -> `SyntheticEntry`. The target body becomes
///   entry-reachable through the synthetic `Call`, so `return_unify` rewrites the early
///   return and the program compiles to QIR under an Adaptive profile.
/// * Qubit capture -> NOT FIR-lowerable -> `ReinvokeOriginal`. The target body is pinned
///   (not entry-reachable), so the main pipeline never return-unifies it. The body-only
///   signature-preserving sub-pipeline runs on the pinned body, so the early return is
///   rewritten into flag-guarded forward control flow and the program compiles to QIR
///   under an Adaptive profile with parity to the `SyntheticEntry` variant.
///
/// This test asserts both routes compile.
#[test]
fn early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity() {
    let caps = Profile::AdaptiveRIF.into();

    // --- SyntheticEntry variant: classical capture compiles to QIR. ---
    let synthetic_source = early_return_synthetic_entry_source();
    let (store, pkg, items) = compile_and_locate_items(
        &synthetic_source,
        &[("RunOp", true), ("Rotate", true), (".lambda", true)],
        caps,
    );

    // The lifted lambda's input is `(reps, target)`; capturing `reps = 1` leaves the
    // explicit `target` slot for the closure invocation to supply.
    let rotation_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Int(1)].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &rotation_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "classical-capture early-return closure should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "classical-capture closure should route to the synthetic entry"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &rotation_closure, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate from the SyntheticEntry early-return body in QIR:\n{qir}"
    );

    // --- ReinvokeOriginal variant: qubit capture compiles to QIR. ---
    let reinvoke_source = early_return_reinvoke_original_source();
    let (store, pkg, items) = compile_and_locate_items(
        &reinvoke_source,
        &[("RunOp", true), ("Entangle", true), (".lambda", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture early-return closure should compile \
                     (ReinvokeOriginal sub-pipeline return-unifies the pinned body), got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &qubit_closure, caps);
    assert!(
        qir.contains("__quantum__qis__cnot__body") || qir.contains("__quantum__qis__cx__body"),
        "expected the entangler CNOT from the ReinvokeOriginal early-return body in QIR:\n{qir}"
    );
}

/// Parity check: the `ReinvokeOriginal` early-return-in-dynamic-branch body compiles to
/// QIR with parity to the `SyntheticEntry` variant now that the body-only
/// signature-preserving sub-pipeline runs `return_unify` on the pinned target body.
///
/// This is the focused twin of
/// `early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity`: it isolates
/// the `ReinvokeOriginal` route and asserts the entangler CNOT appears in the QIR.
#[test]
fn early_return_in_dynamic_branch_reinvoke_original_compiles_parity_target() {
    let caps = Profile::AdaptiveRIF.into();

    let reinvoke_source = early_return_reinvoke_original_source();
    let (store, pkg, items) = compile_and_locate_items(
        &reinvoke_source,
        &[("RunOp", true), ("Entangle", true), (".lambda", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture early-return closure should compile, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &qubit_closure, caps);
    assert!(
        qir.contains("__quantum__qis__cnot__body") || qir.contains("__quantum__qis__cx__body"),
        "expected the entangler CNOT from the ReinvokeOriginal early-return body in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_udt_wrapped_controlled_callable_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype CtlBox = (Op: ((Qubit[], Qubit) => Unit), Tag: Int);
            operation Invoke(b : CtlBox) : Result {
                use (control, target) = (Qubit(), Qubit());
                b::Op([control], target);
                Reset(control);
                MResetZ(target)
            }
            operation DoX(q : Qubit) : Unit is Ctl { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("Invoke", true), ("DoX", true), ("CtlBox", false)],
        caps,
    );

    let controlled_do_x = Value::Global(
        fir_id_for(pkg, items["DoX"]),
        FunctorApp {
            adjoint: false,
            controlled: 1,
        },
    );
    let boxed = Value::Tuple(
        vec![controlled_do_x, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["CtlBox"]))),
    );

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &boxed, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "controlled UDT-wrapped callable should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__cx__body"),
        "expected controlled X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: struct wrapping a callable field ----

#[test]
fn synthetic_path_single_field_struct_wrapping_callable_generates_qir() {
    // Single-field UDT constructors are transparent in Value form: OpBox(DoH)
    // is represented as the bare Global callable value.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunBoxed", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());

    let qir = callable_args_to_qir(&store, pkg, items["RunBoxed"], &do_h, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_struct_wrapping_callable_and_tag_generates_qir() {
    // A newtype that wraps a callable and a non-callable field.
    // This keeps tuple structure in the runtime Value while still exercising
    // UDT pure-type discovery.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit, Tag: Int);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunBoxed", true), ("DoH", true), ("OpBox", false)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let boxed = Value::Tuple(
        vec![do_h, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunBoxed"], &boxed, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_udt_wrapped_adjoint_callable_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit is Adj, Tag: Int);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoS(q: Qubit) : Unit is Adj { S(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunBoxed", true), ("DoS", true), ("OpBox", false)],
        caps,
    );

    let adjoint_do_s = Value::Global(
        fir_id_for(pkg, items["DoS"]),
        FunctorApp {
            adjoint: true,
            controlled: 0,
        },
    );
    let boxed = Value::Tuple(
        vec![adjoint_do_s, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );

    let target_hir = hir_id_for(pkg, items["RunBoxed"]);
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &boxed, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "adjoint UDT-wrapped callable should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__s__adj"),
        "expected adjoint S gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: callable arg with additional non-callable tuple values ----

#[test]
fn synthetic_path_callable_with_double_and_string_generates_qir() {
    // Target takes (factor: Double, op: Qubit => Unit, label: String).
    // All three value types exercise different branches in `lower_value_to_expr`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Tagged(factor : Double, op : Qubit => Unit, label : String) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Tagged", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![Value::Double(1.5), do_h, Value::String("test".into())].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Tagged"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: nested struct (UDT inside UDT) with callable ----

#[test]
fn synthetic_path_nested_struct_with_callable_generates_qir() {
    // Two levels of UDT wrapping: Config(Inner: OpBox, N: Int) where
    // OpBox(Op: Qubit => Unit, Id: Int). This exercises UDT pure-type descent
    // and nested field-chain replacement in defunctionalization.
    // Two fields keep the inner value tuple-shaped; a single-field UDT would
    // use its unwrapped value, as in the single-field test above.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit, Id: Int);
            newtype Config = (Inner: OpBox, N: Int);
            operation RunConfig(cfg: Config) : Result {
                use q = Qubit();
                cfg::Inner::Op(q);
                MResetZ(q)
            }
            operation DoX(q: Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunConfig", true),
            ("DoX", true),
            ("Config", false),
            ("OpBox", false),
        ],
        caps,
    );

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let inner = Value::Tuple(
        vec![do_x, Value::Int(1)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );
    let config = Value::Tuple(
        vec![inner, Value::Int(5)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Config"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunConfig"], &config, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_callable_field_taking_udt_with_callable_generates_qir() {
    // Outer wraps a callable whose input is Inner, and Inner itself wraps a
    // callable. This exercises UDT expansion through arrow input types, not
    // just nested UDT fields that directly contain callable values.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Inner = (NestedOp: Qubit => Unit, Id: Int);
            newtype Outer = (ApplyInner: Inner => Result, Id: Int);

            operation Invoke(outer: Outer) : Result {
                let inner = Inner(DoH, 2);
                outer::ApplyInner(inner)
            }

            operation UseInner(inner: Inner) : Result {
                use q = Qubit();
                inner::NestedOp(q);
                MResetZ(q)
            }

            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("Invoke", true),
            ("UseInner", true),
            ("DoH", true),
            ("Inner", false),
            ("Outer", false),
        ],
        caps,
    );

    let use_inner = Value::Global(fir_id_for(pkg, items["UseInner"]), FunctorApp::default());
    let outer = Value::Tuple(
        vec![use_inner, Value::Int(1)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Outer"]))),
    );

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &outer, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "callable field taking a UDT with a callable should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: tuple arg where only one element is callable ----

#[test]
fn synthetic_path_tuple_with_one_callable_among_many_scalars() {
    // (Int, Int, Qubit => Unit, Bool, Int) — callable buried deep in a wide tuple.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Wide(a : Int, b : Int, op : Qubit => Unit, flag : Bool, c : Int) : Result {
                use q = Qubit();
                if flag { op(q); }
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Wide", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![
            Value::Int(1),
            Value::Int(2),
            do_h,
            Value::Bool(true),
            Value::Int(4),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Wide"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: plain tuple with callable ----

#[test]
fn plain_tuple_with_callable_takes_synthetic_path() {
    // A plain `Value::Tuple(_, None)` (no UDT tag) containing a callable takes
    // the same synthetic path as UDT values.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunPair(op : Qubit => Unit, n : Int) : Result {
                use q = Qubit();
                for _ in 0..n - 1 { op(q); }
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunPair", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    // Plain tuple — no UDT tag.
    let args = Value::Tuple(vec![do_h, Value::Int(2)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunPair"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: struct with two callable fields ----

#[test]
fn synthetic_path_struct_with_two_callable_fields_generates_qir() {
    // A newtype with two arrow fields. Both are wrapped in the UDT.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Ops = (First: Qubit => Unit, Second: Qubit => Unit);
            operation RunOps(ops: Ops) : Result {
                use q = Qubit();
                ops::First(q);
                ops::Second(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
            operation DoX(q: Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunOps", true),
            ("DoH", true),
            ("DoX", true),
            ("Ops", false),
        ],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let ops = Value::Tuple(
        vec![do_h, do_x].into(),
        Some(Rc::new(fir_id_for(pkg, items["Ops"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunOps"], &ops, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
    // `First` maps to `DoH` and `Second` to `DoX`. Confirm the per-field
    // dispatch resolves in that order, guarding against a field-index mix-up
    // where the second callable field collapses onto the first.
    let h_pos = qir.find("__quantum__qis__h__body").expect("H gate present");
    let x_pos = qir.find("__quantum__qis__x__body").expect("X gate present");
    assert!(
        h_pos < x_pos,
        "expected First (H) to be emitted before Second (X):\n{qir}"
    );
}

// ---- Synthetic path: callable with a Pauli arg and Result output ----

#[test]
fn synthetic_path_callable_with_pauli_and_result_values() {
    // Exercises the Pauli branch of `lower_value_to_expr`; Result is the
    // operation's output, not an argument supplied to the synthetic entry.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Measure(op : Qubit => Unit, basis : Pauli) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Measure", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![do_h, Value::Pauli(qsc_fir::fir::Pauli::Z)].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Measure"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: callable whose RETURN type is a closure ----

#[test]
fn callable_returning_closure_arg_generates_qir() {
    // `MakeOp` returns a closure (`() => H(First(qs))`). The synthetic-entry
    // route must preserve that closure and generate QIR containing the H gate.
    let source = indoc::indoc! {r#"
        namespace Test {
            function First<'T>(arr : 'T[]) : 'T { arr[0] }
            function MakeOp(qs : Qubit[]) : Unit => Unit is Adj + Ctl {
                () => H(First(qs))
            }
            operation DoOp(make : Qubit[] -> Unit => Unit is Adj + Ctl) : Unit is Adj + Ctl {
                use qs = Qubit[1];
                let op = make(qs);
                op();
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("DoOp", true), ("MakeOp", true)], caps);

    let make_op = Value::Global(fir_id_for(pkg, items["MakeOp"]), FunctorApp::default());
    let qir = callable_args_to_qir(&store, pkg, items["DoOp"], &make_op, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_sequential_partial_application_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Arrays.Subarray;

            operation ApplyFirstStep(systems : Qubit[]) : Unit {
                for q in systems {
                    H(q);
                }
            }

            operation ApplySecondStep(systems : Qubit[]) : Unit {
                for q in systems {
                    X(q);
                }
            }

            operation ApplyThirdStep(systems : Qubit[]) : Unit {
                for q in systems {
                    Z(q);
                }
            }

            operation ApplySequential(
                first : Qubit[] => Unit,
                second : Qubit[] => Unit,
                systems : Qubit[]
            ) : Unit {
                first(systems);
                second(systems);
            }

            function MakeSequentialOp(
                first : Qubit[] => Unit,
                second : Qubit[] => Unit
            ) : Qubit[] => Unit {
                ApplySequential(first, second, _)
            }

            function MaxInt(values : Int[]) : Int {
                mutable max = values[0];
                for idx in 1 .. Length(values) - 1 {
                    let value = values[idx];
                    if value > max {
                        set max = value;
                    }
                }
                return max;
            }

            operation MakeSequentialCircuit(
                first : Qubit[] => Unit,
                second : Qubit[] => Unit,
                targets : Int[]
            ) : Unit {
                if Length(targets) == 0 {
                    return ();
                } else {
                    let maxTarget = MaxInt(targets);
                    use qs = Qubit[1 + maxTarget];
                    ApplySequential(first, second, Subarray(targets, qs));
                }
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeSequentialCircuit", true),
            (".lambda", true),
            ("ApplyFirstStep", true),
            ("ApplySecondStep", true),
            ("ApplyThirdStep", true),
        ],
        caps,
    );

    let first_step = Value::Global(
        fir_id_for(pkg, items["ApplyFirstStep"]),
        FunctorApp::default(),
    );
    let second_step = Value::Global(
        fir_id_for(pkg, items["ApplySecondStep"]),
        FunctorApp::default(),
    );
    let third_step = Value::Global(
        fir_id_for(pkg, items["ApplyThirdStep"]),
        FunctorApp::default(),
    );
    let sequential = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![first_step, second_step].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(
        vec![
            sequential,
            third_step,
            Value::Array(vec![Value::Int(0), Value::Int(1)].into()),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["MakeSequentialCircuit"], &args, caps);
    for expected_gate in [
        "__quantum__qis__h__body",
        "__quantum__qis__x__body",
        "__quantum__qis__z__body",
    ] {
        assert!(
            qir.contains(expected_gate),
            "expected {expected_gate} in QIR:\n{qir}"
        );
    }
}

#[test]
fn chemistry_like_controlled_factory_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation PrepareIdentity(qs : Qubit[]) : Unit is Adj + Ctl {}

            operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

            function MakeControlledPrepSelPrepOp(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : (Qubit, Qubit[]) => Unit {
                (control, allQubits) => {
                    let systems = allQubits[0..numSystemQubits - 1];
                    let ancilla = allQubits[numSystemQubits...];
                    for _ in 0..power - 1 {
                        Controlled prepareOp([control], systems);
                        Controlled selectOp([control], (systems, ancilla));
                    }
                }
            }

            operation MakeControlledPrepSelPrepCircuit(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : Unit {
                use control = Qubit();
                use systems = Qubit[numSystemQubits + numAncillaQubits];
                let op = MakeControlledPrepSelPrepOp(
                    prepareOp,
                    selectOp,
                    numSystemQubits,
                    numAncillaQubits,
                    power
                );
                op(control, systems);
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeControlledPrepSelPrepCircuit", true),
            ("PrepareIdentity", true),
            ("SelectIdentity", true),
        ],
        caps,
    );

    let prepare = Value::Global(
        fir_id_for(pkg, items["PrepareIdentity"]),
        FunctorApp::default(),
    );
    let select = Value::Global(
        fir_id_for(pkg, items["SelectIdentity"]),
        FunctorApp::default(),
    );
    let args = Value::Tuple(
        vec![prepare, select, Value::Int(1), Value::Int(1), Value::Int(1)].into(),
        None,
    );

    let qir = callable_args_to_qir(
        &store,
        pkg,
        items["MakeControlledPrepSelPrepCircuit"],
        &args,
        caps,
    );
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_controlled_psp_wrapper_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation PrepareIdentity(qs : Qubit[]) : Unit is Adj + Ctl {}

            operation SelectIdentity(ancilla : Qubit[], systems : Qubit[]) : Unit is Adj + Ctl {}

            operation PrepSelPrep(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                systems : Qubit[],
                ancilla : Qubit[]
            ) : Unit is Adj + Ctl {
                body ... {
                    prepareOp(ancilla);
                    selectOp(ancilla, systems);
                    Adjoint prepareOp(ancilla);
                }
                adjoint auto;
                controlled (ctls, ...) {
                    prepareOp(ancilla);
                    Controlled selectOp(ctls, (ancilla, systems));
                    Adjoint prepareOp(ancilla);
                }
                controlled adjoint auto;
            }

            function MakeControlledPrepSelPrepOp(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : (Qubit, Qubit[]) => Unit {
                (control, allQubits) => {
                    let systems = allQubits[0..numSystemQubits - 1];
                    let ancilla = allQubits[numSystemQubits...];
                    for _ in 0..power - 1 {
                        Controlled PrepSelPrep([control], (prepareOp, selectOp, systems, ancilla));
                    }
                }
            }

            operation MakeControlledPrepSelPrepCircuit(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : Unit {
                use control = Qubit();
                use systems = Qubit[numSystemQubits + numAncillaQubits];
                let op = MakeControlledPrepSelPrepOp(
                    prepareOp,
                    selectOp,
                    numSystemQubits,
                    numAncillaQubits,
                    power
                );
                op(control, systems);
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeControlledPrepSelPrepCircuit", true),
            ("PrepareIdentity", true),
            ("SelectIdentity", true),
        ],
        caps,
    );

    let prepare = Value::Global(
        fir_id_for(pkg, items["PrepareIdentity"]),
        FunctorApp::default(),
    );
    let select = Value::Global(
        fir_id_for(pkg, items["SelectIdentity"]),
        FunctorApp::default(),
    );
    let args = Value::Tuple(
        vec![prepare, select, Value::Int(1), Value::Int(1), Value::Int(1)].into(),
        None,
    );

    let qir = callable_args_to_qir(
        &store,
        pkg,
        items["MakeControlledPrepSelPrepCircuit"],
        &args,
        caps,
    );
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_state_preparation_closure_with_empty_expansion_ops_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            struct StatePreparationParams {
                rowMap : Int[],
                stateVector : Double[],
                expansionOps : Int[][],
                numQubits : Int
            }

            operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
                if Length(params.expansionOps) != 0 {
                    X(qs[0]);
                }
            }

            function MakeStatePreparationOp(
                rowMap : Int[],
                stateVector : Double[],
                expansionOps : Int[][],
                numQubits : Int
            ) : Qubit[] => Unit is Adj + Ctl {
                ApplyStatePreparation(
                    new StatePreparationParams {
                        rowMap = rowMap,
                        stateVector = stateVector,
                        expansionOps = expansionOps,
                        numQubits = numQubits
                    },
                    _
                )
            }

            operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

            function MakeControlledPrepSelPrepOp(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : (Qubit, Qubit[]) => Unit {
                (control, allQubits) => {
                    let systems = allQubits[0..numSystemQubits - 1];
                    let ancilla = allQubits[numSystemQubits...];
                    for _ in 0..power - 1 {
                        Controlled prepareOp([control], systems);
                        Controlled selectOp([control], (systems, ancilla));
                    }
                }
            }

            operation MakeControlledPrepSelPrepCircuit(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : Unit {
                use control = Qubit();
                use systems = Qubit[numSystemQubits + numAncillaQubits];
                let op = MakeControlledPrepSelPrepOp(
                    prepareOp,
                    selectOp,
                    numSystemQubits,
                    numAncillaQubits,
                    power
                );
                op(control, systems);
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeControlledPrepSelPrepCircuit", true),
            ("ApplyStatePreparation", true),
            ("SelectIdentity", true),
        ],
        caps,
    );

    let state_params = Value::Tuple(
        vec![
            Value::Array(vec![Value::Int(0)].into()),
            Value::Array(vec![Value::Double(1.0), Value::Double(0.0)].into()),
            Value::Array(vec![].into()),
            Value::Int(1),
        ]
        .into(),
        None,
    );
    let prepare = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![state_params].into(),
        id: fir_id_for(pkg, items["ApplyStatePreparation"]),
        functor: FunctorApp::default(),
    }));
    let select = Value::Global(
        fir_id_for(pkg, items["SelectIdentity"]),
        FunctorApp::default(),
    );
    let args = Value::Tuple(
        vec![prepare, select, Value::Int(1), Value::Int(1), Value::Int(1)].into(),
        None,
    );

    let qir = callable_args_to_qir(
        &store,
        pkg,
        items["MakeControlledPrepSelPrepCircuit"],
        &args,
        caps,
    );
    // This test exercises the controlled-dispatch compile path for a closure whose
    // fixed captured struct must be threaded through `Controlled prepareOp`. Because
    // `expansionOps = []`, `ApplyStatePreparation`'s guarded `X(qs[0])` is never
    // emitted, so a dropped-vs-threaded capture produces identical (empty) gate
    // output here; the QIR cannot discriminate the capture-threading fix on its own.
    // The point of the case is that this controlled-dispatch shape compiles cleanly
    // to a valid entry point. The semantic assertion that the capture actually
    // reaches the controlled call lives at the FIR level in a companion test.
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_standard_qpe_callable_array_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            struct StandardPhaseEstimationParams {
                statePrep : Qubit[] => Unit,
                controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
                phaseQubitPrep : Qubit[] => Unit,
                numBits : Int,
                systems : Int[],
                numAncillaQubits : Int
            }

            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }

            operation PreparePhase(ancillas : Qubit[]) : Unit {
                for q in ancillas {
                    H(q);
                }
            }

            operation ControlledFirst(control : Qubit, targets : Qubit[]) : Unit {
                Controlled X([control], targets[0]);
            }

            operation ControlledSecond(control : Qubit, targets : Qubit[]) : Unit {
                Controlled Z([control], targets[0]);
            }

            operation RunStandardQPE(params : StandardPhaseEstimationParams) : Result[] {
                use qs = Qubit[params.numBits + Length(params.systems) + params.numAncillaQubits];
                let ancillas = qs[0..params.numBits - 1];
                let allTargets = qs[params.numBits...];

                params.statePrep(allTargets);
                params.phaseQubitPrep(ancillas);

                for ancillaIdx in 0..params.numBits - 1 {
                    params.controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                }

                ResetAll(allTargets);
                mutable results = [Zero, size = params.numBits];
                for idx in 0..params.numBits - 1 {
                    set results w/= idx <- MResetZ(ancillas[idx]);
                }
                return results;
            }

            operation MakeStandardQPECircuit(
                statePrep : Qubit[] => Unit,
                controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
                numBits : Int,
                systems : Int[],
                phaseQubitPrep : Qubit[] => Unit,
                numAncillaQubits : Int
            ) : Result[] {
                return RunStandardQPE(new StandardPhaseEstimationParams {
                    statePrep = statePrep,
                    controlledUnitary = controlledUnitary,
                    phaseQubitPrep = phaseQubitPrep,
                    numBits = numBits,
                    systems = systems,
                    numAncillaQubits = numAncillaQubits
                });
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeStandardQPECircuit", true),
            ("PrepareSystems", true),
            ("PreparePhase", true),
            ("ControlledFirst", true),
            ("ControlledSecond", true),
        ],
        caps,
    );

    let state_prep = Value::Global(
        fir_id_for(pkg, items["PrepareSystems"]),
        FunctorApp::default(),
    );
    let controlled_unitaries = Value::Array(
        vec![
            Value::Global(
                fir_id_for(pkg, items["ControlledFirst"]),
                FunctorApp::default(),
            ),
            Value::Global(
                fir_id_for(pkg, items["ControlledSecond"]),
                FunctorApp::default(),
            ),
        ]
        .into(),
    );
    let phase_prep = Value::Global(
        fir_id_for(pkg, items["PreparePhase"]),
        FunctorApp::default(),
    );
    let args = Value::Tuple(
        vec![
            state_prep,
            controlled_unitaries,
            Value::Int(2),
            Value::Array(vec![Value::Int(0)].into()),
            phase_prep,
            Value::Int(0),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["MakeStandardQPECircuit"], &args, caps);
    // The dispatch loop calls controlledUnitary[0] then [1]. Index 0 is
    // `ControlledFirst` (Controlled X -> cx) and index 1 is `ControlledSecond`
    // (Controlled Z -> cz). Both act on the single system/target qubit (index 2);
    // there is no QFT in this variant, so the controlled gates are exactly the two
    // dispatch calls. Anchoring on the qubit-2 target isolates the expected
    // per-index dispatch sequence [cx, cz] from the state-prep `X`.
    let dispatch_gates: Vec<&str> = qir
        .lines()
        .filter(|line| line.trim_end().ends_with("inttoptr (i64 2 to %Qubit*))"))
        .filter_map(|line| {
            if line.contains("__quantum__qis__cx__body") {
                Some("cx")
            } else if line.contains("__quantum__qis__cz__body") {
                Some("cz")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        dispatch_gates,
        ["cx", "cz"],
        "expected ordered per-index dispatch gates [cx, cz]; got {dispatch_gates:?}:\n{qir}"
    );
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_standard_qpe_callable_array_of_closures_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Arrays.Subarray;
            import Std.Canon.ApplyQFT;

            struct ControlledParams {
                angle : Double,
                repetitions : Int
            }

            struct StatePrepParams {
                rowMap : Int[],
                stateVector : Double[],
                expansionOps : Int[][],
                numQubits : Int
            }

            struct StandardPhaseEstimationParams {
                statePrep : Qubit[] => Unit,
                controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
                phaseQubitPrep : Qubit[] => Unit,
                numBits : Int,
                ancillas : Int[],
                systems : Int[],
                numAncillaQubits : Int
            }

            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }

            operation StatePreparation(params : StatePrepParams, systems : Qubit[]) : Unit {
                if Length(params.stateVector) > 0 {
                    X(systems[0]);
                }
            }

            function MakeStatePreparationOp(params : StatePrepParams) : Qubit[] => Unit {
                StatePreparation(params, _)
            }

            operation PreparePhase(ancillas : Qubit[]) : Unit {
                for q in ancillas {
                    H(q);
                }
            }

            function MakePreparePhaseOp() : Qubit[] => Unit {
                PreparePhase(_)
            }

            operation RepControlled(params : ControlledParams, control : Qubit, targets : Qubit[]) : Unit {
                for _ in 1..params.repetitions {
                    if params.angle > 0.0 {
                        Controlled X([control], targets[0]);
                    } else {
                        Controlled Z([control], targets[0]);
                    }
                }
            }

            operation RunStandardQPE(params : StandardPhaseEstimationParams) : Result[] {
                let totalQubits = params.numBits + Length(params.systems) + params.numAncillaQubits;
                use qs = Qubit[totalQubits];
                let ancillas = Subarray(params.ancillas, qs);
                let systems = Subarray(params.systems, qs);
                let unitaryAncillas = if params.numAncillaQubits == 0 {
                    []
                } else {
                    qs[params.numBits + Length(params.systems)..Length(qs) - 1]
                };
                let allTargets = systems + unitaryAncillas;

                params.statePrep(systems);
                params.phaseQubitPrep(ancillas);

                for ancillaIdx in 0..params.numBits - 1 {
                    params.controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                }

                Adjoint ApplyQFT(ancillas);

                ResetAll(allTargets);
                mutable results = [Zero, size = params.numBits];
                for idx in 0..params.numBits - 1 {
                    set results w/= idx <- MResetZ(ancillas[idx]);
                }
                return results;
            }

            operation MakeStandardQPECircuit(
                statePrep : Qubit[] => Unit,
                controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
                numBits : Int,
                ancillas : Int[],
                systems : Int[],
                phaseQubitPrep : Qubit[] => Unit,
                numAncillaQubits : Int
            ) : Result[] {
                return RunStandardQPE(new StandardPhaseEstimationParams {
                    statePrep = statePrep,
                    controlledUnitary = controlledUnitary,
                    phaseQubitPrep = phaseQubitPrep,
                    numBits = numBits,
                    ancillas = ancillas,
                    systems = systems,
                    numAncillaQubits = numAncillaQubits
                });
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeStandardQPECircuit", true),
            ("PrepareSystems", true),
            ("StatePreparation", true),
            ("PreparePhase", true),
            ("RepControlled", true),
            ("ControlledParams", false),
            ("StatePrepParams", false),
        ],
        caps,
    );

    let state_prep_params = Value::Tuple(
        vec![
            Value::Array(vec![Value::Int(1), Value::Int(0)].into()),
            Value::Array(vec![Value::Double(0.6), Value::Double(0.8)].into()),
            Value::Array(vec![Value::Array(vec![Value::Int(0)].into())].into()),
            Value::Int(2),
        ]
        .into(),
        None,
    );
    let state_prep = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![state_prep_params].into(),
        id: fir_id_for(pkg, items["StatePreparation"]),
        functor: FunctorApp::default(),
    }));
    let rep_controlled_id = fir_id_for(pkg, items["RepControlled"]);
    let controlled_unitaries = Value::Array(
        (0..6)
            .map(|idx| {
                let params = Value::Tuple(
                    vec![
                        Value::Double(if idx % 2 == 0 { 1.0 } else { -1.0 }),
                        Value::Int(1),
                    ]
                    .into(),
                    None,
                );
                Value::Closure(Box::new(qsc_eval::val::Closure {
                    fixed_args: vec![params].into(),
                    id: rep_controlled_id,
                    functor: FunctorApp::default(),
                }))
            })
            .collect::<Vec<_>>()
            .into(),
    );
    let phase_prep = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![].into(),
        id: fir_id_for(pkg, items["PreparePhase"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(
        vec![
            state_prep,
            controlled_unitaries,
            Value::Int(6),
            Value::Array((0..6).map(Value::Int).collect::<Vec<_>>().into()),
            Value::Array(vec![Value::Int(6)].into()),
            phase_prep,
            Value::Int(0),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["MakeStandardQPECircuit"], &args, caps);
    // The dispatch loop calls controlledUnitary[0..5] in order. Each element is a
    // closure over `RepControlled` capturing `angle`: even indices capture 1.0
    // (angle > 0.0 -> Controlled X -> cx) and odd indices capture -1.0
    // (Controlled Z -> cz), so the expected dispatch sequence alternates
    // cx, cz, cx, cz, cx, cz. All six dispatch gates act on the single system
    // qubit (index 6). The
    // `Adjoint ApplyQFT(ancillas)` that follows emits many cx among the ancilla
    // qubits (indices 0..5) but never targets qubit 6 and emits no cz, so
    // anchoring on the qubit-6 target isolates the dispatch subsequence.
    let dispatch_gates: Vec<&str> = qir
        .lines()
        .filter(|line| line.trim_end().ends_with("inttoptr (i64 6 to %Qubit*))"))
        .filter_map(|line| {
            if line.contains("__quantum__qis__cx__body") {
                Some("cx")
            } else if line.contains("__quantum__qis__cz__body") {
                Some("cz")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        dispatch_gates,
        ["cx", "cz", "cx", "cz", "cx", "cz"],
        "expected alternating per-index dispatch gates; got {dispatch_gates:?}:\n{qir}"
    );
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_iqpe_params_struct_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Arrays.Subarray;

            struct IterativePhaseEstimationParams {
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            }

            operation PrepareSystems(systems : Qubit[]) : Unit {}

            operation RepControlledUnitary(control : Qubit, targets : Qubit[]) : Unit {}

            operation RunIQPE(params : IterativePhaseEstimationParams) : Result[] {
                use qs = Qubit[Length(params.systems) + 1 + params.numAncillaQubits];
                let phaseQubit = qs[params.phaseQubit];
                let systems = Subarray(params.systems, qs);
                let ancillas = if params.numAncillaQubits == 0 {
                    []
                } else {
                    qs[1 + Length(params.systems)..Length(qs) - 1]
                };
                let allTargets = systems + ancillas;

                params.statePrep(systems);

                within {
                    H(phaseQubit);
                } apply {
                    Rz(params.accumulatePhase, phaseQubit);
                    params.repControlledUnitary(phaseQubit, allTargets);
                }
                ResetAll(allTargets);
                return [MResetZ(phaseQubit)];
            }

            operation MakeIQPECircuit(
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            ) : Result[] {
                return RunIQPE(new IterativePhaseEstimationParams {
                    statePrep = statePrep,
                    repControlledUnitary = repControlledUnitary,
                    accumulatePhase = accumulatePhase,
                    phaseQubit = phaseQubit,
                    systems = systems,
                    numAncillaQubits = numAncillaQubits
                });
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeIQPECircuit", true),
            ("PrepareSystems", true),
            ("RepControlledUnitary", true),
        ],
        caps,
    );

    let state_prep = Value::Global(
        fir_id_for(pkg, items["PrepareSystems"]),
        FunctorApp::default(),
    );
    let rep_controlled_unitary = Value::Global(
        fir_id_for(pkg, items["RepControlledUnitary"]),
        FunctorApp::default(),
    );
    let args = Value::Tuple(
        vec![
            state_prep,
            rep_controlled_unitary,
            Value::Double(0.25),
            Value::Int(0),
            Value::Array(vec![Value::Int(1)].into()),
            Value::Int(1),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["MakeIQPECircuit"], &args, caps);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_iqpe_with_controlled_psp_closure_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            struct IterativePhaseEstimationParams {
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            }

            operation PrepareIdentity(qs : Qubit[]) : Unit is Adj + Ctl {}

            operation SelectIdentity(ancilla : Qubit[], systems : Qubit[]) : Unit is Adj + Ctl {}

            operation PrepSelPrep(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                systems : Qubit[],
                ancilla : Qubit[]
            ) : Unit is Adj + Ctl {
                body ... {
                    prepareOp(ancilla);
                    selectOp(ancilla, systems);
                    Adjoint prepareOp(ancilla);
                }
                adjoint auto;
                controlled (ctls, ...) {
                    prepareOp(ancilla);
                    Controlled selectOp(ctls, (ancilla, systems));
                    Adjoint prepareOp(ancilla);
                }
                controlled adjoint auto;
            }

            function MakeControlledPrepSelPrepOp(
                prepareOp : Qubit[] => Unit is Adj + Ctl,
                selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
                numSystemQubits : Int,
                numAncillaQubits : Int,
                power : Int
            ) : (Qubit, Qubit[]) => Unit {
                (control, allQubits) => {
                    let systems = allQubits[0..numSystemQubits - 1];
                    let ancilla = allQubits[numSystemQubits...];
                    for _ in 0..power - 1 {
                        Controlled PrepSelPrep([control], (prepareOp, selectOp, systems, ancilla));
                    }
                }
            }

            operation RunIQPE(params : IterativePhaseEstimationParams) : Result[] {
                use qs = Qubit[Length(params.systems) + 1 + params.numAncillaQubits];
                let phaseQubit = qs[params.phaseQubit];
                let allTargets = qs[1...];

                params.statePrep(allTargets);

                within {
                    H(phaseQubit);
                } apply {
                    Rz(params.accumulatePhase, phaseQubit);
                    params.repControlledUnitary(phaseQubit, allTargets);
                }
                ResetAll(allTargets);
                return [MResetZ(phaseQubit)];
            }

            operation MakeIQPECircuit(
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            ) : Result[] {
                return RunIQPE(new IterativePhaseEstimationParams {
                    statePrep = statePrep,
                    repControlledUnitary = repControlledUnitary,
                    accumulatePhase = accumulatePhase,
                    phaseQubit = phaseQubit,
                    systems = systems,
                    numAncillaQubits = numAncillaQubits
                });
            }
        }
    "#};
    let caps = Profile::Base.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("MakeIQPECircuit", true),
            ("PrepareIdentity", true),
            ("SelectIdentity", true),
            (".lambda", true),
        ],
        caps,
    );

    let prepare = Value::Global(
        fir_id_for(pkg, items["PrepareIdentity"]),
        FunctorApp::default(),
    );
    let select = Value::Global(
        fir_id_for(pkg, items["SelectIdentity"]),
        FunctorApp::default(),
    );
    let rep_controlled_unitary = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![prepare.clone(), select, Value::Int(1), Value::Int(1)].into(),
        id: fir_id_for(pkg, items[".lambda"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(
        vec![
            prepare,
            rep_controlled_unitary,
            Value::Double(0.25),
            Value::Int(0),
            Value::Array(vec![Value::Int(1)].into()),
            Value::Int(1),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["MakeIQPECircuit"], &args, caps);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
}

#[test]
fn chemistry_like_iqpe_with_udt_capture_closure_generates_base_profile_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            struct ControlledParams {
                pauliExponents : Pauli[][],
                pauliCoefficients : Double[],
                repetitions : Int
            }

            struct IterativePhaseEstimationParams {
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            }

            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }

            operation RepControlledUnitary(params : ControlledParams, control : Qubit, systems : Qubit[]) : Unit {
                for _ in 1..params.repetitions {
                    for idx in 0..Length(params.pauliExponents) - 1 {
                        if params.pauliCoefficients[idx] > 0.0 {
                            Controlled X([control], systems[0]);
                        } else {
                            Controlled Z([control], systems[0]);
                        }
                    }
                }
            }

            operation RunIQPE(params : IterativePhaseEstimationParams) : Result[] {
                use qs = Qubit[Length(params.systems) + 1 + params.numAncillaQubits];
                let phaseQubit = qs[params.phaseQubit];
                let allTargets = qs[1...];

                params.statePrep(allTargets);

                within {
                    H(phaseQubit);
                } apply {
                    Rz(params.accumulatePhase, phaseQubit);
                    params.repControlledUnitary(phaseQubit, allTargets);
                }
                ResetAll(allTargets);
                return [MResetZ(phaseQubit)];
            }

            operation MakeIQPECircuit(
                statePrep : Qubit[] => Unit,
                repControlledUnitary : (Qubit, Qubit[]) => Unit,
                accumulatePhase : Double,
                phaseQubit : Int,
                systems : Int[],
                numAncillaQubits : Int
            ) : Result[] {
                return RunIQPE(new IterativePhaseEstimationParams {
                    statePrep = statePrep,
                    repControlledUnitary = repControlledUnitary,
                    accumulatePhase = accumulatePhase,
                    phaseQubit = phaseQubit,
                    systems = systems,
                    numAncillaQubits = numAncillaQubits
                });
            }
        }
    "#};
    let caps = Profile::Base.into();
    let mut interpreter = interpreter_with_capabilities(caps);
    eval_fragments(&mut interpreter, source);
    let target = eval_fragments(&mut interpreter, "Test.MakeIQPECircuit");
    let args = eval_fragments(
        &mut interpreter,
        r#"
            {
                let params = new Test.ControlledParams {
                    pauliExponents = [[PauliX]],
                    pauliCoefficients = [1.0],
                    repetitions = 1
                };
                let unitary = (control, systems) =>
                    Test.RepControlledUnitary(params, control, systems);
                (Test.PrepareSystems, unitary, 0.0, 0, [1], 0)
            }
        "#,
    );

    let qir = interpreter
        .qirgen_from_callable(&target, args)
        .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)));
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__cx__body"),
        "expected threaded Controlled X capture in QIR:\n{qir}"
    );
}

// `break`/`continue` are desugared to flags in HIR, so codegen sees only
// ordinary `while` loops. These tests confirm the desugared programs lower all
// the way to QIR/RIR with no residual early-exit constructs, and that
// classically-conditioned `break`/`continue` needs no capability beyond the
// Base profile.

#[test]
fn break_and_continue_in_for_loop_generates_qir_on_base_profile() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..10 {
                    if i == 5 {
                        break;
                    }
                    if i % 2 == 0 {
                        continue;
                    }
                    X(q);
                }
                Reset(q);
            }
        }
    "#;

    // Classical conditions keep the loop static, so the most restrictive Base
    // profile suffices: no new capability is required for `break`/`continue`.
    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());

    // `continue` skips even i and `break` stops at i == 5, so X is applied for
    // i = 1 and i = 3 only. Match call sites, not the trailing `declare`.
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        2,
        "expected exactly two X gates from odd i < 5:\n{qir}"
    );
}

#[test]
fn while_and_repeat_with_break_continue_generate_qir_on_base_profile() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable i = 0;
                while i < 100 {
                    set i += 1;
                    if i == 3 {
                        break;
                    }
                    X(q);
                }
                mutable j = 0;
                repeat {
                    set j += 1;
                    if j % 2 == 0 {
                        continue;
                    }
                    H(q);
                } until j >= 5;
                Reset(q);
            }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());

    // while: X for i = 1, 2 then break at i == 3 -> 2 X gates.
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        2,
        "expected two X gates before the while `break`:\n{qir}"
    );
    // repeat: `continue` skips even j, so H fires for odd j = 1, 3, 5 -> 3 H gates.
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        3,
        "expected three H gates for odd j via repeat `continue`:\n{qir}"
    );
}

#[test]
fn return_and_break_in_loop_composes_with_return_unify() {
    // Check that `return` and `break` play nicely together in the same loop.
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable total = 0;
                for i in 0..20 {
                    if i == 10 {
                        break;
                    }
                    X(q);
                    set total += i;
                    if total > 15 {
                        return total;
                    }
                }
                Reset(q);
                total
            }
        }
    "#;

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    // The early `return` fires at i == 6, where total = 21 > 15, after X on i = 0..6.
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        7,
        "expected seven X gates before the early return:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__rt__int_record_output(i64 21"),
        "expected the composed return value 21 to be recorded:\n{qir}"
    );
}

#[test]
fn break_continue_loops_lower_to_rir_without_early_exit() {
    // Exercises the RIR lowering path, a superset of the checks QIR relies on,
    // for `for` and `repeat` loops carrying `break`/`continue`, confirming no residual
    // `break`/`continue`/early-exit construct survives into RIR.
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..6 {
                    if i == 4 {
                        break;
                    }
                    if i == 1 {
                        continue;
                    }
                    X(q);
                }
                mutable k = 0;
                repeat {
                    set k += 1;
                    if k == 2 {
                        continue;
                    }
                    if k == 5 {
                        break;
                    }
                    Y(q);
                } until k >= 8;
                Reset(q);
            }
        }
    "#;

    let rir = compile_source_to_rir(source, TargetCapabilityFlags::empty());
    let rir_text = rir.join("\n");
    // Both loop bodies' quantum ops survive into the RIR callable table, and the
    // fully-desugared program lowers to a clean single-return body; no residual
    // early-exit construct could survive, because the lowerer panics on any
    // leftover `break`/`continue` and `return_unify` asserts no leftover `Return`.
    assert!(
        rir_text.contains("name: __quantum__qis__x__body"),
        "expected the X gate callable to survive into RIR:\n{rir_text}"
    );
    assert!(
        rir_text.contains("name: __quantum__qis__y__body"),
        "expected the Y gate callable to survive into RIR:\n{rir_text}"
    );
    for rir_form in &rir {
        assert_eq!(
            rir_form.matches("Return Integer(0)").count(),
            1,
            "expected each desugared RIR form to contain exactly one return:\n{rir_form}"
        );
    }
}

#[test]
fn bare_operand_break_lowers_to_qir_and_skips_eager_consumer() {
    // A bare-operand `break` buried in a call argument such as `Sink(q, break)` is
    // hoisted and lowered without evaluating the eager consumer. `Sink` applies
    // an `H` gate, so no H call in the QIR proves the break fires first.
    let source = r#"
        namespace Test {
            operation Sink(q : Qubit, x : Int) : Int { H(q); x }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..10 {
                    if i == 3 {
                        let _ = Sink(q, break);
                    }
                    X(q);
                }
                Reset(q);
            }
        }
    "#;

    // Classical loop and condition keep the program static, so the Base profile
    // suffices. `X(q)` runs for i = 0, 1, 2 after the false guard; on i == 3 the break
    // fires first, so the trailing `X(q)` is skipped and the loop exits. That
    // gives exactly three X gates, and no H gate because `Sink` never runs.
    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        3,
        "expected exactly three X gates before the operand-position break:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__h__body"),
        "expected no H gate: the eager consumer must not run on the break path:\n{qir}"
    );
}

#[test]
fn bare_operand_break_preserves_prior_argument_effects_and_skips_later_arguments() {
    let source = r#"
        namespace Test {
            operation Foo(first : Unit, second : Unit, third : Unit) : Unit {}
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for _ in 0..10 {
                    Foo(H(q), break, Y(q));
                }
                Reset(q);
            }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        1,
        "expected H to run before the operand-position break:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__y__body"),
        "expected Y after the operand-position break to be skipped:\n{qir}"
    );
}

#[test]
fn operand_break_in_qubit_allocation_skips_following_controlled_call_in_qir() {
    let source = r#"
        namespace Test {
            @EntryPoint(Adaptive)
            operation Main() : Result {
                use qq = Qubit[2];
                repeat {
                    H(qq[0]);
                    use rr = Qubit[break];
                    Controlled X([qq[0]], qq[1]);
                } until (M(qq[1]) == One)
                fixup {
                    ResetAll(qq);
                }
                let r = M(qq[0]);
                ResetAll(qq);
                r
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        1,
        "expected H before the allocation-size break:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__cx__body"),
        "expected the controlled X after the first break to be skipped:\n{qir}"
    );
}

#[test]
fn operand_break_in_controlled_call_skips_consumer_and_following_statement_in_qir() {
    let source = r#"
        namespace Test {
            @EntryPoint(Adaptive)
            operation Main() : Result {
                use qq = Qubit[2];
                repeat {
                    H(qq[0]);
                    Controlled X(break);
                    Y(qq[0]);
                } until (M(qq[1]) == One)
                fixup {
                    ResetAll(qq);
                }
                let r = M(qq[0]);
                ResetAll(qq);
                r
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        1,
        "expected H before the controlled-call operand break:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__cx__body"),
        "expected the controlled X consumer to be skipped:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__y__body"),
        "expected the statement after the controlled-call operand break to be skipped:\n{qir}"
    );
}

#[test]
fn operand_break_in_index_skips_following_controlled_continue() {
    let source = r#"
        namespace Test {
            @EntryPoint(Adaptive)
            operation Main() : Result {
                use qq = Qubit[2];
                repeat {
                    H(qq[break]);
                    Controlled X(continue, qq[1]);
                } until (M(qq[1]) == One)
                fixup {
                    ResetAll(qq);
                }
                let r = M(qq[0]);
                ResetAll(qq);
                r
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    assert!(
        !qir.contains("call void @__quantum__qis__h__body"),
        "the index break should prevent H from running:\n{qir}"
    );
    // The `continue` controls operand defaults to `[]`. If its consumer guard
    // failed, `Controlled X([], qq[1])` would emit a plain X rather than a CX.
    assert!(
        !qir.contains("call void @__quantum__qis__x__body"),
        "the controlled X after the break should be unreachable:\n{qir}"
    );
}

#[test]
fn operand_continue_in_controlled_call_skips_consumer_and_following_statement_in_qir() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use qq = Qubit[2];
                mutable i = 0;
                repeat {
                    set i = i + 1;
                    H(qq[0]);
                    Controlled X(continue, qq[1]);
                    Y(qq[0]);
                } until i >= 1;
                ResetAll(qq);
            }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        1,
        "expected H before the controlled-call operand continue:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__x__body"),
        "expected the empty-controls X consumer after continue to be skipped:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__y__body"),
        "expected the statement after the controlled-call operand continue to be skipped:\n{qir}"
    );
}

#[test]
fn array_with_dynamic_contents_passed_as_argument_and_dynamically_indexed_emits_locally_constant_array()
 {
    let source = indoc::indoc! {r#"
        namespace Test {
            function ValueAt(arr : Int[], idx : Int) : Int {
                arr[idx]
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let arr = [if M(q) == One { 1 } else { 0 }, if M(q) == One { 1 } else { 0 }];
                ValueAt(arr, if M(q) == One { 1 } else { 0 })
            }
        }
    "#};

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i64
          %var_7 = alloca i64
          %var_10 = alloca i64
          %var_11 = alloca i64
          %var_12 = alloca [2 x i64]
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          store i64 1, ptr %var_2
          br label %block_3
        block_2:
          store i64 0, ptr %var_2
          br label %block_3
        block_3:
          %var_15 = load i64, ptr %var_2
          store i64 %var_15, ptr %var_3
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_4, label %block_4, label %block_5
        block_4:
          store i64 1, ptr %var_6
          br label %block_6
        block_5:
          store i64 0, ptr %var_6
          br label %block_6
        block_6:
          %var_18 = load i64, ptr %var_6
          store i64 %var_18, ptr %var_7
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          %var_8 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          br i1 %var_8, label %block_7, label %block_8
        block_7:
          store i64 1, ptr %var_10
          br label %block_9
        block_8:
          store i64 0, ptr %var_10
          br label %block_9
        block_9:
          %var_21 = load i64, ptr %var_10
          store i64 %var_21, ptr %var_11
          %var_23 = load i64, ptr %var_3
          %var_24 = load i64, ptr %var_7
          %var_12_0 = getelementptr [2 x i64], ptr %var_12, i64 0, i64 0
          store i64 %var_23, ptr %var_12_0
          %var_12_1 = getelementptr [2 x i64], ptr %var_12, i64 0, i64 1
          store i64 %var_24, ptr %var_12_1
          %var_26 = load i64, ptr %var_11
          %var_27_offset_chk = icmp slt i64 %var_26, 0
          %var_27_offset = select i1 %var_27_offset_chk, i64 1, i64 0
          %var_27 = getelementptr [2 x i64], ptr %var_12, i64 %var_27_offset, i64 %var_26
          %var_13 = load i64, ptr %var_27
          call void @__quantum__rt__int_record_output(i64 %var_13, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="3" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn array_with_dynamic_contents_passed_as_argument_with_static_index_does_not_emit_locally_constant_array()
 {
    let source = indoc::indoc! {r#"
        namespace Test {
            function ValueAt(arr : Int[], idx : Int) : Int {
                arr[idx]
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let arr = [if M(q) == One { 1 } else { 0 }, if M(q) == One { 1 } else { 0 }];
                ValueAt(arr, 0)
            }
        }
    "#};

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i64
          %var_7 = alloca i64
          %var_9 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          store i64 1, ptr %var_2
          br label %block_3
        block_2:
          store i64 0, ptr %var_2
          br label %block_3
        block_3:
          %var_11 = load i64, ptr %var_2
          store i64 %var_11, ptr %var_3
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_4, label %block_4, label %block_5
        block_4:
          store i64 1, ptr %var_6
          br label %block_6
        block_5:
          store i64 0, ptr %var_6
          br label %block_6
        block_6:
          %var_14 = load i64, ptr %var_6
          store i64 %var_14, ptr %var_7
          %var_16 = load i64, ptr %var_3
          store i64 %var_16, ptr %var_9
          %var_18 = load i64, ptr %var_9
          call void @__quantum__rt__int_record_output(i64 %var_18, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn nested_array_with_dynamic_contents_passed_as_argument_and_dynamically_indexed_emits_multiple_locally_constant_array()
 {
    let source = indoc::indoc! {r#"
        namespace Test {
            function ValueAt(arr : Int[][], idx : Int) : Int[] {
                [arr[0][idx], arr[1][idx]]
            }
            @EntryPoint()
            operation Main() : Int[] {
                use q = Qubit();
                let arr = [[if M(q) == One { 1 } else { 0 }, if M(q) == One { 1 } else { 0 }],
                    [if M(q) == One { 1 } else { 0 }, if M(q) == One { 1 } else { 0 }, if M(q) == One { 1 } else { 0 }]];
                ValueAt(arr, if M(q) == One { 1 } else { 0 })
            }
        }
    "#};

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0i\00"
        @2 = internal constant [6 x i8] c"2_a1i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i64
          %var_7 = alloca i64
          %var_10 = alloca i64
          %var_11 = alloca i64
          %var_14 = alloca i64
          %var_15 = alloca i64
          %var_18 = alloca i64
          %var_19 = alloca i64
          %var_22 = alloca i64
          %var_23 = alloca i64
          %var_24 = alloca [2 x i64]
          %var_25 = alloca [3 x i64]
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          store i64 1, ptr %var_2
          br label %block_3
        block_2:
          store i64 0, ptr %var_2
          br label %block_3
        block_3:
          %var_30 = load i64, ptr %var_2
          store i64 %var_30, ptr %var_3
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_4, label %block_4, label %block_5
        block_4:
          store i64 1, ptr %var_6
          br label %block_6
        block_5:
          store i64 0, ptr %var_6
          br label %block_6
        block_6:
          %var_33 = load i64, ptr %var_6
          store i64 %var_33, ptr %var_7
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          %var_8 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          br i1 %var_8, label %block_7, label %block_8
        block_7:
          store i64 1, ptr %var_10
          br label %block_9
        block_8:
          store i64 0, ptr %var_10
          br label %block_9
        block_9:
          %var_36 = load i64, ptr %var_10
          store i64 %var_36, ptr %var_11
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
          %var_12 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
          br i1 %var_12, label %block_10, label %block_11
        block_10:
          store i64 1, ptr %var_14
          br label %block_12
        block_11:
          store i64 0, ptr %var_14
          br label %block_12
        block_12:
          %var_39 = load i64, ptr %var_14
          store i64 %var_39, ptr %var_15
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
          %var_16 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
          br i1 %var_16, label %block_13, label %block_14
        block_13:
          store i64 1, ptr %var_18
          br label %block_15
        block_14:
          store i64 0, ptr %var_18
          br label %block_15
        block_15:
          %var_42 = load i64, ptr %var_18
          store i64 %var_42, ptr %var_19
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 5 to ptr))
          %var_20 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
          br i1 %var_20, label %block_16, label %block_17
        block_16:
          store i64 1, ptr %var_22
          br label %block_18
        block_17:
          store i64 0, ptr %var_22
          br label %block_18
        block_18:
          %var_45 = load i64, ptr %var_22
          store i64 %var_45, ptr %var_23
          %var_47 = load i64, ptr %var_3
          %var_48 = load i64, ptr %var_7
          %var_24_0 = getelementptr [2 x i64], ptr %var_24, i64 0, i64 0
          store i64 %var_47, ptr %var_24_0
          %var_24_1 = getelementptr [2 x i64], ptr %var_24, i64 0, i64 1
          store i64 %var_48, ptr %var_24_1
          %var_50 = load i64, ptr %var_11
          %var_51 = load i64, ptr %var_15
          %var_52 = load i64, ptr %var_19
          %var_25_0 = getelementptr [3 x i64], ptr %var_25, i64 0, i64 0
          store i64 %var_50, ptr %var_25_0
          %var_25_1 = getelementptr [3 x i64], ptr %var_25, i64 0, i64 1
          store i64 %var_51, ptr %var_25_1
          %var_25_2 = getelementptr [3 x i64], ptr %var_25, i64 0, i64 2
          store i64 %var_52, ptr %var_25_2
          %var_54 = load i64, ptr %var_23
          %var_55_offset_chk = icmp slt i64 %var_54, 0
          %var_55_offset = select i1 %var_55_offset_chk, i64 1, i64 0
          %var_55 = getelementptr [2 x i64], ptr %var_24, i64 %var_55_offset, i64 %var_54
          %var_26 = load i64, ptr %var_55
          %var_56_offset_chk = icmp slt i64 %var_54, 0
          %var_56_offset = select i1 %var_56_offset_chk, i64 1, i64 0
          %var_56 = getelementptr [3 x i64], ptr %var_25, i64 %var_56_offset, i64 %var_54
          %var_28 = load i64, ptr %var_56
          call void @__quantum__rt__array_record_output(i64 2, ptr @0)
          call void @__quantum__rt__int_record_output(i64 %var_26, ptr @1)
          call void @__quantum__rt__int_record_output(i64 %var_28, ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="6" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn array_entry_used_as_argument_to_callable_with_inner_branch() {
    // Validates QIR for specific bug: loading a value from an array
    // as part of a call expression (without an intermediate variable binding)
    // with an operation that includes both a qubit allocation and an inner branch
    // The fix ensures that the value is loaded from the computed offset again,
    // rather than used directly.
    let source = indoc::indoc! {r#"
        operation Foo(q : Qubit) : Unit {
            use a = Qubit();
            if M(a) == One {
                X(q);
            }
        }
        operation Main() : Unit {
            use qs = Qubit[1];
            for i in 0..0 {
                Foo(qs[i]);
            }
        }
    "#};

    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @array0 = internal constant [1 x ptr] [ptr inttoptr (i64 0 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_2 = alloca i64
          %var_5 = alloca i1
          %var_6 = alloca i64
          %var_11 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_1
          br label %block_1
        block_1:
          %var_14 = load i64, ptr %var_1
          store i64 %var_14, ptr %var_2
          %var_16 = load i64, ptr %var_2
          %var_3 = icmp sle i64 %var_16, 0
          store i1 true, ptr %var_5
          br i1 %var_3, label %block_2, label %block_3
        block_2:
          %var_19 = load i1, ptr %var_5
          br i1 %var_19, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_5
          br label %block_2
        block_4:
          %var_20 = load i64, ptr %var_1
          store i64 %var_20, ptr %var_6
          %var_22 = load i64, ptr %var_6
          %var_23_offset_chk = icmp slt i64 %var_22, 0
          %var_23_offset = select i1 %var_23_offset_chk, i64 1, i64 0
          %var_23 = getelementptr [1 x ptr], ptr @array0, i64 %var_23_offset, i64 %var_22
          %var_7 = load ptr, ptr %var_23
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_8 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_8, label %block_6, label %block_7
        block_5:
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        block_6:
          call void @X(ptr %var_7)
          br label %block_7
        block_7:
          %var_24 = load i64, ptr %var_1
          store i64 %var_24, ptr %var_11
          %var_26 = load i64, ptr %var_11
          %var_12 = add i64 %var_26, 1
          store i64 %var_12, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        define internal void @X(ptr %var_10) {
        block_8:
          call void @__quantum__qis__x__body(ptr %var_10)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn mutable_fixed_size_arrays_constant_update_after_dynamic_update() {
    let source = indoc::indoc! {r#"
        operation Main() : Int[] {
            use qubit = Qubit();
            mutable values = [1, 2];
            values[0] = MResetZ(qubit) == Zero ? 9 | 10;
            values[1] = 3;
            values
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0i\00"
        @2 = internal constant [6 x i8] c"2_a1i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %var_1 = icmp eq i1 %var_0, false
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 9, ptr %var_2
          br label %block_3
        block_2:
          store i64 10, ptr %var_2
          br label %block_3
        block_3:
          %var_5 = load i64, ptr %var_2
          store i64 %var_5, ptr %var_3
          call void @__quantum__rt__array_record_output(i64 2, ptr @0)
          %var_7 = load i64, ptr %var_3
          call void @__quantum__rt__int_record_output(i64 %var_7, ptr @1)
          call void @__quantum__rt__int_record_output(i64 3, ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn mutable_fixed_size_arrays_snapshot_reassignment_uses_array_copy() {
    let source = indoc::indoc! {r#"
        operation Main() : Int[] {
            use qubit = Qubit();
            mutable values = [1, 2];
            values[0] = MResetZ(qubit) == Zero ? 9 | 10;
            let snapshot = values;
            values = snapshot;
            values
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0i\00"
        @2 = internal constant [6 x i8] c"2_a1i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %var_1 = icmp eq i1 %var_0, false
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 9, ptr %var_2
          br label %block_3
        block_2:
          store i64 10, ptr %var_2
          br label %block_3
        block_3:
          %var_5 = load i64, ptr %var_2
          store i64 %var_5, ptr %var_3
          call void @__quantum__rt__array_record_output(i64 2, ptr @0)
          %var_7 = load i64, ptr %var_3
          call void @__quantum__rt__int_record_output(i64 %var_7, ptr @1)
          call void @__quantum__rt__int_record_output(i64 2, ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn mutable_fixed_size_arrays_literal_assignment_after_dynamic_update() {
    let source = indoc::indoc! {r#"
        operation Main() : Int[] {
            use qubit = Qubit();
            mutable values = [1, 2];
            values[0] = MResetZ(qubit) == Zero ? 9 | 10;
            values = [3, 4];
            values
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0i\00"
        @2 = internal constant [6 x i8] c"2_a1i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %var_1 = icmp eq i1 %var_0, false
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 9, ptr %var_2
          br label %block_3
        block_2:
          store i64 10, ptr %var_2
          br label %block_3
        block_3:
          call void @__quantum__rt__array_record_output(i64 2, ptr @0)
          call void @__quantum__rt__int_record_output(i64 3, ptr @1)
          call void @__quantum__rt__int_record_output(i64 4, ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn mutable_fixed_size_arrays_concatenation_with_literal_array() {
    let source = indoc::indoc! {r#"
        operation Main() : Int[] {
            use qubit = Qubit();
            mutable values = [1, 2];
            values[0] = MResetZ(qubit) == Zero ? 9 | 10;
            values + [3]
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0i\00"
        @2 = internal constant [6 x i8] c"2_a1i\00"
        @3 = internal constant [6 x i8] c"3_a2i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %var_1 = icmp eq i1 %var_0, false
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 9, ptr %var_2
          br label %block_3
        block_2:
          store i64 10, ptr %var_2
          br label %block_3
        block_3:
          %var_5 = load i64, ptr %var_2
          store i64 %var_5, ptr %var_3
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          %var_7 = load i64, ptr %var_3
          call void @__quantum__rt__int_record_output(i64 %var_7, ptr @1)
          call void @__quantum__rt__int_record_output(i64 2, ptr @2)
          call void @__quantum__rt__int_record_output(i64 3, ptr @3)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn mutable_fixed_size_arrays_zero_size_iteration() {
    let source = indoc::indoc! {r#"
        operation Main() : Int[] {
            use qubit = Qubit();
            mutable values : Int[] = [];
            for index in 0..Length(values)-1 {
                values[index] = MResetZ(qubit) == Zero ? 1 | 2;
            }
            values
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Adaptive.into());
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__rt__array_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__rt__array_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn foreign_table_lookup_callable_generates_qir() {
    let source = indoc::indoc! {r#"
        operation Invoke() : Unit {
            use address = Qubit[1];
            use output = Qubit[1];
            Std.TableLookup.Select([[false], [true]], address, output);
            Adjoint Std.TableLookup.Select([[false], [true]], address, output);
        }
    "#};
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let capabilities = profile.into();
        let (mut store, package_id, items) =
            compile_and_locate_items(source, &[("Invoke", true)], capabilities);
        let root_sources =
            source_map_from_source("namespace DriverEmpty { function Anchor() : Unit {} }");
        let root_dependencies = vec![(package_id, Some(Arc::from("ReproLib")))];
        let (root_unit, root_errors) = crate::compile::compile(
            &store,
            &root_dependencies,
            root_sources,
            PackageType::Lib,
            capabilities,
            LanguageFeatures::default(),
        );
        assert!(
            root_errors.is_empty(),
            "{profile:?} root compilation failed: {root_errors:?}"
        );
        store.insert(root_unit);

        let qir = callable_args_to_qir(
            &store,
            package_id,
            items["Invoke"],
            &Value::unit(),
            capabilities,
        );
        assert!(
            qir.contains("define i64 @ENTRYPOINT__main()"),
            "{profile:?} callable should emit an entry point, got:\n{qir}"
        );
    }
}

#[test]
fn foreign_hof_capability_error_resolves_against_owning_package() {
    // `ForEach` lives in the std package; specializing it for this call site
    // clones std nodes into the user package, so the capability diagnostic must
    // still resolve against std's source map rather than the entry map.
    let source = r#"
namespace Test {
    import Std.Arrays.ForEach;
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        ForEach(q => M(q) == One, [q]);
    }
}
"#;
    let errors = compile_source_to_qir_result(source, TargetCapabilityFlags::from(Profile::Base))
        .expect_err("Base profile must reject a dynamic bool");

    // The diagnostics must be owned by two different packages: the user's lambda
    // and the std `ForEach` body specialized into the user package.
    let mut packages: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            crate::interpret::Error::Pass(with_source) => match with_source.error() {
                qsc_passes::Error::CapabilitiesCk(inner) => Some(inner.span().package),
                _ => None,
            },
            _ => None,
        })
        .collect();
    packages.sort_unstable();
    packages.dedup();
    assert!(
        packages.len() >= 2,
        "expected capability diagnostics from both the user and std packages, got {packages:?}"
    );

    // Rendering is the regression guard: resolving a std-owned span against the
    // entry source map used to panic inside `WithSource::from_map`.
    let rendered = format_interpret_errors(errors);
    assert!(
        rendered.contains("cannot use a dynamic bool"),
        "expected a dynamic-bool capability diagnostic, got:\n{rendered}"
    );
}

#[test]
fn residual_callable_sources_preserve_profile_acceptance() {
    use qsc_rca::errors::Error as CapabilityError;

    let mut failures = Vec::new();
    for (name, source) in residual_callable_sources() {
        for profile in [Profile::Base, Profile::AdaptiveRI, Profile::AdaptiveRIF] {
            let context = format!("{name}/{profile:?}");
            let accepted = match name {
                "false_branch" | "false_loop" => false,
                "post_return" => profile != Profile::Base,
                _ => true,
            };
            match compile_source_to_qir_result(&source, profile.into()) {
                Ok(qir) if accepted => {
                    let expected_x = match name {
                        "killed_producer" => 2,
                        "unrelated_callable" => 1,
                        _ => 0,
                    };
                    let expected_y = usize::from(name == "post_return");
                    if qir.matches("call void @__quantum__qis__x__body").count() != expected_x
                        || qir.matches("call void @__quantum__qis__y__body").count() != expected_y
                        || qir.contains("call void @__quantum__qis__h__body")
                    {
                        failures.push(format!("{context}: unexpected gate effects:\n{qir}"));
                    }
                }
                Err(errors) if !accepted => {
                    let expected_error = errors.iter().any(|error| {
                        let crate::interpret::Error::Pass(error) = error else {
                            return false;
                        };
                        matches!(
                            (name, error.error()),
                            (
                                "post_return",
                                qsc_passes::Error::CapabilitiesCk(
                                    CapabilityError::UseOfDynamicBool(_)
                                )
                            ) | (
                                "false_branch" | "false_loop",
                                qsc_passes::Error::CapabilitiesCk(
                                    CapabilityError::CallToDynamicCallee(_)
                                )
                            )
                        )
                    });
                    let only_capability_errors = errors.iter().all(|error| {
                        matches!(error, crate::interpret::Error::Pass(error) if matches!(error.error(), qsc_passes::Error::CapabilitiesCk(_)))
                    });
                    if !expected_error || !only_capability_errors {
                        failures.push(format!("{context}: unexpected typed errors: {errors:?}"));
                    }
                }
                result => failures.push(format!("{context}: unexpected acceptance: {result:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn residual_callable_sources() -> Vec<(&'static str, String)> {
    let mut sources = Vec::new();
    for (name, body) in [
        ("false_branch", "if false { ApplyOp(ops[index], q); }"),
        ("false_loop", "while false { ApplyOp(ops[index], q); }"),
        ("post_return", "return (); ApplyOp(ops[index], q);"),
    ] {
        sources.push((
            name,
            format!(
                r#"
            namespace Test {{
                operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {{ Y(q); [H, X] }}
                operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{ op(target); }}
                @EntryPoint()
                operation Main() : Unit {{
                    use q = Qubit();
                    let ops = MakeCandidates(q);
                    let index = if MResetZ(q) == Zero {{ 0 }} else {{ 1 }};
                    {body}
                }}
            }}
        "#
            ),
        ));
    }
    sources.push((
        "killed_producer",
        r#"
        namespace Test {
            operation MakeOp(q : Qubit) : Qubit => Unit { X(q); Rx(0.0, _) }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit { op(target); }
            operation Replacement(q : Qubit) : Unit { H(q); }
            operation LoopValue(q : Qubit) : Unit { X(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable op = MakeOp(q);
                op = Replacement;
                for _ in 0..2 { op = LoopValue; }
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#
        .to_string(),
    ));
    sources.push((
        "unrelated_callable",
        r#"
        namespace Test {
            function Identity(value : Int) : Int { value }
            operation Unrelated() : Unit { let decoy = Identity; }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Unrelated();
                mutable op = H;
                for _ in 0..3 { op = X; }
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#
        .to_string(),
    ));
    sources
}

#[test]
fn forwarded_closure_preserves_captured_offset_and_generates_base_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Forward(callable : Int -> Int) : Int -> Int { callable }

            @EntryPoint()
            operation Main() : Unit {
                let callable = Forward(Make(17));
                let actual = callable(1);
                let expected = 18;
                if actual != expected {
                    fail $"Expected forwarded closure to return {expected}, got {actual}";
                }
            }
        }
    "#};

    // The Q# assertion is evaluated during partial evaluation.
    compile_source_to_qir_result(source, Profile::Base.into())
        .expect("partial evaluation should verify that the forwarded closure returns 18");
}

#[test]
fn pipeline_snapshot_and_embedded_capture_identity_record_507() {
    let source = r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { x * 2 }
        function Wrap(f : Int -> Int) : Int -> Int { x -> f(x) + 1 }
        @EntryPoint()
        operation Main() : Int {
            let first = Wrap(Inc);
            let second = Wrap(Twice);
            mutable selected = first;
            let earlier = ({ selected })({ set selected = second; 3 });
            earlier * 100 + selected(3)
        }
    "#;
    let mut original = interpreter_with_capabilities(TargetCapabilityFlags::all());
    eval_fragments(&mut original, source);
    assert_eq!(eval_fragments(&mut original, "Main()"), Value::Int(507));
    assert_single_integer_output(
        &compile_source_to_qir(source, Profile::AdaptiveRIF.into()),
        507,
    );
}

#[test]
fn composite_callee_preserves_selection_effects_and_captures_in_qir() {
    for call in [
        "({ f })({ set f = Times2; 2 })",
        "(if true { f } else { Times2 })({ set f = Times2; 2 })",
        "Identity(f)({ set f = Times2; 2 })",
        "(new Holder { Op = f }).Op({ set f = Times2; 2 })",
        "Apply(if true { f } else { Times2 }, { set f = Times2; 2 })",
    ] {
        let source = format!(
            "namespace Test {{
                struct Holder {{ Op : Int -> Int }}
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ x * 2 }}
                function Identity(f : Int -> Int) : Int -> Int {{ f }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                @EntryPoint() operation Main() : Int {{
                    mutable f = Add1;
                    {call}
                }}
            }}"
        );
        let qir = compile_source_to_qir(&source, Profile::AdaptiveRIF.into());
        let outputs: Vec<_> = qir
            .lines()
            .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
            .collect();
        assert_eq!(outputs.len(), 1, "{call}\n{qir}");
        assert!(outputs[0].contains("(i64 3,"), "{call}\n{qir}");
    }
    for flag in [true, false] {
        for callee in [
            "{ set count += 1; flag ? Add1 | Times2 }",
            "{ { set count += 1; flag ? Add1 | Times2 } }",
            "if flag { set count += 1; Add1 } else { set count += 1; Times2 }",
            "{ set count += 1; if flag { let n = count; x -> x + n } else { Times2 } }",
            "{ let n = { set count += 1; count }; if flag { x -> x + n } else { Times2 } }",
            if flag {
                "if flag { set count += 1; Add1 } else { fail \"inactive callee\"; Times2 }"
            } else {
                "if flag { fail \"inactive callee\"; Add1 } else { set count += 1; Times2 }"
            },
        ] {
            for (argument, count) in [
                ("2", 1),
                ("{ set count += 10; set flag = not flag; 2 }", 11),
            ] {
                let expected = count * 100 + if flag { 3 } else { 4 };
                let source = format!(
                    "namespace Test {{
                        function Add1(x : Int) : Int {{ x + 1 }}
                        function Times2(x : Int) : Int {{ 2 * x }}
                        @EntryPoint() operation Main() : Int {{
                            mutable count = 0;
                            mutable flag = {flag};
                            let value = ({callee})({argument});
                            count * 100 + value
                        }}
                    }}"
                );
                let qir = compile_source_to_qir(&source, Profile::AdaptiveRIF.into());
                let outputs: Vec<_> = qir
                    .lines()
                    .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
                    .collect();
                assert_eq!(outputs.len(), 1, "{source}\n{qir}");
                assert!(
                    outputs[0].contains(&format!("(i64 {expected},")),
                    "{source}\n{qir}"
                );
            }
        }
        let source = format!(
            "namespace Test {{
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ 2 * x }}
                @EntryPoint() operation Main() : Int {{
                    mutable flag = {flag};
                    ({{ fail \"callee prefix failed\"; flag ? Add1 | Times2 }})(
                        {{ fail \"argument failed\"; 2 }})
                }}
            }}"
        );
        let errors = compile_source_to_qir_result(&source, Profile::AdaptiveRIF.into())
            .expect_err("callee failure must precede the argument");
        let [crate::interpret::Error::PartialEvaluation(error)] = errors.as_slice() else {
            panic!("expected a partial-evaluation failure: {errors:?}");
        };
        let qsc_partial_eval::Error::EvaluationFailed(message, span) = error.error() else {
            panic!("expected a source evaluation failure: {error:?}");
        };
        assert!(message.contains("callee prefix failed"), "{message}");
        assert_eq!(
            &source[span.span.lo as usize..span.span.hi as usize],
            "fail \"callee prefix failed\"",
        );
    }
    for body in [
        "for i in 0..2 { set total += ({ set count += 1; flag ? Add1 | Times2 })({ set flag = not flag; 2 }); }",
        "mutable i = 0; repeat { set total += ({ set count += 1; flag ? Add1 | Times2 })({ set flag = not flag; 2 }); set i += 1; } until i == 3;",
    ] {
        let source = format!(
            "namespace Test {{
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ 2 * x }}
                @EntryPoint() operation Main() : Int {{
                    mutable count = 0;
                    mutable flag = true;
                    mutable total = 0;
                    {body}
                    count * 100 + total
                }}
            }}"
        );
        let qir = compile_source_to_qir(&source, Profile::AdaptiveRIF.into());
        let outputs: Vec<_> = qir
            .lines()
            .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
            .collect();
        assert_eq!(outputs.len(), 1, "{source}\n{qir}");
        assert!(outputs[0].contains("(i64 310,"), "{source}\n{qir}");
    }
    for (flag, expected) in [(true, 33), (false, 44)] {
        let source = format!(
            "namespace Test {{
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ 2 * x }}
                @EntryPoint() operation Main() : Int {{
                    mutable flag = {flag};
                    mutable f = Add1;
                    let value = (if flag {{ set f = Add1; Add1 }} else {{ set f = Times2; Times2 }})(
                        {{ set flag = not flag; 2 }});
                    value * 10 + f(2)
                }}
            }}"
        );
        let qir = compile_source_to_qir(&source, Profile::AdaptiveRIF.into());
        let outputs: Vec<_> = qir
            .lines()
            .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
            .collect();
        assert_eq!(outputs.len(), 1, "{source}\n{qir}");
        assert!(
            outputs[0].contains(&format!("(i64 {expected},")),
            "{source}\n{qir}"
        );
    }
}

#[test]
fn tuple_assignment_preserves_callable_and_value_in_base_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            @EntryPoint()
            operation Main() : Unit {
                mutable (value, callable) = (14, Add11);
                set (value, callable) = (9, Times3);
                if callable(2) != 6 { fail "wrong assigned callable"; }
                let pair = (7, Add11);
                set (value, callable) = pair;
                if value * 100 + callable(2) != 713 { fail "wrong tuple copy"; }
            }
        }
    "#};
    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert!(qir.contains("call void @__quantum__rt__tuple_record_output(i64 0"));
}

#[test]
fn closure_used_in_capture_assignment_compiles_to_base_qir() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle, _);
                set angle = { op(target); 0.0 };
                op(target);
                Reset(target);
            }
        }
    "#;
    let result = compile_source_to_qir_result(source, Profile::Base.into());
    assert!(result.is_ok(), "expected Base QIR acceptance: {result:?}");
}

#[test]
fn callable_payload_dispatch_preserves_qir_acceptance() {
    let source = r#"
        namespace Test {
            operation Dispatch(
                choices : ((Qubit => Unit) => Unit)[],
                index : Int,
                payload : Qubit => Unit
            ) : Unit {
                choices[index](payload);
            }

            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                use selector = Qubit();
                H(selector);
                let choices : ((Qubit => Unit) => Unit)[] = [
                    op => { H(target); op(target); },
                    op => { X(target); op(target); }
                ];
                Dispatch(choices, 0, S);
                Reset(selector);
                MResetZ(target)
            }
        }
    "#;
    let qir = compile_source_to_qir(source, Profile::AdaptiveRIF.into());
    assert!(qir.contains("call void @__quantum__qis__s__body("));
}

#[test]
fn factory_capture_uses_creation_time_value_in_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Mark(enabled : Bool, q : Qubit) : Unit {
                if enabled {
                    X(q);
                }
            }
            function Make(enabled : Bool) : Qubit => Unit {
                Mark(enabled, _)
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable enabled = false;
                let op = Make(enabled);
                set enabled = true;
                ApplyOp(op, q);
                Reset(q);
            }
        }
    "#};

    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        0,
        "factory closure must retain the creation-time false capture:\n{qir}"
    );
}

#[test]
fn source_entry_callable_array_effectful_index_runs_once() {
    let source = r#"
        namespace Test {
            operation ChooseIndex(q : Qubit) : Int {
                X(q);
                1
            }

            operation RunAt(ops : (Qubit => Unit)[], q : Qubit) : Unit {
                ops[ChooseIndex(q)](q);
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                RunAt([I, X, Y], q);
                Reset(q);
            }
        }
    "#;

    let qir = compile_source_to_qir(source, Profile::Base.into());
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        2,
        "expected one index effect and one selected X gate:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__qis__y__body"),
        "expected no unselected Y gate:\n{qir}"
    );
}

#[test]
fn dispatched_callable_before_classical_field_reports_diagnostics() {
    // A tuple parameter mixing callables with a classical field between them,
    // where the dispatched callable sits at an earlier field than the static
    // one. Argument promotion used to leave the call site disagreeing with the
    // callee's input type, tripping the `PostArgPromote/PostAll` call invariant
    // and aborting the compiler on valid Q#.
    let source = r#"
operation Apply(data : (Qubit => Unit, Int, Qubit => Unit), q : Qubit) : Unit {
    let (f, n, g) = data;
    f(q); g(q);
}
@EntryPoint()
operation Main() : Unit {
    use q = Qubit();
    let first = if MResetZ(q) == One { X } else { Y };
    Apply((first, 5, Z), q);
}
"#;
    let errors =
        compile_source_to_qir_result(source, TargetCapabilityFlags::from(Profile::AdaptiveRIF))
            .expect_err("AdaptiveRIF must reject a measurement-dependent callable");

    // Reaching any capability diagnostic at all means the transform pipeline ran
    // to completion; the regression aborted the process before this point.
    assert!(
        errors.iter().any(|e| matches!(
            e,
            crate::interpret::Error::Pass(with_source)
                if matches!(with_source.error(), qsc_passes::Error::CapabilitiesCk(..))
        )),
        "expected capability diagnostics, got: {errors:?}"
    );
}

fn assert_single_integer_output(qir: &str, expected: i64) {
    let records: Vec<_> = qir
        .lines()
        .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
        .collect();
    assert_eq!(records.len(), 1, "{qir}");
    assert!(records[0].contains(&format!("i64 {expected},")), "{qir}");
}

#[test]
fn generic_lambda_dependencies_compile_through_executable_entries() {
    check_generic_lambda_dependency_qir(compile_source_to_qir);
}

#[test]
fn generic_lambda_dependencies_compile_through_incremental_entries() {
    check_generic_lambda_dependency_qir(|source, capabilities| {
        let mut interpreter = interpreter_with_capabilities(capabilities);
        eval_fragments(&mut interpreter, source);
        interpreter
            .qirgen("Main()")
            .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)))
    });
}

fn check_generic_lambda_dependency_qir(compile: impl Fn(&str, TargetCapabilityFlags) -> String) {
    let definitions = r#"
        function Id<'T>(x : 'T) : 'T { x }
        function Outer<'T>(x : 'T) : 'T {
            let f = y -> Id(y);
            f(x)
        }
    "#;
    for (main, profile, expected) in [
        (
            "operation Main() : Unit { let _ = Outer(1); }",
            Profile::Base,
            None,
        ),
        (
            "operation Main() : Int { Outer(7) }",
            Profile::AdaptiveRIF,
            Some(7),
        ),
    ] {
        let source = format!("{definitions}\n{main}");
        let qir = compile(&source, profile.into());
        if let Some(expected) = expected {
            assert_single_integer_output(&qir, expected);
        } else {
            assert!(
                qir.contains("call void @__quantum__rt__tuple_record_output(i64 0,"),
                "{profile:?}: {qir}"
            );
        }
    }
}

/// A direct use consumes one library closure while a forwarded, reassigned
/// aggregate still needs its own closure and data field. The QIR must record
/// `1 * 10 + (3 + 3)`, preserving both captured values and the deferred field.
#[test]
fn foreign_forwarded_closure_aggregates_preserve_captures_and_data_in_qir() {
    let library = r#"
        namespace Lib {
            function Make(n : Int) : (Int -> Int, Int) { (x -> x + n, n) }
            function Forward(n : Int) : (Int -> Int, Int) { Make(n) }
            function Apply(pair : (Int -> Int, Int)) : Int {
                let (f, n) = pair;
                f(0) + n
            }
            export Make, Forward, Apply;
        }
    "#;
    let source = r#"
        @EntryPoint()
        operation Main() : Int {
            let (first, _) = Lib.Make(1);
            mutable deferred = Lib.Forward(0);
            for index in 0..0 { set deferred = Lib.Forward(3); }
            first(0) * 10 + Lib.Apply(deferred)
        }
    "#;
    assert_single_integer_output(
        &compile_source_to_qir_with_library(library, source, Profile::AdaptiveRIF.into()),
        16,
    );
}

/// Base QIR cannot return integer values, so make each computed value control
/// a gate count on a different qubit. The emitted calls must contain eight Xs
/// for the direct call and twelve for the higher-order call, in that order.
#[test]
fn nested_closure_captures_preserve_values_in_base_qir() {
    let qir = compile_source_to_qir(
        indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {
                    value -> inner(value) * scale
                }
                function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
                @EntryPoint()
                operation Main() : Unit {
                    let first = Wrap(Make(3), 2);
                    use qubits = Qubit[2];
                    for _ in 1..first(1) { X(qubits[0]); }
                    for _ in 1..Invoke(first, 3) { X(qubits[1]); }
                }
            }
        "#},
        Profile::Base.into(),
    );
    let gates: Vec<_> = qir
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("call void @__quantum__qis__"))
        .collect();
    let expected: Vec<_> = std::iter::repeat_n(
        "call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))",
        8,
    )
    .chain(std::iter::repeat_n(
        "call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))",
        12,
    ))
    .collect();
    assert_eq!(gates, expected, "{qir}");
}

#[test]
fn generated_generic_entry_qir_reports_missing_type_arguments() {
    check_generated_entry_missing_type_arguments(
        "@EntryPoint() operation Main<'T>() : 'T { fail \"unreachable\" }",
    );
}

#[test]
fn generated_entry_rejects_unused_type_parameters() {
    use miette::Diagnostic;

    let source = "@EntryPoint() operation Main<'T : Eq, 'U : Show>() : Int { 42 }";
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let errors = compile_source_to_qir_result(source, profile.into())
            .expect_err("unused type parameters must be rejected before entry generation");
        let expected = [
            ("Qdk.Qsc.TypeCk.UnusedGeneric", "'T"),
            ("Qdk.Qsc.TypeCk.UnusedGeneric", "'U"),
            ("Qdk.Qsc.LowerAst.UnusedGeneric", "'T"),
            ("Qdk.Qsc.LowerAst.UnusedGeneric", "'U"),
        ];
        assert_eq!(errors.len(), expected.len(), "{profile:?}: {errors:?}");
        for (error, (code, parameter)) in errors.iter().zip(expected) {
            assert_eq!(error.code().expect("diagnostic code").to_string(), code);
            let labels: Vec<_> = error.labels().expect("parameter source").collect();
            assert_eq!(labels.len(), 1);
            let span = labels[0].inner();
            assert_eq!(
                &source[span.offset()..span.offset() + span.len()],
                parameter
            );
            assert!(error.source_code().is_some());
        }
    }
}

fn check_generated_entry_missing_type_arguments(source: &str) {
    use miette::Diagnostic;

    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let errors = compile_source_to_qir_result(source, profile.into())
            .expect_err("generated entry requires concrete type arguments");
        assert_eq!(errors.len(), 1, "{profile:?}: {errors:?}");
        assert_eq!(
            errors[0].code().expect("diagnostic code").to_string(),
            "Qdk.Qsc.EntryPoint.MissingTypeArguments"
        );
        let labels: Vec<_> = errors[0].labels().expect("entry source").collect();
        assert_eq!(labels.len(), 1);
        let span = labels[0].inner();
        assert_eq!(&source[span.offset()..span.offset() + span.len()], "Main");
        assert!(errors[0].source_code().is_some());
    }
}

#[test]
fn explicit_generic_entry_call_requires_type_evidence() {
    use miette::Diagnostic;

    let mut interpreter = interpreter_with_capabilities(Profile::AdaptiveRIF.into());
    eval_fragments(
        &mut interpreter,
        "operation Generic<'T>() : 'T { fail \"unreachable\" }",
    );
    let errors = interpreter
        .qirgen("Generic()")
        .expect_err("ordinary explicit calls still need type evidence");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].code().expect("diagnostic code").to_string(),
        "Qdk.Qsc.TypeCk.AmbiguousTy"
    );
}

#[test]
fn generated_nongeneric_entry_qir_reports_source_located_user_failure() {
    use miette::Diagnostic;

    let source = "@EntryPoint() operation Main() : Result { fail \"unreachable\"; }";
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let errors = compile_source_to_qir_result(source, profile.into())
            .expect_err("valid nongeneric entry must reach its user failure");
        let [crate::interpret::Error::PartialEvaluation(error)] = errors.as_slice() else {
            panic!("{profile:?}: {errors:?}");
        };
        assert_eq!(
            error.code().expect("diagnostic code").to_string(),
            "Qdk.Qsc.PartialEval.EvaluationFailed"
        );
        assert_eq!(
            error.to_string(),
            "partial evaluation failed with error: program failed: unreachable"
        );
        let labels: Vec<_> = error.labels().expect("failure source").collect();
        assert_eq!(labels.len(), 1);
        let (location, span) = error.resolve_span(labels[0].inner());
        assert_eq!(
            &location.contents[span.offset()..span.offset() + span.len()],
            "fail \"unreachable\""
        );
    }
}

/// Check the computed value as an `AdaptiveRIF` integer output and as a Base
/// rotation operand. This verifies emitted data, not merely successful compilation.
/// These fixtures also protect existing tuple-alias behavior; the isolated
/// prepass tests assert that environment normalization itself runs.
fn assert_classical_result_in_qir(source: &str, expected: i32) {
    let adaptive = format!(
        "{source}\nnamespace Entry {{
            @EntryPoint() operation Main() : Int {{ Test.Compute() }}
        }}"
    );
    assert_single_integer_output(
        &compile_source_to_qir(&adaptive, Profile::AdaptiveRIF.into()),
        i64::from(expected),
    );
    let base = format!(
        "{source}\nnamespace Entry {{
            @EntryPoint() operation Main() : Unit {{
                use q = Qubit();
                Rz(Std.Convert.IntAsDouble(Test.Compute()), q);
            }}
        }}"
    );
    let qir = compile_source_to_qir(&base, Profile::Base.into());
    let gates: Vec<_> = qir
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("call void @__quantum__qis__"))
        .collect();
    assert_eq!(gates.len(), 1, "{qir}");
    let angle = gates[0]
        .strip_prefix("call void @__quantum__qis__rz__body(double ")
        .expect("expected a rotation with a constant angle")
        .split(',')
        .next()
        .expect("rotation angle")
        .parse::<f64>()
        .expect("constant double");
    // Every i32 is exactly representable as f64; no tolerance is needed here.
    assert_eq!(angle.to_bits(), f64::from(expected).to_bits(), "{qir}");
}

#[test]
fn nonadjacent_callable_tuple_snapshot_preserves_original_values_in_qir() {
    let source = r#"
        namespace Test {
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            function Compute() : Int {
                mutable (value, callable) = (14, Add11);
                let pair = (value, callable);
                set value = 9;
                set callable = Times3;
                set (value, callable) = pair;
                value * 100 + callable(2)
            }
        }
    "#;
    assert_classical_result_in_qir(source, 1413);
}

#[test]
fn nested_callable_tuple_snapshot_preserves_scalar_and_callable_values_in_qir() {
    let source = r#"
        namespace Test {
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            function Compute() : Int {
                mutable (value, callable) = (14, Add11);
                let (tag, saved) = (3, (value, callable));
                set (value, callable) = (9, Times3);
                set (value, callable) = saved;
                tag * 10000 + value * 100 + callable(2)
            }
        }
    "#;
    assert_classical_result_in_qir(source, 31_413);
}

#[test]
fn captured_callable_tuple_snapshot_survives_reassignment_in_qir() {
    let source = r#"
        namespace Test {
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            function Compute() : Int {
                mutable (value, callable) = (14, Add11);
                let saved = (value, callable);
                let observe = input -> {
                    let (stored, action) = saved;
                    stored * 100 + action(input)
                };
                set (value, callable) = (9, Times3);
                let before = observe(2);
                set (value, callable) = saved;
                before * 10000 + value * 100 + callable(2)
            }
        }
    "#;
    assert_classical_result_in_qir(source, 14_131_413);
}

#[test]
fn forwarded_callable_tuple_preserves_original_capture_in_qir() {
    let source = r#"
        namespace Test {
            function Forward(pair : (Int, Int -> Int)) : (Int, Int -> Int) { pair }
            function Compute() : Int {
                mutable offset = 3;
                mutable (value, callable) = (14, {
                    let captured = offset;
                    input -> input + captured
                });
                let saved = Forward((value, callable));
                set offset = 17;
                set (value, callable) = (9, {
                    let captured = offset;
                    input -> input + captured
                });
                let forwarded = Forward(saved);
                set (value, callable) = forwarded;
                offset * 10000 + value * 100 + callable(2)
            }
        }
    "#;
    assert_classical_result_in_qir(source, 171_405);
}

#[test]
fn nested_wrapper_array_preserves_distinct_capture_values_in_qir() {
    let source = r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {
                value -> inner(value) * scale
            }
            function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
            function Compute() : Int {
                let wrappers = [Wrap(Wrap(Make(3), 2), 3), Wrap(Wrap(Make(17), 5), 7)];
                mutable answer = 0;
                for index in 0..0 {
                    set answer = 1000 * Invoke(wrappers[index], 1) + wrappers[1 - index](1);
                }
                answer
            }
        }
    "#;
    assert_classical_result_in_qir(source, 24_630);
}

#[test]
fn controlled_struct_factory_preserves_control_and_target_qubits_in_qir() {
    let source = r#"
        struct PauliSelectParams {
            paulis : Pauli[][],
            qubitIndices : Int[],
            signs : Int[]
        }

        operation ApplySelect(params : PauliSelectParams, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.signs) != 0 {
                X(systems[0]);
            }
        }

        operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        @EntryPoint()
        operation Main() : Unit {
            let params = new PauliSelectParams {
                paulis = [[PauliX]],
                qubitIndices = [0],
                signs = [1]
            };
            let sel = ApplySelect(params, _, _);
            MakeControlledPrepSelPrepCircuit(ApplyPrepare, sel, 1, 1);
        }
    "#;
    let qir = compile_source_to_qir(source, Profile::Base.into());
    let gates: Vec<_> = qir
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("call void @__quantum__qis__"))
        .collect();
    assert_eq!(
        gates,
        [
            "call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))"
        ],
        "{qir}"
    );
}

#[test]
fn unselected_non_unit_helper_with_semicolon_failure_records_forty_two() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                function Deferred() : Int {
                    fail "expected";
                }
                if false { Deferred() } else { 42 }
            }
        }
    "#;
    let qir = compile_source_to_qir(source, Profile::AdaptiveRIF.into());
    assert_single_integer_output(&qir, 42);
}

#[test]
fn non_unit_entry_with_eager_failure_reports_original_user_error() {
    use miette::Diagnostic;

    for body in [
        "fail \"expected\";",
        "[fail \"expected\"];",
        "{ let value = { fail \"expected\"; 0 }; while false {} }",
    ] {
        let source =
            format!("namespace Test {{ @EntryPoint() operation Main() : Int {{ {body} }} }}");
        let errors = compile_source_to_qir_result(&source, Profile::AdaptiveRIF.into())
            .expect_err("executed fail should return an evaluation error");
        let [crate::interpret::Error::PartialEvaluation(error)] = errors.as_slice() else {
            panic!("expected a partial-evaluation failure, got: {errors:?}")
        };
        assert_eq!(
            error.code().expect("diagnostic code").to_string(),
            "Qdk.Qsc.PartialEval.EvaluationFailed"
        );
        assert_eq!(
            error.to_string(),
            "partial evaluation failed with error: program failed: expected"
        );
        let labels: Vec<_> = error.labels().expect("failure source").collect();
        assert_eq!(labels.len(), 1);
        let (location, span) = error.resolve_span(labels[0].inner());
        assert_eq!(
            &location.contents[span.offset()..span.offset() + span.len()],
            "fail \"expected\""
        );
    }
}

#[test]
fn return_in_while_condition_emits_forty_two_in_base_and_adaptive_qir() {
    assert_classical_result_in_qir(
        indoc::indoc! {r#"
            namespace Test {
                function Compute() : Int {
                    while ({ return 42; 0 }) < 1 {}
                    0
                }
            }
        "#},
        42,
    );
}
