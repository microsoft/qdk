// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Env, Error, ErrorBehavior, State, StepAction, StepResult, Value,
    backend::{Backend, SparseSim},
    debug::Frame,
    exec_graph_section,
    output::{GenericReceiver, Receiver},
    val,
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
use qsc_fir::fir::{self, ExecGraph, StmtId};
use qsc_fir::fir::{PackageId, PackageStoreLookup};
use qsc_frontend::compile::{self, PackageStore, SourceMap, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_passes::{PackageType, run_core_passes, run_default_passes};

/// Evaluates the given control flow graph with the given context.
/// Creates a new environment and simulator.
/// # Errors
/// Returns the first error encountered during execution.
pub(super) fn eval_graph(
    graph: ExecGraph,
    sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
    globals: &impl PackageStoreLookup,
    package: PackageId,
    env: &mut Env,
    out: &mut impl Receiver,
) -> Result<Value, (Error, Vec<Frame>)> {
    let mut state = State::new(package, graph, None, ErrorBehavior::FailOnError);
    let StepResult::Return(value) =
        state.eval(globals, env, sim, out, &[], StepAction::Continue)?
    else {
        unreachable!("eval_expr should always return a value");
    };
    Ok(value)
}

fn check_expr(file: &str, expr: &str, expect: &Expect) {
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    // store can be empty since core doesn't have any dependencies
    let core_fir = fir_lowerer.lower_package(&core.package, &fir_store);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, TargetCapabilityFlags::all());
    assert!(std.errors.is_empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std_fir = fir_lowerer.lower_package(&std.package, &fir_store);
    let std_id = store.insert(std);

    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let mut unit = compile(
        &store,
        &[(std_id, None)],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let unit_fir = fir_lowerer.lower_package(&unit.package, &fir_store);
    let entry = unit_fir.entry_exec_graph.clone();
    let id = store.insert(unit);

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    fir_store.insert(map_hir_package_to_fir(id), unit_fir);

    let mut out = Vec::new();
    match eval_graph(
        entry,
        &mut SparseSim::new(),
        &fir_store,
        map_hir_package_to_fir(id),
        &mut Env::default(),
        &mut GenericReceiver::new(&mut out),
    ) {
        Ok(value) => expect.assert_eq(&value.to_string()),
        Err((err, _)) => expect.assert_debug_eq(&err),
    }
}

fn check_partial_eval_stmt(
    file: &str,
    expr: &str,
    stmts: &[StmtId],
    fir_expect: &Expect,
    result_expect: &Expect,
) {
    let mut core = compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    let core_fir = qsc_lowerer::Lowerer::new().lower_package(&core.package, &fir_store);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, TargetCapabilityFlags::all());
    assert!(std.errors.is_empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std_fir = qsc_lowerer::Lowerer::new().lower_package(&std.package, &fir_store);
    let std_id = store.insert(std);

    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let mut unit = compile(
        &store,
        &[(std_id, None)],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let unit_fir = qsc_lowerer::Lowerer::new().lower_package(&unit.package, &fir_store);
    fir_expect.assert_eq(&unit_fir.to_string());

    let entry = unit_fir.entry_exec_graph.clone();
    let id = store.insert(unit);

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    let id = map_hir_package_to_fir(id);
    fir_store.insert(id, unit_fir);

    let mut out = Vec::new();
    let mut env = Env::default();
    let (last_stmt, most_stmts) = stmts.split_last().expect("should have at least one stmt");
    for stmt_id in most_stmts {
        let stmt = fir_store.get_stmt((id, *stmt_id).into());
        match eval_graph(
            exec_graph_section(&entry, stmt.exec_graph_range.clone()),
            &mut SparseSim::new(),
            &fir_store,
            id,
            &mut env,
            &mut GenericReceiver::new(&mut out),
        ) {
            Ok(_) => {}
            Err(err) => panic!("Unexpected error: {err:?}"),
        }
    }

    let stmt = fir_store.get_stmt((id, *last_stmt).into());
    match eval_graph(
        exec_graph_section(&entry, stmt.exec_graph_range.clone()),
        &mut SparseSim::new(),
        &fir_store,
        id,
        &mut env,
        &mut GenericReceiver::new(&mut out),
    ) {
        Ok(value) => result_expect.assert_eq(&value.to_string()),
        Err(err) => result_expect.assert_debug_eq(&err),
    }
}

#[test]
fn array_expr() {
    check_expr("", "[1, 2, 3]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_repeat_expr() {
    check_expr("", "[4, size = 3]", &expect!["[4, 4, 4]"]);
}

#[test]
fn block_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let y = x;
            y
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_empty_is_unit_expr() {
    check_expr("", "{}", &expect!["()"]);
}

#[test]
fn block_shadowing_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let x = 2;
            x
        }"},
        &expect!["2"],
    );
}

#[test]
fn block_nested_shadowing_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let y = {
                let x = 2;
                x
            };
            (y, x)
        }"},
        &expect!["(2, 1)"],
    );
}

#[test]
fn block_let_bind_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            let x = (1, 2);
            let (y, z) = x;
            (z, y)
        }"},
        &expect!["(2, 1)"],
    );
}

#[test]
fn block_let_bind_assign_expr_is_unit() {
    check_expr(
        "",
        indoc! {"{
            mutable a = 0;
            let b = a = 1;
            (a, b)
        }"},
        &expect!["(1, ())"],
    );
}

#[test]
fn block_let_bind_assign_field_expr_is_unit() {
    check_expr(
        "",
        indoc! {"{
            struct S {
                inner : Int,
            }
            mutable a = new S { inner = 0 };
            let b = a w/= inner <- 1;
            (a.inner, b)
        }"},
        &expect!["(1, ())"],
    );
}

#[test]
fn block_let_bind_assign_index_expr_is_unit() {
    check_expr(
        "",
        indoc! {"{
            mutable a = [0];
            let b = a[0] = 1;
            (a, b)
        }"},
        &expect!["([1], ())"],
    );
}

#[test]
fn block_let_bind_assign_op_expr_is_unit() {
    check_expr(
        "",
        indoc! {"{
            mutable a = 0;
            let b = a += 1;
            (a, b)
        }"},
        &expect!["(1, ())"],
    );
}

#[test]
fn block_mutable_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            x
        }"},
        &expect!["0"],
    );
}

#[test]
fn block_mutable_update_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x = 1;
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_mutable_update_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = (0, 1);
            set x = (1, 2);
            x
        }"},
        &expect!["(1, 2)"],
    );
}

#[test]
fn block_mutable_update_tuple_item_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable (x, y) = (0, 1);
            set (x, y) = (1, 2);
            (x, y)
        }"},
        &expect!["(1, 2)"],
    );
}

#[test]
fn block_mutable_update_tuple_hole_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable (x, y) = (0, 1);
            set (_, y) = (1, 2);
            (x, y)
        }"},
        &expect!["(0, 2)"],
    );
}

#[test]
fn block_mutable_nested_scopes_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            {
                mutable y = 1;
                set x = y;
            }
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_mutable_nested_scopes_shadowing_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            {
                mutable x = 1;
                set x = 2;
            }
            x
        }"},
        &expect!["0"],
    );
}

#[test]
fn block_qubit_use_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = Qubit();
            $"{q}"
        }"#},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_use_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = Qubit();
            use q1 = Qubit();
            $"{q1}"
        }"#},
        &expect!["Qubit1"],
    );
}

#[test]
fn block_qubit_use_reuse_expr() {
    check_expr(
        "",
        indoc! {r#"{
            {
                use q = Qubit();
            }
            use q = Qubit();
            $"{q}"
        }"#},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_scope_reuse_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = Qubit() {
            }
            use q = Qubit();
            $"{q}"
        }"#},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_array_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = Qubit[3];
            $"{q}"
        }"#},
        &expect!["[Qubit0, Qubit1, Qubit2]"],
    );
}

#[test]
fn block_qubit_use_array_invalid_count_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit[-3];
            q
        }"},
        &expect![[r#"
            UserFail(
                "Cannot allocate qubit array with a negative length",
                PackageSpan {
                    package: PackageId(
                        0,
                    ),
                    span: Span {
                        lo: 2664,
                        hi: 2721,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn block_qubit_use_tuple_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = (Qubit[3], Qubit(), Qubit());
            $"{q}"
        }"#},
        &expect!["([Qubit0, Qubit1, Qubit2], Qubit3, Qubit4)"],
    );
}

#[test]
fn block_qubit_use_nested_tuple_expr() {
    check_expr(
        "",
        indoc! {r#"{
            use q = (Qubit[3], (Qubit(), Qubit()));
            $"{q}"
        }"#},
        &expect!["([Qubit0, Qubit1, Qubit2], (Qubit3, Qubit4))"],
    );
}

#[test]
fn block_with_no_stmts_is_unit() {
    check_expr("", "{}", &expect!["()"]);
}

#[test]
fn block_with_semi_is_unit() {
    check_expr("", "{4;}", &expect!["()"]);
}

#[test]
fn binop_add_array() {
    check_expr("", "[1, 2] + [3, 4]", &expect!["[1, 2, 3, 4]"]);
}

#[test]
fn binop_add_bigint() {
    check_expr(
        "",
        "2L + 9_223_372_036_854_775_808L",
        &expect!["9223372036854775810"],
    );
}

#[test]
fn binop_add_double() {
    check_expr("", "2.8 + 5.4", &expect!["8.2"]);
}

#[test]
fn binop_add_int() {
    check_expr("", "28 + 54", &expect!["82"]);
}

#[test]
fn binop_add_int_wrap() {
    check_expr(
        "",
        "0x7FFFFFFFFFFFFFFF + 1",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn binop_add_string() {
    check_expr("", r#""Hello," + " World!""#, &expect!["Hello, World!"]);
}

#[test]
fn binop_andb_bigint() {
    check_expr("", "28L &&& 54L", &expect!["20"]);
}

#[test]
fn binop_andb_int() {
    check_expr("", "28 &&& 54", &expect!["20"]);
}

#[test]
fn binop_andl() {
    check_expr("", "true and true", &expect!["true"]);
}

#[test]
fn binop_andl_false() {
    check_expr("", "true and false", &expect!["false"]);
}

#[test]
fn binop_andl_shortcut() {
    check_expr("", r#"false and (fail "Should Fail")"#, &expect!["false"]);
}

#[test]
fn binop_andl_no_shortcut() {
    check_expr(
        "",
        r#"true and (fail "Should Fail")"#,
        &expect![[r#"
            UserFail(
                "Should Fail",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 10,
                        hi: 28,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_div_bigint() {
    check_expr("", "12L / 3L", &expect!["4"]);
}

#[test]
fn binop_div_bigint_zero() {
    check_expr(
        "",
        "12L / 0L",
        &expect![[r#"
            DivZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 6,
                        hi: 8,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_div_int() {
    check_expr("", "12 / 3", &expect!["4"]);
}

#[test]
fn binop_div_int_wrap() {
    check_expr(
        "",
        "(-0x8000000000000000) / (-1)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn binop_div_int_zero() {
    check_expr(
        "",
        "12 / 0",
        &expect![[r#"
            DivZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 5,
                        hi: 6,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_div_double() {
    check_expr("", "1.2 / 0.3", &expect!["4.0"]);
}

#[test]
fn binop_div_double_inf() {
    check_expr("", "1.2 / 0.0", &expect!["inf"]);
}

#[test]
fn binop_div_double_neg_inf() {
    check_expr("", "1.2 / -0.0", &expect!["-inf"]);
}

#[test]
fn binop_div_double_nan() {
    check_expr("", "0.0 / 0.0", &expect!["NaN"]);
}

#[test]
fn binop_eq_double() {
    check_expr("", "1.2 / 0.3", &expect!["4.0"]);
}

#[test]
fn binop_equal_array() {
    check_expr("", "[1, 2, 3] == [1, 2, 3]", &expect!["true"]);
}

#[test]
fn binop_equal_array_false_content() {
    check_expr("", "[1, 2, 3] == [1, 0, 3]", &expect!["false"]);
}

#[test]
fn binop_equal_array_false_length() {
    check_expr("", "[1, 2, 3] == [1, 2, 3, 4]", &expect!["false"]);
}

#[test]
fn binop_equal_bigint() {
    check_expr("", "18L == 18L", &expect!["true"]);
}

#[test]
fn binop_equal_bigint_false() {
    check_expr("", "18L == 8L", &expect!["false"]);
}

#[test]
fn binop_equal_bool() {
    check_expr("", "false == false", &expect!["true"]);
}

#[test]
fn binop_equal_bool_false() {
    check_expr("", "false == true", &expect!["false"]);
}

#[test]
fn binop_equal_double() {
    check_expr("", "1.254 == 1.254", &expect!["true"]);
}

#[test]
fn binop_equal_double_false() {
    check_expr("", "1.254 == 1.25", &expect!["false"]);
}

#[test]
fn binop_equal_int() {
    check_expr("", "42 == 42", &expect!["true"]);
}

#[test]
fn binop_equal_int_false() {
    check_expr("", "42 == 43", &expect!["false"]);
}

#[test]
fn binop_equal_pauli() {
    check_expr("", "PauliX == PauliX", &expect!["true"]);
}

#[test]
fn binop_equal_pauli_false() {
    check_expr("", "PauliX == PauliZ", &expect!["false"]);
}

#[test]
fn binop_equal_range() {
    check_expr("", "(0..4) == (0..4)", &expect!["true"]);
}

#[test]
fn binop_equal_range_false() {
    check_expr("", "(0..2..4) == (0..4)", &expect!["false"]);
}

#[test]
fn binop_equal_result() {
    check_expr("", "One == One", &expect!["true"]);
}

#[test]
fn binop_equal_result_false() {
    check_expr("", "One == Zero", &expect!["false"]);
}

#[test]
fn binop_equal_string() {
    check_expr("", r#""foo" == "foo""#, &expect!["true"]);
}

#[test]
fn binop_equal_string_false() {
    check_expr("", r#""foo" == "bar""#, &expect!["false"]);
}

#[test]
fn binop_equal_tuple() {
    check_expr("", "(1, 2, 3) == (1, 2, 3)", &expect!["true"]);
}

#[test]
fn binop_equal_tuple_false_content() {
    check_expr("", "(1, 2, 3) == (1, -2, 3)", &expect!["false"]);
}

#[test]
fn binop_exp_bigint() {
    check_expr("", "2L^3", &expect!["8"]);
}

#[test]
fn binop_exp_bigint_zero_exp() {
    check_expr("", "2L^0", &expect!["1"]);
}

#[test]
fn binop_exp_bigint_neg_zero_exp() {
    check_expr("", "(-2L)^0", &expect!["1"]);
}

#[test]
fn binop_exp_bigint_negative_exp() {
    check_expr(
        "",
        "2L^-3",
        &expect![[r#"
            InvalidNegativeInt(
                -3,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 3,
                        hi: 5,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_exp_bigint_too_large() {
    check_expr(
        "",
        "2L^9_223_372_036_854_775_807",
        &expect![[r#"
            IntTooLarge(
                9223372036854775807,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 3,
                        hi: 28,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_exp_double() {
    check_expr("", "2.3^3.1", &expect!["13.22380059125472"]);
}

#[test]
fn binop_exp_double_negative_exp() {
    check_expr("", "2.3^-3.1", &expect!["0.07562122501010253"]);
}

#[test]
fn binop_exp_int() {
    check_expr("", "2^3", &expect!["8"]);
}

#[test]
fn binop_exp_int_zero_exp() {
    check_expr("", "2^0", &expect!["1"]);
}

#[test]
fn binop_exp_int_neg_zero_exp() {
    check_expr("", "(-2)^0", &expect!["1"]);
}

#[test]
fn binop_exp_int_negative_exp() {
    check_expr(
        "",
        "2^-3",
        &expect![[r#"
            InvalidNegativeInt(
                -3,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 2,
                        hi: 4,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_exp_int_too_large() {
    check_expr(
        "",
        "100^50",
        &expect![[r#"
            IntTooLarge(
                50,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 4,
                        hi: 6,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_gt_bigint() {
    check_expr("", "23L > 3L", &expect!["true"]);
}

#[test]
fn binop_gt_bigint_false() {
    check_expr("", "2L > 3L", &expect!["false"]);
}

#[test]
fn binop_gt_int() {
    check_expr("", "23 > 3", &expect!["true"]);
}

#[test]
fn binop_gt_int_false() {
    check_expr("", "2 > 3", &expect!["false"]);
}

#[test]
fn binop_gt_double() {
    check_expr("", "2.3 > 0.3", &expect!["true"]);
}

#[test]
fn binop_gt_double_false() {
    check_expr("", "0.2 > 0.3", &expect!["false"]);
}

#[test]
fn binop_gte_bigint() {
    check_expr("", "23L >= 3L", &expect!["true"]);
}

#[test]
fn binop_gte_bigint_false() {
    check_expr("", "2L >= 3L", &expect!["false"]);
}

#[test]
fn binop_gte_bigint_eq() {
    check_expr("", "3L >= 3L", &expect!["true"]);
}

#[test]
fn binop_gte_int() {
    check_expr("", "23 >= 3", &expect!["true"]);
}

#[test]
fn binop_gte_int_false() {
    check_expr("", "2 >= 3", &expect!["false"]);
}

#[test]
fn binop_gte_int_eq() {
    check_expr("", "3 >= 3", &expect!["true"]);
}

#[test]
fn binop_gte_double() {
    check_expr("", "2.3 >= 0.3", &expect!["true"]);
}

#[test]
fn binop_gte_double_false() {
    check_expr("", "0.2 >= 0.3", &expect!["false"]);
}

#[test]
fn binop_gte_double_eq() {
    check_expr("", "0.3 >= 0.3", &expect!["true"]);
}

#[test]
fn binop_lt_bigint_false() {
    check_expr("", "23L < 3L", &expect!["false"]);
}

#[test]
fn binop_lt_bigint() {
    check_expr("", "2L < 3L", &expect!["true"]);
}

#[test]
fn binop_lt_int_false() {
    check_expr("", "23 < 3", &expect!["false"]);
}

#[test]
fn binop_lt_int() {
    check_expr("", "2 < 3", &expect!["true"]);
}

#[test]
fn binop_lt_double_false() {
    check_expr("", "2.3 < 0.3", &expect!["false"]);
}

#[test]
fn binop_lt_double() {
    check_expr("", "0.2 < 0.3", &expect!["true"]);
}

#[test]
fn binop_lte_bigint_false() {
    check_expr("", "23L <= 3L", &expect!["false"]);
}

#[test]
fn binop_lte_bigint() {
    check_expr("", "2L <= 3L", &expect!["true"]);
}

#[test]
fn binop_lte_bigint_eq() {
    check_expr("", "3L <= 3L", &expect!["true"]);
}

#[test]
fn binop_lte_int_false() {
    check_expr("", "23 <= 3", &expect!["false"]);
}

#[test]
fn binop_lte_int() {
    check_expr("", "2 <= 3", &expect!["true"]);
}

#[test]
fn binop_lte_int_eq() {
    check_expr("", "3 <= 3", &expect!["true"]);
}

#[test]
fn binop_lte_double_false() {
    check_expr("", "2.3 <= 0.3", &expect!["false"]);
}

#[test]
fn binop_lte_double() {
    check_expr("", "0.2 <= 0.3", &expect!["true"]);
}

#[test]
fn binop_lte_double_eq() {
    check_expr("", "0.3 <= 0.3", &expect!["true"]);
}

#[test]
fn binop_mod_bigint() {
    check_expr("", "8L % 6L", &expect!["2"]);
}

#[test]
fn binop_mod_bigint_zero() {
    check_expr(
        "",
        "12L % 0L",
        &expect![[r#"
            DivZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 6,
                        hi: 8,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_mod_int() {
    check_expr("", "8 % 6", &expect!["2"]);
}

#[test]
fn binop_mod_int_wrap() {
    check_expr("", "(-0x8000000000000000) % (-1)", &expect!["0"]);
}

#[test]
fn binop_mod_int_zero() {
    check_expr(
        "",
        "12 % 0",
        &expect![[r#"
            DivZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 5,
                        hi: 6,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_mod_double() {
    check_expr("", "8.411 % 6.833", &expect!["1.5779999999999994"]);
}

#[test]
fn binop_mod_double_zero() {
    check_expr(
        "",
        "1.2 % 0.0",
        &expect![[r#"
            DivZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 6,
                        hi: 9,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_mul_bigint() {
    check_expr("", "8L * 6L", &expect!["48"]);
}

#[test]
fn binop_mul_int() {
    check_expr("", "8 * 6", &expect!["48"]);
}
#[test]
fn binop_mul_int_wrap() {
    check_expr(
        "",
        "0x7FFFFFFFFFFFFFFF * 0xFF",
        &expect!["9223372036854775553"],
    );
}

#[test]
fn binop_mul_double() {
    check_expr("", "8.411 * 6.833", &expect!["57.472363"]);
}

#[test]
fn binop_neq_array() {
    check_expr("", "[1, 2, 3] != [1, 2, 3]", &expect!["false"]);
}

#[test]
fn binop_neq_array_true_content() {
    check_expr("", "[1, 2, 3] != [1, 0, 3]", &expect!["true"]);
}

#[test]
fn binop_neq_array_true_length() {
    check_expr("", "[1, 2, 3] != [1, 2, 3, 4]", &expect!["true"]);
}

#[test]
fn binop_neq_bigint() {
    check_expr("", "18L != 18L", &expect!["false"]);
}

#[test]
fn binop_neq_bigint_true() {
    check_expr("", "18L != 8L", &expect!["true"]);
}

#[test]
fn binop_neq_bool() {
    check_expr("", "false != false", &expect!["false"]);
}

#[test]
fn binop_neq_bool_true() {
    check_expr("", "false != true", &expect!["true"]);
}

#[test]
fn binop_neq_double() {
    check_expr("", "1.254 != 1.254", &expect!["false"]);
}

#[test]
fn binop_neq_double_true() {
    check_expr("", "1.254 != 1.25", &expect!["true"]);
}

#[test]
fn binop_neq_int() {
    check_expr("", "42 != 42", &expect!["false"]);
}

#[test]
fn binop_neq_int_true() {
    check_expr("", "42 != 43", &expect!["true"]);
}

#[test]
fn binop_neq_pauli() {
    check_expr("", "PauliX != PauliX", &expect!["false"]);
}

#[test]
fn binop_neq_pauli_true() {
    check_expr("", "PauliX != PauliZ", &expect!["true"]);
}

#[test]
fn binop_neq_range() {
    check_expr("", "(0..4) != (0..4)", &expect!["false"]);
}

#[test]
fn binop_neq_range_true() {
    check_expr("", "(0..2..4) != (0..4)", &expect!["true"]);
}

#[test]
fn binop_neq_result() {
    check_expr("", "One != One", &expect!["false"]);
}

#[test]
fn binop_neq_result_true() {
    check_expr("", "One != Zero", &expect!["true"]);
}

#[test]
fn binop_neq_string() {
    check_expr("", r#""foo" != "foo""#, &expect!["false"]);
}

#[test]
fn binop_neq_string_true() {
    check_expr("", r#""foo" != "bar""#, &expect!["true"]);
}

#[test]
fn binop_neq_tuple() {
    check_expr("", "(1, 2, 3) != (1, 2, 3)", &expect!["false"]);
}

#[test]
fn binop_neq_tuple_true_content() {
    check_expr("", "(1, 2, 3) != (1, -2, 3)", &expect!["true"]);
}

#[test]
fn binop_orb_bigint() {
    check_expr("", "28L ||| 54L", &expect!["62"]);
}

#[test]
fn binop_orb_int() {
    check_expr("", "28 ||| 54", &expect!["62"]);
}

#[test]
fn binop_orl() {
    check_expr("", "true or true", &expect!["true"]);
}

#[test]
fn binop_orl_true_lhs() {
    check_expr("", "true or false", &expect!["true"]);
}

#[test]
fn binop_orl_true_rhs() {
    check_expr("", "false or true", &expect!["true"]);
}

#[test]
fn binop_orl_false() {
    check_expr("", "false or false", &expect!["false"]);
}

#[test]
fn binop_orl_shortcut() {
    check_expr("", r#"true or (fail "Shouldn't Fail")"#, &expect!["true"]);
}

#[test]
fn binop_shl_bigint() {
    check_expr("", "4L <<< 2", &expect!["16"]);
}

#[test]
fn binop_shl_bigint_negative() {
    check_expr("", "4L <<< -2", &expect!["1"]);
}

#[test]
fn binop_shl_int() {
    check_expr("", "4 <<< 2", &expect!["16"]);
}

#[test]
fn binop_shl_int_negative() {
    check_expr("", "4 <<< -2", &expect!["1"]);
}

#[test]
fn binop_shl_int_truncate() {
    check_expr("", "1 <<< 63", &expect!["-9223372036854775808"]);
    check_expr("", "2 <<< 63", &expect!["0"]);
}

#[test]
fn binop_shl_int_overflow() {
    check_expr(
        "",
        "1 <<< 64",
        &expect![[r#"
            IntTooLarge(
                64,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 6,
                        hi: 8,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_shr_bigint() {
    check_expr("", "4L >>> 2", &expect!["1"]);
}

#[test]
fn binop_shr_bigint_negative() {
    check_expr("", "4L >>> -2", &expect!["16"]);
}

#[test]
fn binop_shr_int() {
    check_expr("", "4 >>> 2", &expect!["1"]);
}

#[test]
fn binop_shr_int_negative() {
    check_expr("", "4 >>> -2", &expect!["16"]);
}

#[test]
fn binop_shr_int_truncate() {
    check_expr("", "(-9223372036854775808) >>> 63", &expect!["-1"]);
    check_expr("", "1 >>> 63", &expect!["0"]);
}

#[test]
fn binop_shr_int_overflow() {
    check_expr(
        "",
        "1 >>> 64",
        &expect![[r#"
            IntTooLarge(
                64,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 6,
                        hi: 8,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn binop_sub_bigint() {
    check_expr("", "4L - 2L", &expect!["2"]);
}

#[test]
fn binop_sub_int() {
    check_expr("", "4 - 2", &expect!["2"]);
}

#[test]
fn binop_sub_int_wrap() {
    check_expr(
        "",
        "-0x8000000000000000 - 1",
        &expect!["9223372036854775807"],
    );
}

#[test]
fn binop_sub_double() {
    check_expr("", "4.7 - 2.5", &expect!["2.2"]);
}

#[test]
fn binop_xorb_bigint() {
    check_expr("", "28L ^^^ 54L", &expect!["42"]);
}

#[test]
fn binop_xorb_int() {
    check_expr("", "28 ^^^ 54", &expect!["42"]);
}

#[test]
fn assignop_add_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x += 1;
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn assignop_add_concat() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2];
            set x += [3, 4];
            x
        }"},
        &expect!["[1, 2, 3, 4]"],
    );
}

#[test]
fn assignop_add_concat_copy() {
    check_expr(
        "",
        indoc! {"{
            let x = [1, 2];
            mutable y = x;
            set y += [3, 4];
            (x, y)
        }"},
        &expect!["([1, 2], [1, 2, 3, 4])"],
    );
}

#[test]
fn assignop_sub_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x -= 1;
            x
        }"},
        &expect!["-1"],
    );
}

#[test]
fn assignop_orl_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = false;
            set x or= true;
            x
        }"},
        &expect!["true"],
    );
}

#[test]
fn fail_expr() {
    check_expr(
        "",
        r#"fail "This is a failure""#,
        &expect![[r#"
            UserFail(
                "This is a failure",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 0,
                        hi: 24,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn fail_shortcut_expr() {
    check_expr(
        "",
        r#"{ fail "Got Here!"; fail "Shouldn't get here..."; }"#,
        &expect![[r#"
            UserFail(
                "Got Here!",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 2,
                        hi: 18,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn field_range_start_expr() {
    check_expr("", "(0..2..8)::Start", &expect!["0"]);
}

#[test]
fn field_range_step_expr() {
    check_expr("", "(0..2..8)::Step", &expect!["2"]);
}

#[test]
fn field_range_step_missing_treated_as_1_expr() {
    check_expr("", "(0..8)::Step", &expect!["1"]);
}

#[test]
fn field_range_end_expr() {
    check_expr("", "(0..2..8)::End", &expect!["8"]);
}

#[test]
fn for_loop_range_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for i in 0..10 {
                set x = x + i;
            }
            x
        }"},
        &expect!["55"],
    );
}

#[test]
fn for_loop_array_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for i in [5, size = 5] {
                set x = x + i;
            }
            x
        }"},
        &expect!["25"],
    );
}

#[test]
fn for_loop_ignore_iterator_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for _ in [5, size = 5] {
                set x = x + 1;
            }
            x
        }"},
        &expect!["5"],
    );
}

#[test]
fn array_index_expr() {
    check_expr("", "[1, 2, 3][1]", &expect!["2"]);
}

#[test]
fn array_slice_start_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..2]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_slice_start_step_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..2..2]", &expect!["[1, 3]"]);
}

#[test]
fn array_slice_start_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2...]", &expect!["[3, 4, 5]"]);
}

#[test]
fn array_slice_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_slice_step_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2..3]", &expect!["[1, 3]"]);
}

#[test]
fn array_slice_step_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2...]", &expect!["[1, 3, 5]"]);
}

#[test]
fn array_slice_reverse_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2..-1..0]", &expect!["[3, 2, 1]"]);
}

#[test]
fn array_slice_reverse_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...-1..2]", &expect!["[5, 4, 3]"]);
}

#[test]
fn array_slice_reverse_start_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2..-1...]", &expect!["[3, 2, 1]"]);
}

#[test]
fn array_slice_reverse_all_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...-1...]", &expect!["[5, 4, 3, 2, 1]"]);
}

#[test]
fn array_slice_all_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...]", &expect!["[1, 2, 3, 4, 5]"]);
}

#[test]
fn array_slice_none_expr() {
    check_expr("", "[1, 2, 3, 4, 5][1..0]", &expect!["[]"]);
}

#[test]
fn array_slice_reverse_none_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..-1..1]", &expect!["[]"]);
}

#[test]
fn array_slice_step_zero_expr() {
    check_expr(
        "",
        "[1, 2, 3, 4, 5][...0...]",
        &expect![[r#"
            RangeStepZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 16,
                        hi: 23,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn array_slice_out_of_range_expr() {
    check_expr(
        "",
        "[1, 2, 3, 4, 5][0..7]",
        &expect![[r#"
            IndexOutOfRange(
                5,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 16,
                        hi: 20,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn array_index_negative_expr() {
    check_expr("", "[1, 2, 3][-2]", &expect!["2"]);
}

#[test]
fn array_index_out_of_range_expr() {
    check_expr(
        "",
        "[1, 2, 3][4]",
        &expect![[r#"
            IndexOutOfRange(
                4,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 10,
                        hi: 11,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn array_index_out_of_range_with_length_expr() {
    check_expr(
        "",
        "[1, 2, 3][3]",
        &expect![[r#"
            IndexOutOfRange(
                3,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 10,
                        hi: 11,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn array_index_out_of_range_with_negative_length_minus_one_expr() {
    check_expr(
        "",
        "[1, 2, 3][-4]",
        &expect![[r#"
            IndexOutOfRange(
                -4,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 10,
                        hi: 12,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn literal_big_int_expr() {
    check_expr(
        "",
        "9_223_372_036_854_775_808L",
        &expect!["9223372036854775808"],
    );
}

#[test]
fn literal_bool_false_expr() {
    check_expr("", "false", &expect!["false"]);
}

#[test]
fn literal_bool_true_expr() {
    check_expr("", "true", &expect!["true"]);
}

#[test]
fn literal_double_expr() {
    check_expr("", "4.2", &expect!["4.2"]);
}

#[test]
fn literal_double_trailing_dot_expr() {
    check_expr("", "4.", &expect!["4.0"]);
}

#[test]
fn literal_int_expr() {
    check_expr("", "42", &expect!["42"]);
}

#[test]
fn literal_int_too_big_expr() {
    check_expr(
        "",
        "9_223_372_036_854_775_808",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn literal_pauli_i_expr() {
    check_expr("", "PauliI", &expect!["PauliI"]);
}

#[test]
fn literal_pauli_x_expr() {
    check_expr("", "PauliX", &expect!["PauliX"]);
}

#[test]
fn literal_pauli_y_expr() {
    check_expr("", "PauliY", &expect!["PauliY"]);
}

#[test]
fn literal_pauli_z_expr() {
    check_expr("", "PauliZ", &expect!["PauliZ"]);
}

#[test]
fn literal_result_one_expr() {
    check_expr("", "One", &expect!["One"]);
}

#[test]
fn literal_result_zero_expr() {
    check_expr("", "Zero", &expect!["Zero"]);
}

#[test]
fn literal_string_expr() {
    check_expr("", r#""foo""#, &expect!["foo"]);
}

#[test]
fn literal_tuple_expr() {
    check_expr("", "(1, 2, 3)", &expect!["(1, 2, 3)"]);
}

#[test]
fn literal_tuple_singleton_expr() {
    check_expr("", "(1,)", &expect!["(1,)"]);
}

#[test]
fn literal_tuple_mixed_expr() {
    check_expr(
        "",
        "(1, One, 1.0, [1, 2, 3])",
        &expect!["(1, One, 1.0, [1, 2, 3])"],
    );
}

#[test]
fn paren_expr() {
    check_expr("", "(42)", &expect!["42"]);
}

#[test]
fn range_all_expr() {
    check_expr("", "...", &expect!["..."]);
}

#[test]
fn range_end_expr() {
    check_expr("", "...3", &expect!["...3"]);
}

#[test]
fn range_step_end_expr() {
    check_expr("", "...2..3", &expect!["...2..3"]);
}

#[test]
fn range_start_expr() {
    check_expr("", "1...", &expect!["1..."]);
}

#[test]
fn range_start_end_expr() {
    check_expr("", "1..3", &expect!["1..3"]);
}

#[test]
fn range_start_step_expr() {
    check_expr("", "1..2...", &expect!["1..2..."]);
}

#[test]
fn range_start_step_end_expr() {
    check_expr("", "1..2..3", &expect!["1..2..3"]);
}

#[test]
fn repeat_until_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            repeat {
                set x = x + 1;
            }
            until x >= 3;
            x
        }"},
        &expect!["3"],
    );
}

#[test]
fn repeat_until_fixup_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            repeat {}
            until x >= 3
            fixup {
                set x = x + 1;
            }
            x
        }"},
        &expect!["3"],
    );
}

#[test]
fn return_expr() {
    check_expr("", "return 4", &expect!["4"]);
}

#[test]
fn return_shortcut_expr() {
    check_expr(
        "",
        r#"{return 4; fail "Shouldn't get here...";}"#,
        &expect!["4"],
    );
}

#[test]
fn tuple_expr() {
    check_expr("", "(1, 2, 3)", &expect!["(1, 2, 3)"]);
}

#[test]
fn unop_bitwise_not_big_int_expr() {
    check_expr(
        "",
        "~~~(9_223_372_036_854_775_808L)",
        &expect!["-9223372036854775809"],
    );
}

#[test]
fn while_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            while x < 10 {
                set x = x + 1;
            }
            x
        }"},
        &expect!["10"],
    );
}

#[test]
fn while_false_shortcut_expr() {
    check_expr(
        "",
        r#"while false { fail "Shouldn't fail" }"#,
        &expect!["()"],
    );
}

#[test]
fn cond_expr() {
    check_expr("", "true ? 1 | 0", &expect!["1"]);
}

#[test]
fn cond_false_expr() {
    check_expr("", "false ? 1 | 0", &expect!["0"]);
}

#[test]
fn cond_shortcircuit_expr() {
    check_expr("", r#"true ? 1 | fail "Shouldn't fail""#, &expect!["1"]);
}

#[test]
fn cond_false_shortcircuit_expr() {
    check_expr("", r#"false ? fail "Shouldn't fail" | 0"#, &expect!["0"]);
}

#[test]
fn update_expr() {
    check_expr("", "[1, 2, 3] w/ 2 <- 4", &expect!["[1, 2, 4]"]);
}

#[test]
fn update_invalid_index_range_expr() {
    check_expr(
        "",
        "[1, 2, 3] w/ 7 <- 4",
        &expect![[r#"
            IndexOutOfRange(
                7,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 13,
                        hi: 14,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn update_invalid_index_negative_expr() {
    check_expr(
        "",
        "[1, 2, 3] w/ -1 <- 4",
        &expect![[r#"
            InvalidNegativeInt(
                -1,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 13,
                        hi: 15,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn update_array_index_var() {
    check_expr(
        "",
        indoc! {"{
            let xs = [2];
            let i = 0;
            xs w/ i <- 3
        }"},
        &expect!["[3]"],
    );
}

#[test]
fn update_array_index_expr() {
    check_expr(
        "",
        indoc! {"{
            let xs = [1, 2];
            let i = 0;
            xs w/ i + 1 <- 3
        }"},
        &expect!["[1, 3]"],
    );
}

#[test]
fn struct_cons() {
    check_expr(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
            }
        "},
        indoc! {"{
            open A;
            new Pair { First = 1, Second = 2}
        }"},
        &expect!["(1, 2)"],
    );
}

#[test]
fn struct_copy_cons() {
    check_expr(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
            }
        "},
        indoc! {"{
            open A;
            let p = new Pair { First = 1, Second = 2};
            new Pair { ...p, First = 3 }
        }"},
        &expect!["(3, 2)"],
    );
}

#[test]
fn update_udt_known_field_name() {
    check_expr(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);
            }
        "},
        indoc! {"{
            open A;
            let p = Pair(1, 2);
            p w/ First <- 3
        }"},
        &expect!["(3, 2)"],
    );
}

#[test]
fn update_udt_nested_field() {
    check_expr(
        indoc! {"
            namespace A {
                newtype Triple = (First : Int, (Second : Int, Third : Int));
            }
        "},
        indoc! {"{
            open A;
            let p = Triple(1, (2, 3));
            p w/ Third <- 4
        }"},
        &expect!["(1, (2, 4))"],
    );
}

#[test]
fn update_range_start() {
    check_expr("", "1..2..3 w/ Start <- 10", &expect!["10..2..3"]);
}

#[test]
fn update_range_from_start() {
    check_expr("", "1..2... w/ Start <- 10", &expect!["10..2..."]);
}

#[test]
fn update_range_step() {
    check_expr("", "1..2..3 w/ Step <- 10", &expect!["1..10..3"]);
}

#[test]
fn update_range_from_step() {
    check_expr("", "1..2... w/ Step <- 10", &expect!["1..10..."]);
}

#[test]
fn update_range_to_step() {
    check_expr("", "...2..3 w/ Step <- 10", &expect!["...10..3"]);
}

#[test]
fn update_range_full_step() {
    check_expr("", "...2... w/ Step <- 10", &expect!["...10..."]);
}

#[test]
fn update_range_end() {
    check_expr("", "1..2..3 w/ End <- 10", &expect!["1..2..10"]);
}

#[test]
fn update_range_to_end() {
    check_expr("", "...2..3 w/ End <- 10", &expect!["...2..10"]);
}

#[test]
fn update_array_with_range() {
    check_expr("", "[0, 1, 2] w/ 1..2 <- [10, 11]", &expect!["[0, 10, 11]"]);
}

#[test]
fn update_array_with_range_start() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1... <- [10, 11]",
        &expect!["[0, 10, 11, 3]"],
    );
}

#[test]
fn update_array_with_range_step() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ ...2... <- [10, 11]",
        &expect!["[10, 1, 11, 3]"],
    );
}

#[test]
fn update_array_with_range_end() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ ...2 <- [10, 11, 12, 13]",
        &expect!["[10, 11, 12, 3]"],
    );
}

#[test]
fn update_array_with_range_fully_open() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ ... <- [10, 11, 12, 13]",
        &expect!["[10, 11, 12, 13]"],
    );
}

#[test]
fn update_array_with_range_reverse() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 2..-1..0 <- [10, 11]",
        &expect!["[0, 11, 10, 3]"],
    );
}

#[test]
fn update_array_with_range_out_of_range_err() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1..5 <- [10, 11, 12, 13]",
        &expect![[r#"
            IndexOutOfRange(
                4,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 16,
                        hi: 20,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn update_array_with_range_negative_index() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ -1..0 <- [10, 11, 12, 13]",
        &expect!["[11, 1, 2, 10]"],
    );
}

#[test]
fn update_array_with_range_zero_step_err() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ ...0... <- [10, 11, 12, 13]",
        &expect![[r#"
            RangeStepZero(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 16,
                        hi: 23,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn update_array_with_range_bigger_than_update() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1..3 <- [10]",
        &expect!["[0, 10, 2, 3]"],
    );
}

#[test]
fn update_array_with_range_smaller_than_update() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1..3 <- [10, 11, 12, 13]",
        &expect!["[0, 10, 11, 12]"],
    );
}

#[test]
fn update_array_with_range_open_ended_bigger_than_update() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1... <- [10]",
        &expect!["[0, 10, 2, 3]"],
    );
}

#[test]
fn update_array_with_range_open_ended_smaller_than_update() {
    check_expr(
        "",
        "[0, 1, 2, 3] w/ 1... <- [10, 11, 12, 13]",
        &expect!["[0, 10, 11, 12]"],
    );
}

#[test]
fn update_array_with_range_empty_update() {
    check_expr("", "[0, 1, 2, 3] w/ 1..3 <- []", &expect!["[0, 1, 2, 3]"]);
}

#[test]
fn assignupdate_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3];
            set x w/= 2 <- 4;
            x
        }"},
        &expect!["[1, 2, 4]"],
    );
}

#[test]
fn assignupdate_on_copy_should_work() {
    check_expr(
        "",
        indoc! {"{
            let x = [1, 2, 3];
            mutable y = x;
            set y w/= 2 <- 4;
            (x, y)
        }"},
        &expect!["([1, 2, 3], [1, 2, 4])"],
    );
}

#[test]
fn assignupdate_out_of_range_err() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3];
            set x w/= 4 <- 4;
            x
        }"},
        &expect![[r#"
            IndexOutOfRange(
                4,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 43,
                        hi: 44,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn assignupdate_expr_negative_index_err() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3];
            set x w/= -1 <- 4;
            x
        }"},
        &expect![[r#"
            InvalidNegativeInt(
                -1,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 43,
                        hi: 45,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn assignupdate_expr_using_field_name() {
    check_expr(
        indoc! {"
        namespace A {
            newtype Pair = (First : Int, Second : Int);
        }
    "},
        indoc! {"{
            open A;
            mutable p = Pair(1, 2);
            set p w/= First <- 3;
            p
        }"},
        &expect!["(3, 2)"],
    );
}

#[test]
fn assignupdate_expr_using_range() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3];
            set x w/= 1..2 <- [10, 11];
            x
        }"},
        &expect!["[1, 10, 11]"],
    );
}

#[test]
fn assignupdate_expr_using_range_start() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1... <- [10, 11];
            x
        }"},
        &expect!["[1, 10, 11, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_step() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= ...2... <- [10, 11];
            x
        }"},
        &expect!["[10, 2, 11, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_end() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= ...2 <- [10, 11, 12, 13];
            x
        }"},
        &expect!["[10, 11, 12, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_fully_open() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= ... <- [10, 11, 12, 13];
            x
        }"},
        &expect!["[10, 11, 12, 13]"],
    );
}

#[test]
fn assignupdate_expr_using_range_reverse() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 2..-1..0 <- [10, 11];
            x
        }"},
        &expect!["[1, 11, 10, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_bigger_than_update() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1..3 <- [10];
            x
        }"},
        &expect!["[1, 10, 3, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_smaller_than_update() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1..3 <- [10, 11, 12, 13];
            x
        }"},
        &expect!["[1, 10, 11, 12]"],
    );
}

#[test]
fn assignupdate_expr_using_range_open_ended_bigger_than_update() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1... <- [10, 11];
            x
        }"},
        &expect!["[1, 10, 11, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_open_ended_smaller_than_update() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1... <- [10, 11, 12, 13];
            x
        }"},
        &expect!["[1, 10, 11, 12]"],
    );
}

#[test]
fn assignupdate_expr_using_range_empty_update() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1..3 <- [];
            x
        }"},
        &expect!["[1, 2, 3, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_range_out_of_range_err() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= 1..5 <- [10, 11, 12, 13];
            x
        }"},
        &expect![[r#"
            IndexOutOfRange(
                4,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 46,
                        hi: 50,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn assignupdate_expr_using_range_negative_index_err() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3, 4];
            set x w/= -1..0 <- [10, 11, 12, 13];
            x
        }"},
        &expect![[r#"
            InvalidNegativeInt(
                -1,
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 46,
                        hi: 51,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn unop_bitwise_not_int_expr() {
    check_expr("", "~~~(13)", &expect!["-14"]);
}

#[test]
fn unop_negate_big_int_expr() {
    check_expr(
        "",
        "-(9_223_372_036_854_775_808L)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn unop_negate_double_expr() {
    check_expr("", "-(3.4)", &expect!["-3.4"]);
}

#[test]
fn unop_negate_int_expr() {
    check_expr("", "-(13)", &expect!["-13"]);
}

#[test]
fn unop_negate_int_overflow_expr() {
    check_expr(
        "",
        "-(9_223_372_036_854_775_808)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn unop_negate_negative_int_expr() {
    check_expr("", "-(-(13))", &expect!["13"]);
}

#[test]
fn unop_not_bool_expr() {
    check_expr("", "not false", &expect!["true"]);
}

#[test]
fn unop_positive_big_int_expr() {
    check_expr(
        "",
        "+(9_223_372_036_854_775_808L)",
        &expect!["9223372036854775808"],
    );
}

#[test]
fn unop_positive_double_expr() {
    check_expr("", "+(3.4)", &expect!["3.4"]);
}

#[test]
fn unop_positive_int_expr() {
    check_expr("", "+(13)", &expect!["13"]);
}

#[test]
fn unop_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint Test.Foo",
        &expect!["Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled Test.Foo",
        &expect!["Controlled <item 1 in package 2>"],
    );
}

#[test]
fn unop_adjoint_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint (Adjoint Test.Foo)",
        &expect!["<item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled Adjoint Test.Foo",
        &expect!["Controlled Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_adjoint_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint Controlled Test.Foo",
        &expect!["Controlled Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled (Controlled Test.Foo)",
        &expect!["Controlled Controlled <item 1 in package 2>"],
    );
}

#[test]
fn if_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";}"#,
        &expect!["()"],
    );
}

#[test]
fn if_else_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_else_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} else {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_true_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} elif true {return"Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_false_true_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif true {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_false_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif false {return "Shouldn't get here..."}"#,
        &expect!["()"],
    );
}

#[test]
fn if_elif_else_true_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} elif true {return "Shouldn't get here..."} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_else_false_true_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif true {return "Got Here!"} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_else_false_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif false {return "Shouldn't get here..."} else {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn call_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Answer() : Int {
                    42
                }
            }
        "},
        "Test.Answer()",
        &expect!["42"],
    );
}

#[test]
fn call_return_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Answer() : Int {
                    return 42;
                }
            }
        "},
        "Test.Answer()",
        &expect!["42"],
    );
}

#[test]
fn call_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Echo(val : Int) : Int {
                    return val;
                }
            }
        "},
        "Test.Echo(42)",
        &expect!["42"],
    );
}

#[test]
fn call_multiple_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Echo(val1 : Int, val2 : Int) : (Int, Int) {
                    return (val1, val2);
                }
            }
        "},
        "Test.Echo(42, 43)",
        &expect!["(42, 43)"],
    );
}

#[test]
fn call_tuple_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function MakeList(val1 : (Int, Int), val2 : Int) : Int[] {
                    let (v1, v2) = val1;
                    return [v1, v2, val2];
                }
            }
        "},
        "Test.MakeList((42, 43), 44)",
        &expect!["[42, 43, 44]"],
    );
}

#[test]
fn call_call_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function TupleToList(tup : (Int, Int)) : Int[] {
                    let (val, size) = tup;
                    return MakeList(val, size);
                }
                function MakeList(val : Int, size : Int) : Int[] {
                    return [val, size = size];
                }
            }
        "},
        "Test.TupleToList((3, 2))",
        &expect!["[3, 3]"],
    );
}

#[test]
fn call_adjoint_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint (...) {
                        fail "Adjoint Implementation";
                    }
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                    controlled adjoint (ctls, ...) {
                        fail "Controlled Adjoint Implementation";
                    }
                }
            }
        "#},
        "Adjoint Test.Foo()",
        &expect![[r#"
            UserFail(
                "Adjoint Implementation",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 185,
                        hi: 214,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn call_adjoint_adjoint_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint (...) {
                        fail "Adjoint Implementation";
                    }
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                    controlled adjoint (ctls, ...) {
                        fail "Controlled Adjoint Implementation";
                    }
                }
            }
        "#},
        "Adjoint Adjoint Test.Foo()",
        &expect![[r#"
            UserFail(
                "Body Implementation",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 119,
                        hi: 145,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn call_adjoint_self_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint self;
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                }
            }
        "#},
        "Adjoint Test.Foo()",
        &expect![[r#"
            UserFail(
                "Body Implementation",
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 111,
                        hi: 137,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn check_ctls_count_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs = Qubit[3];
                Controlled Test.Foo(qs, ());
            }
        "},
        &expect!["()"],
    );
}

#[test]
fn check_ctls_count_nested_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs1 = Qubit[1];
                use qs2 = Qubit[2];
                Controlled Controlled Test.Foo(qs2, (qs1, ()));
            }
        "},
        &expect!["()"],
    );
}

#[test]
fn check_generated_ctl_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl {
                    body ... {}
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl {
                    A();
                }
            }
        "#},
        "{use qs = Qubit[3]; Controlled Test.B(qs, ())}",
        &expect!["()"],
    );
}

#[test]
fn check_generated_ctladj_distrib_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl + Adj {
                    body ... { fail "Shouldn't get here"; }
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                    controlled adjoint (ctls, ...) {
                        if Length(ctls) != 2 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl + Adj {
                    body ... { A(); }
                    adjoint ... { Adjoint A(); }
                }
            }
        "#},
        "{use qs = Qubit[2]; Controlled Adjoint Test.B(qs, ())}",
        &expect!["()"],
    );
}

#[test]
fn global_callable_as_arg() {
    check_expr(
        indoc! {"
            namespace Test {
                function PlusOne(x : Int) : Int {
                    x + 1
                }
                function ApplyToIntArray(f : (Int -> Int)) : Int[] {
                    mutable arr = [1, size = 3];
                    for i in 0..2 {
                        set arr w/= i <- f(arr[i]);
                    }
                    arr
                }
            }
        "},
        "Test.ApplyToIntArray(Test.PlusOne)",
        &expect!["[2, 2, 2]"],
    );
}

#[test]
fn conjugate_output_preserved() {
    check_expr("", "{let x = within{}apply{4}; x}", &expect!["4"]);
}

#[test]
fn interpolated_string() {
    check_expr("", r#"$"string""#, &expect!["string"]);
}

#[test]
fn interpolated_string_var() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 5;
            $"{x}"
        }"#},
        &expect!["5"],
    );
}

#[test]
fn interpolated_string_array_index() {
    check_expr(
        "",
        indoc! {r#"{
            let xs = [1, 2, 3];
            $"{xs[0]}"
        }"#},
        &expect!["1"],
    );
}

#[test]
fn interpolated_string_two_vars() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 4;
            let y = (true, Zero);
            $"{x} {y}"
        }"#},
        &expect!["4 (true, Zero)"],
    );
}

#[test]
fn interpolated_string_nested_normal_string() {
    check_expr("", r#"$"{"{}"}""#, &expect!["{}"]);
}

#[test]
fn nested_interpolated_string() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 4;
            $"{$"{x}"}"
        }"#},
        &expect!["4"],
    );
}

#[test]
fn nested_interpolated_string_with_exprs() {
    check_expr(
        "",
        indoc! {r#"{
            let x = "hello!";
            let y = 1.5;
            $"foo {x + $"bar {y}"} baz"
        }"#},
        &expect!["foo hello!bar 1.5 baz"],
    );
}

#[test]
fn udt_unwrap() {
    check_expr(
        "",
        "{
            newtype Foo = (Int, Bool);
            let foo = Foo(1, true);
            foo!
        }",
        &expect!["(1, true)"],
    );
}

#[test]
fn udt_fields() {
    check_expr(
        "",
        "{
            newtype Point = (X : Int, Y : Int);
            let p = Point(1, 2);
            (p::X, p::Y)
        }",
        &expect!["(1, 2)"],
    );
}

#[test]
fn udt_field_nested() {
    check_expr(
        "",
        "{
            newtype Point = (X : Int, (Y : Int, Z : Int));
            let p = Point(1, (2, 3));
            (p::Y, p::Z)
        }",
        &expect!["(2, 3)"],
    );
}

#[test]
fn lambda_function_empty_closure() {
    check_expr("", "{ let f = x -> x + 1; f(1) }", &expect!["2"]);
}

#[test]
fn lambda_function_empty_closure_passed() {
    check_expr(
        "",
        "{ function Foo(f : Int -> Int) : Int { f(2) }; Foo(x -> x + 1) }",
        &expect!["3"],
    );
}

#[test]
fn lambda_function_closure() {
    check_expr(
        "",
        "{ let x = 5; let f = y -> (x, y); f(2) }",
        &expect!["(5, 2)"],
    );
}

#[test]
fn lambda_function_closure_passed() {
    check_expr(
        "",
        "{ function Foo(f : Int -> (Int, Int)) : (Int, Int) { f(2) }; let x = 5; Foo(y -> (x, y)) }",
        &expect!["(5, 2)"],
    );
}

#[test]
fn lambda_function_nested_closure() {
    check_expr(
        "
            namespace A {
                function Foo(f : Int -> Int -> (Int, Int, Int, Int)) : (Int, Int, Int, Int) {
                    f(2)(3)
                }

                function Bar() : (Int, Int, Int, Int) {
                    let a = 5;
                    Foo(b -> {
                        let c = 1;
                        d -> (a, b, c, d)
                    })
                }
            }
        ",
        "A.Bar()",
        &expect!["(5, 2, 1, 3)"],
    );
}

#[test]
fn lambda_operation_empty_closure() {
    check_expr(
        "
            namespace A {
                operation Foo(op : Qubit => ()) : Result {
                    use q = Qubit();
                    op(q);
                    MResetZ(q)
                }

                operation Bar() : Result { Foo(q => X(q)) }
            }
        ",
        "A.Bar()",
        &expect!["One"],
    );
}

#[test]
fn lambda_operation_closure() {
    check_expr(
        "
            namespace A {
                operation Foo(op : () => Result) : Result { op() }
                operation Bar() : Result {
                    use q = Qubit();
                    X(q);
                    Foo(() => MResetZ(q))
                }
            }
        ",
        "A.Bar()",
        &expect!["One"],
    );
}

#[test]
fn lambda_operation_controlled() {
    check_expr(
        "
            namespace A {
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls, q) = (Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    X(ctls[0]);
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    ResetAll(ctls);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect!["[One, Zero, One]"],
    );
}

#[test]
fn lambda_operation_controlled_controlled() {
    check_expr(
        "
            namespace A {
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls1, ctls2, q) = (Qubit[1], Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    X(ctls1[0]);
                    X(ctls2[0]);
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    ResetAll(ctls1 + ctls2);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect!["[One, Zero, One]"],
    );
}

#[test]
fn partial_app_all_holes() {
    check_expr(
        "",
        "{
            function F(x : Int, y : Int) : Int { x + y }
            let f = F(_, _);
            f(1, 2)
        }",
        &expect!["3"],
    );
}

#[test]
fn partial_app_one_fixed_arg() {
    check_expr(
        "",
        "{
            function F(x : Int, y : Int) : Int { x + y }
            let f = F(_, 2);
            f(1)
        }",
        &expect!["3"],
    );
}

#[test]
fn partial_app_nested_tuple() {
    check_expr(
        "",
        "{
            function F(a : Int, (b : Int, c : Int, d : Int)) : (Int, Int, Int, Int) { (a, b, c, d) }
            let f = F(_, (_, 3, _));
            f(1, (2, 4))
        }",
        &expect!["(1, 2, 3, 4)"],
    );
}

#[test]
fn partial_app_arg_with_side_effect() {
    check_expr(
        "",
        "{
            operation F(_ : (), x : Int) : Int { x }
            use q = Qubit();
            let f = F(X(q), _);
            let r1 = M(q);
            f(1);
            let r2 = M(q);
            f(2);
            let r3 = M(q);
            Reset(q);
            (r1, r2, r3)
        }",
        &expect!["(One, One, One)"],
    );
}

#[test]
fn partial_app_mutable_arg() {
    check_expr(
        "",
        "{
            function F(a : Int, b : Int) : (Int, Int) { (a, b) }
            mutable x = 0;
            let f = F(x, _);
            let r1 = f(1);
            set x = 1;
            let r2 = f(2);
            (r1, r2)
        }",
        &expect!["((0, 1), (0, 2))"],
    );
}

#[test]
fn controlled_operation_with_duplicate_controls_fails() {
    check_expr(
        "",
        "{
            use ctl = Qubit();
            use q = Qubit();
            Controlled I([ctl, ctl], q);
        }",
        &expect![[r#"
            QubitUniqueness(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 86,
                        hi: 101,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn controlled_operation_with_target_in_controls_fails() {
    check_expr(
        "",
        "{
            use ctl = Qubit();
            use q = Qubit();
            Controlled I([ctl, q], q);
        }",
        &expect![[r#"
            QubitUniqueness(
                PackageSpan {
                    package: PackageId(
                        2,
                    ),
                    span: Span {
                        lo: 86,
                        hi: 99,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn controlled_operation_with_unique_controls_duplicate_targets_allowed() {
    check_expr(
        "",
        "{
            operation DoubleI(q0 : Qubit, q1 : Qubit) : Unit is Ctl {
                I(q0);
                I(q1);
            }
            use ctl = Qubit();
            use q = Qubit();
            Controlled DoubleI([ctl], (q, q));
        }",
        &expect!["()"],
    );
}

#[test]
fn complex_literal_double_first_with_plus_expr() {
    check_expr("", "3.0 + 4.0i", &expect!["(3.0, 4.0)"]);
}

#[test]
fn complex_literal_double_second_with_plus_expr() {
    check_expr("", "4.0i + 3.0", &expect!["(3.0, 4.0)"]);
}

#[test]
fn complex_literal_double_first_with_minus_expr() {
    check_expr("", "3.0 - 4.0i", &expect!["(3.0, -4.0)"]);
}

#[test]
fn complex_literal_double_second_with_minus_expr() {
    check_expr("", "4.0i - 3.0", &expect!["(-3.0, 4.0)"]);
}

#[test]
fn complex_literal_double_first_with_negate_and_plus_expr() {
    check_expr("", "-3.0 + 4.0i", &expect!["(-3.0, 4.0)"]);
}

#[test]
fn complex_literal_double_first_with_negate_and_minus_expr() {
    check_expr("", "-3.0 - 4.0i", &expect!["(-3.0, -4.0)"]);
}

#[test]
fn complex_literal_double_second_with_negate_and_plus_expr() {
    check_expr("", "-4.0i + 3.0", &expect!["(3.0, -4.0)"]);
}

#[test]
fn complex_literals_support_equality() {
    check_expr("", "(3.0 + 4.0i) == (3.0 + 4.0i)", &expect!["true"]);
    check_expr("", "(3.0 + 4.0i) != (3.0 + 4.0i)", &expect!["false"]);
    check_expr("", "(3.0 + 4.0i) == (3.0 - 4.0i)", &expect!["false"]);
    check_expr("", "(3.0 + 4.0i) != (3.0 - 4.0i)", &expect!["true"]);
}

#[test]
fn complex_literal_supports_addition() {
    check_expr("", "(3.0 + 4.0i) + (1.0 + 2.0i)", &expect!["(4.0, 6.0)"]);
}

#[test]
fn complex_literal_supports_subtraction() {
    check_expr("", "(3.0 + 4.0i) - (1.0 + 2.0i)", &expect!["(2.0, 2.0)"]);
}

#[test]
fn complex_literal_supports_multiplication() {
    check_expr("", "(3.0 + 4.0i) * (1.0 + 2.0i)", &expect!["(-5.0, 10.0)"]);
}

#[test]
fn complex_literal_supports_division() {
    check_expr("", "(3.0 + 4.0i) / (1.0 + 2.0i)", &expect!["(2.2, -0.4)"]);
}

#[test]
fn complex_literal_supports_negation() {
    check_expr("", "-(3.0 + 4.0i)", &expect!["(-3.0, -4.0)"]);
}

#[test]
fn complex_literal_supports_unary_positive() {
    check_expr("", "+(3.0 + 4.0i)", &expect!["(3.0, 4.0)"]);
}

#[test]
fn complex_literal_supports_exponentiation() {
    check_expr("", "(3.0 + 0.0i) ^ (0.0 + 0.0i)", &expect!["(1.0, 0.0)"]);
}

#[test]
fn partial_eval_simple_stmt() {
    check_partial_eval_stmt(
        "",
        "{3; {4} 5;}",
        &[2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [12-12] (Public):
                        Namespace (Ident 0 [12-12] "test"): <empty>
                Blocks:
                    Block 0 [0-11] [Type Unit]:
                        0
                        1
                        3
                    Block 1 [4-7] [Type Int]:
                        2
                Stmts:
                    Stmt 0 [1-3]: Semi: 1
                    Stmt 1 [4-7]: Expr: 2
                    Stmt 2 [5-6]: Expr: 3
                    Stmt 3 [8-10]: Semi: 4
                Exprs:
                    Expr 0 [0-11] [Type Unit]: Expr Block: 0
                    Expr 1 [1-2] [Type Int]: Lit: Int(3)
                    Expr 2 [4-7] [Type Int]: Expr Block: 1
                    Expr 3 [5-6] [Type Int]: Lit: Int(4)
                    Expr 4 [8-9] [Type Int]: Lit: Int(5)
                Pats:"#]],
        &expect!["4"],
    );
}

#[test]
fn partial_eval_stmt_with_bound_variable() {
    check_partial_eval_stmt(
        "",
        "{let x = 3; {x} ()}",
        &[0_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [20-20] (Public):
                        Namespace (Ident 1 [20-20] "test"): <empty>
                Blocks:
                    Block 0 [0-19] [Type Unit]:
                        0
                        1
                        3
                    Block 1 [12-15] [Type Int]:
                        2
                Stmts:
                    Stmt 0 [1-11]: Local (Immutable):
                        0
                        1
                    Stmt 1 [12-15]: Expr: 2
                    Stmt 2 [13-14]: Expr: 3
                    Stmt 3 [16-18]: Expr: 4
                Exprs:
                    Expr 0 [0-19] [Type Unit]: Expr Block: 0
                    Expr 1 [9-10] [Type Int]: Lit: Int(3)
                    Expr 2 [12-15] [Type Int]: Expr Block: 1
                    Expr 3 [13-14] [Type Int]: Var: Local 0
                    Expr 4 [16-18] [Type Unit]: Unit
                Pats:
                    Pat 0 [5-6] [Type Int]: Bind: Ident 0 [5-6] "x""#]],
        &expect!["3"],
    );
}

#[test]
fn partial_eval_stmt_with_mutable_variable_update() {
    check_partial_eval_stmt(
        "",
        "{mutable x = 0; set x += 1; {x} set x = -1;}",
        &[0_u32.into(), 1_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [45-45] (Public):
                        Namespace (Ident 1 [45-45] "test"): <empty>
                Blocks:
                    Block 0 [0-44] [Type Unit]:
                        0
                        1
                        2
                        4
                    Block 1 [28-31] [Type Int]:
                        3
                Stmts:
                    Stmt 0 [1-15]: Local (Mutable):
                        0
                        1
                    Stmt 1 [16-27]: Semi: 2
                    Stmt 2 [28-31]: Expr: 5
                    Stmt 3 [29-30]: Expr: 6
                    Stmt 4 [32-43]: Semi: 7
                Exprs:
                    Expr 0 [0-44] [Type Unit]: Expr Block: 0
                    Expr 1 [13-14] [Type Int]: Lit: Int(0)
                    Expr 2 [16-26] [Type Unit]: AssignOp (Add):
                        3
                        4
                    Expr 3 [20-21] [Type Int]: Var: Local 0
                    Expr 4 [25-26] [Type Int]: Lit: Int(1)
                    Expr 5 [28-31] [Type Int]: Expr Block: 1
                    Expr 6 [29-30] [Type Int]: Var: Local 0
                    Expr 7 [32-42] [Type Unit]: Assign:
                        8
                        9
                    Expr 8 [36-37] [Type Int]: Var: Local 0
                    Expr 9 [40-42] [Type Int]: UnOp (Neg):
                        10
                    Expr 10 [41-42] [Type Int]: Lit: Int(1)
                Pats:
                    Pat 0 [9-10] [Type Int]: Bind: Ident 0 [9-10] "x""#]],
        &expect!["1"],
    );
}

#[test]
fn partial_eval_stmt_with_mutable_variable_update_out_of_order_works() {
    check_partial_eval_stmt(
        "",
        "{mutable x = 0; set x += 1; {x} set x = -1;}",
        &[0_u32.into(), 4_u32.into(), 3_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [45-45] (Public):
                        Namespace (Ident 1 [45-45] "test"): <empty>
                Blocks:
                    Block 0 [0-44] [Type Unit]:
                        0
                        1
                        2
                        4
                    Block 1 [28-31] [Type Int]:
                        3
                Stmts:
                    Stmt 0 [1-15]: Local (Mutable):
                        0
                        1
                    Stmt 1 [16-27]: Semi: 2
                    Stmt 2 [28-31]: Expr: 5
                    Stmt 3 [29-30]: Expr: 6
                    Stmt 4 [32-43]: Semi: 7
                Exprs:
                    Expr 0 [0-44] [Type Unit]: Expr Block: 0
                    Expr 1 [13-14] [Type Int]: Lit: Int(0)
                    Expr 2 [16-26] [Type Unit]: AssignOp (Add):
                        3
                        4
                    Expr 3 [20-21] [Type Int]: Var: Local 0
                    Expr 4 [25-26] [Type Int]: Lit: Int(1)
                    Expr 5 [28-31] [Type Int]: Expr Block: 1
                    Expr 6 [29-30] [Type Int]: Var: Local 0
                    Expr 7 [32-42] [Type Unit]: Assign:
                        8
                        9
                    Expr 8 [36-37] [Type Int]: Var: Local 0
                    Expr 9 [40-42] [Type Int]: UnOp (Neg):
                        10
                    Expr 10 [41-42] [Type Int]: Lit: Int(1)
                Pats:
                    Pat 0 [9-10] [Type Int]: Bind: Ident 0 [9-10] "x""#]],
        &expect!["-1"],
    );
}

#[test]
fn partial_eval_stmt_with_mutable_variable_update_repeat_stmts_works() {
    check_partial_eval_stmt(
        "",
        "{mutable x = 0; set x += 1; {x} set x = -1;}",
        &[0_u32.into(), 1_u32.into(), 1_u32.into(), 3_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [45-45] (Public):
                        Namespace (Ident 1 [45-45] "test"): <empty>
                Blocks:
                    Block 0 [0-44] [Type Unit]:
                        0
                        1
                        2
                        4
                    Block 1 [28-31] [Type Int]:
                        3
                Stmts:
                    Stmt 0 [1-15]: Local (Mutable):
                        0
                        1
                    Stmt 1 [16-27]: Semi: 2
                    Stmt 2 [28-31]: Expr: 5
                    Stmt 3 [29-30]: Expr: 6
                    Stmt 4 [32-43]: Semi: 7
                Exprs:
                    Expr 0 [0-44] [Type Unit]: Expr Block: 0
                    Expr 1 [13-14] [Type Int]: Lit: Int(0)
                    Expr 2 [16-26] [Type Unit]: AssignOp (Add):
                        3
                        4
                    Expr 3 [20-21] [Type Int]: Var: Local 0
                    Expr 4 [25-26] [Type Int]: Lit: Int(1)
                    Expr 5 [28-31] [Type Int]: Expr Block: 1
                    Expr 6 [29-30] [Type Int]: Var: Local 0
                    Expr 7 [32-42] [Type Unit]: Assign:
                        8
                        9
                    Expr 8 [36-37] [Type Int]: Var: Local 0
                    Expr 9 [40-42] [Type Int]: UnOp (Neg):
                        10
                    Expr 10 [41-42] [Type Int]: Lit: Int(1)
                Pats:
                    Pat 0 [9-10] [Type Int]: Bind: Ident 0 [9-10] "x""#]],
        &expect!["2"],
    );
}

#[test]
fn partial_eval_stmt_with_bool_short_circuit() {
    check_partial_eval_stmt(
        "",
        "{let x = true; { x or false } ();}",
        &[0_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [35-35] (Public):
                        Namespace (Ident 1 [35-35] "test"): <empty>
                Blocks:
                    Block 0 [0-34] [Type Unit]:
                        0
                        1
                        3
                    Block 1 [15-29] [Type Bool]:
                        2
                Stmts:
                    Stmt 0 [1-14]: Local (Immutable):
                        0
                        1
                    Stmt 1 [15-29]: Expr: 2
                    Stmt 2 [17-27]: Expr: 3
                    Stmt 3 [30-33]: Semi: 6
                Exprs:
                    Expr 0 [0-34] [Type Unit]: Expr Block: 0
                    Expr 1 [9-13] [Type Bool]: Lit: Bool(true)
                    Expr 2 [15-29] [Type Bool]: Expr Block: 1
                    Expr 3 [17-27] [Type Bool]: BinOp (OrL):
                        4
                        5
                    Expr 4 [17-18] [Type Bool]: Var: Local 0
                    Expr 5 [22-27] [Type Bool]: Lit: Bool(false)
                    Expr 6 [30-32] [Type Unit]: Unit
                Pats:
                    Pat 0 [5-6] [Type Bool]: Bind: Ident 0 [5-6] "x""#]],
        &expect!["true"],
    );
}

#[test]
fn partial_eval_stmt_with_bool_no_short_circuit() {
    check_partial_eval_stmt(
        "",
        "{let x = false; { x or true } ();}",
        &[0_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [35-35] (Public):
                        Namespace (Ident 1 [35-35] "test"): <empty>
                Blocks:
                    Block 0 [0-34] [Type Unit]:
                        0
                        1
                        3
                    Block 1 [16-29] [Type Bool]:
                        2
                Stmts:
                    Stmt 0 [1-15]: Local (Immutable):
                        0
                        1
                    Stmt 1 [16-29]: Expr: 2
                    Stmt 2 [18-27]: Expr: 3
                    Stmt 3 [30-33]: Semi: 6
                Exprs:
                    Expr 0 [0-34] [Type Unit]: Expr Block: 0
                    Expr 1 [9-14] [Type Bool]: Lit: Bool(false)
                    Expr 2 [16-29] [Type Bool]: Expr Block: 1
                    Expr 3 [18-27] [Type Bool]: BinOp (OrL):
                        4
                        5
                    Expr 4 [18-19] [Type Bool]: Var: Local 0
                    Expr 5 [23-27] [Type Bool]: Lit: Bool(true)
                    Expr 6 [30-32] [Type Unit]: Unit
                Pats:
                    Pat 0 [5-6] [Type Bool]: Bind: Ident 0 [5-6] "x""#]],
        &expect!["true"],
    );
}

#[test]
fn partial_eval_stmt_with_loop() {
    check_partial_eval_stmt(
        "",
        "{mutable x = 0; while x < 3 { set x += 1; } {x} ();}",
        &[0_u32.into(), 1_u32.into(), 4_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [53-53] (Public):
                        Namespace (Ident 1 [53-53] "test"): <empty>
                Blocks:
                    Block 0 [0-52] [Type Unit]:
                        0
                        1
                        3
                        5
                    Block 1 [28-43] [Type Unit]:
                        2
                    Block 2 [44-47] [Type Int]:
                        4
                Stmts:
                    Stmt 0 [1-15]: Local (Mutable):
                        0
                        1
                    Stmt 1 [16-43]: Expr: 2
                    Stmt 2 [30-41]: Semi: 6
                    Stmt 3 [44-47]: Expr: 9
                    Stmt 4 [45-46]: Expr: 10
                    Stmt 5 [48-51]: Semi: 11
                Exprs:
                    Expr 0 [0-52] [Type Unit]: Expr Block: 0
                    Expr 1 [13-14] [Type Int]: Lit: Int(0)
                    Expr 2 [16-43] [Type Unit]: While:
                        3
                        1
                    Expr 3 [22-27] [Type Bool]: BinOp (Lt):
                        4
                        5
                    Expr 4 [22-23] [Type Int]: Var: Local 0
                    Expr 5 [26-27] [Type Int]: Lit: Int(3)
                    Expr 6 [30-40] [Type Unit]: AssignOp (Add):
                        7
                        8
                    Expr 7 [34-35] [Type Int]: Var: Local 0
                    Expr 8 [39-40] [Type Int]: Lit: Int(1)
                    Expr 9 [44-47] [Type Int]: Expr Block: 2
                    Expr 10 [45-46] [Type Int]: Var: Local 0
                    Expr 11 [48-50] [Type Unit]: Unit
                Pats:
                    Pat 0 [9-10] [Type Int]: Bind: Ident 0 [9-10] "x""#]],
        &expect!["3"],
    );
}

#[test]
fn partial_eval_stmt_function_calls() {
    check_partial_eval_stmt(
        indoc! {"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
            }
        "},
        "{let x = Test.Add1(4); {x} Test.Add1(3)}",
        &[0_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [41-102] (Public):
                        Namespace (Ident 1 [51-55] "Test"): Item 1
                    Item 1 [62-100] (Internal):
                        Parent: 0
                        Callable 0 [62-100] (function):
                            name: Ident 0 [71-75] "Add1"
                            input: 1
                            output: Int
                            functors: empty set
                            implementation: Spec:
                                SpecImpl:
                                    body: SpecDecl 1 [62-100]: None 2
                                    adj: <none>
                                    ctl: <none>
                                    ctl-adj: <none>
                Blocks:
                    Block 0 [0-40] [Type Int]:
                        0
                        1
                        3
                    Block 1 [23-26] [Type Int]:
                        2
                    Block 2 [91-100] [Type Int]:
                        4
                Stmts:
                    Stmt 0 [1-22]: Local (Immutable):
                        0
                        1
                    Stmt 1 [23-26]: Expr: 4
                    Stmt 2 [24-25]: Expr: 5
                    Stmt 3 [27-39]: Expr: 6
                    Stmt 4 [93-98]: Expr: 9
                Exprs:
                    Expr 0 [0-40] [Type Int]: Expr Block: 0
                    Expr 1 [9-21] [Type Int]: Call:
                        2
                        3
                    Expr 2 [9-18] [Type (Int -> Int)]: Var: Item 1
                    Expr 3 [19-20] [Type Int]: Lit: Int(4)
                    Expr 4 [23-26] [Type Int]: Expr Block: 1
                    Expr 5 [24-25] [Type Int]: Var: Local 0
                    Expr 6 [27-39] [Type Int]: Call:
                        7
                        8
                    Expr 7 [27-36] [Type (Int -> Int)]: Var: Item 1
                    Expr 8 [37-38] [Type Int]: Lit: Int(3)
                    Expr 9 [93-98] [Type Int]: BinOp (Add):
                        10
                        11
                    Expr 10 [93-94] [Type Int]: Var: Local 1
                    Expr 11 [97-98] [Type Int]: Lit: Int(1)
                Pats:
                    Pat 0 [5-6] [Type Int]: Bind: Ident 0 [5-6] "x"
                    Pat 1 [76-83] [Type Int]: Bind: Ident 1 [76-77] "x""#]],
        &expect!["5"],
    );
}

#[test]
fn partial_eval_stmt_function_calls_from_library() {
    check_partial_eval_stmt(
        "",
        "{let x = [1, 2, 3]; {Length(x)} 3}",
        &[0_u32.into(), 2_u32.into()],
        &expect![[r#"
            Package:
                Entry Expression: 0
                Items:
                    Item 0 [35-35] (Public):
                        Namespace (Ident 1 [35-35] "test"): <empty>
                Blocks:
                    Block 0 [0-34] [Type Int]:
                        0
                        1
                        3
                    Block 1 [20-31] [Type Int]:
                        2
                Stmts:
                    Stmt 0 [1-19]: Local (Immutable):
                        0
                        1
                    Stmt 1 [20-31]: Expr: 5
                    Stmt 2 [21-30]: Expr: 6
                    Stmt 3 [32-33]: Expr: 9
                Exprs:
                    Expr 0 [0-34] [Type Int]: Expr Block: 0
                    Expr 1 [9-18] [Type (Int)[]]: Array:
                        2
                        3
                        4
                    Expr 2 [10-11] [Type Int]: Lit: Int(1)
                    Expr 3 [13-14] [Type Int]: Lit: Int(2)
                    Expr 4 [16-17] [Type Int]: Lit: Int(3)
                    Expr 5 [20-31] [Type Int]: Expr Block: 1
                    Expr 6 [21-30] [Type Int]: Call:
                        7
                        8
                    Expr 7 [21-27] [Type ((Int)[] -> Int)]: Var:
                        res: Item 1 (Package 0)
                        generics:
                            Int
                    Expr 8 [28-29] [Type (Int)[]]: Var: Local 0
                    Expr 9 [32-33] [Type Int]: Lit: Int(3)
                Pats:
                    Pat 0 [5-6] [Type (Int)[]]: Bind: Ident 0 [5-6] "x""#]],
        &expect!["3"],
    );
}
