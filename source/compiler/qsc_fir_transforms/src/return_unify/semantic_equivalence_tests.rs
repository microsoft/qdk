// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(feature = "slow-proptest-tests")]
use crate::test_utils::check_semantic_equivalence;
use crate::test_utils::check_semantic_equivalence_with_expected as check_value;
#[cfg(feature = "slow-proptest-tests")]
use indoc::formatdoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;
use qsc_eval::val::Value;

#[test]
fn semicolon_failure_in_non_unit_body_preserves_user_error() {
    check_non_unit_failure("fail \"expected\";");
}

#[test]
fn eager_operand_failures_in_non_unit_tails_preserve_user_error_and_location() {
    for body in [
        "fail \"expected\"",
        "[fail \"expected\"];",
        "(1, fail \"expected\");",
        "function Ignore(value : Int) : Unit {} Ignore(fail \"expected\");",
        "if fail \"expected\" {}",
        "$\"{fail \"expected\"}\";",
        "1 + (fail \"expected\");",
        "[1, size = fail \"expected\"];",
        "mutable value = 0; set value += fail \"expected\";",
    ] {
        check_non_unit_failure(body);
    }
}

#[test]
fn short_circuit_unselected_branch_and_deferred_closure_failures_return_seventeen() {
    check_value(
        indoc::indoc! {r#"
            @EntryPoint()
            operation Main() : Int {
                let a = false and (fail "right and operand");
                let b = true or (fail "right or operand");
                let c = if false { fail "unselected branch" } else { 17 };
                let deferred : Int -> Int = x -> { fail "closure body"; x };
                if a or not b { fail "unexpected boolean result"; }
                c
            }
        "#},
        Value::Int(17),
    );
}

fn check_non_unit_failure(body: &str) {
    use crate::test_utils::{
        PipelineStage, compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };

    let source = format!("@EntryPoint() operation Main() : Int {{ {body} }}");
    let (store, package_id) = compile_to_fir(&source);
    let fail_span = store
        .get(package_id)
        .exprs
        .iter()
        .find_map(|(_, expr)| {
            matches!(expr.kind, qsc_fir::fir::ExprKind::Fail(_)).then_some(expr.span)
        })
        .expect("source must contain a fail expression");
    let expected = (
        Err(format!(
            "{:?}",
            qsc_eval::Error::UserFail(
                "expected".into(),
                (
                    qsc_lowerer::map_fir_package_to_hir(fail_span.package),
                    fail_span.span
                )
                    .into()
            )
        )),
        Vec::new(),
    );
    assert_eq!(try_eval_fir_entry_with_trace(&store, package_id), expected);
    for stage in [PipelineStage::ReturnUnify, PipelineStage::Full] {
        let (mut store, package_id) = compile_and_run_pipeline_to(&source, stage);
        crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package_id, &[]);
        assert_eq!(
            try_eval_fir_entry_with_trace(&store, package_id),
            expected,
            "failure must survive {stage:?}: {body}"
        );
    }
}

#[test]
fn nested_while_condition_preserves_return_and_fallthrough_values() {
    // Run(2) returns 172 during the third condition; Run(5) exits normally with 24.
    check_value(
        indoc::indoc! {r#"
        namespace Test {
            function Identity(value : Int) : Int { value }
            function Run(stop : Int) : Int {
                mutable count = 0;
                mutable total = 1;
                while Identity({
                    let next = {
                        set total = total + 2;
                        if count == stop { return total * 10 + count; }
                        count
                    };
                    next
                }) < 3 {
                    set total = total + 5;
                    set count = count + 1;
                }
                total
            }
            @EntryPoint()
            operation Main() : Int { Run(2) * 100 + Run(5) }
        }
    "#},
        Value::Int(17_224),
    );
}

#[test]
fn return_in_while_comparison_operand_returns_forty_two() {
    check_value(
        indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                while ({ return 42; 0 }) < 1 {}
                0
            }
        }
    "#},
        Value::Int(42),
    );
}

#[test]
fn return_in_while_condition_block_returns_forty_two() {
    check_value(
        indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                while ({ return 42; 0 < 1 }) {}
                0
            }
        }
    "#},
        Value::Int(42),
    );
}

#[test]
fn while_condition_return_skips_division_by_zero_and_loop_body() {
    check_value(
        indoc::indoc! {r#"
            function Run(stop : Bool, divisor : Int) : Int {
                while ({ if stop { return 42; } 10 / divisor }) < 2 {
                    return 7;
                }
                0
            }
            @EntryPoint()
            operation Main() : (Int, Int) { (Run(true, 0), Run(false, 10)) }
        "#},
        Value::Tuple(vec![Value::Int(42), Value::Int(7)].into(), None),
    );
}

#[test]
fn negated_while_condition_return_does_not_execute_loop_body() {
    check_value(
        indoc::indoc! {r#"
            function Run(stop : Bool) : Int {
                while not ({ if stop { return 42; } false }) {
                    fail "loop body executed after condition returned";
                }
                0
            }
            @EntryPoint()
            operation Main() : Int { Run(true) }
        "#},
        Value::Int(42),
    );
}

#[test]
fn while_condition_qubit_operand_return_skips_measurement_and_preserves_release() {
    // Qubit operand temporaries have no scalar default. Their array-backed
    // representation must not be read after the condition returns.
    check_value(
        indoc::indoc! {r#"
            operation Run(stop : Bool) : Int {
                use qs = Qubit[1];
                while MResetZ({
                    if stop { return 42; }
                    qs[0]
                }) == One {
                    fail "zero qubit entered the loop";
                }
                7
            }
            @EntryPoint()
            operation Main() : (Int, Int) { (Run(true), Run(false)) }
        "#},
        Value::Tuple(vec![Value::Int(42), Value::Int(7)].into(), None),
    );
}

#[test]
fn nested_loop_in_condition_preserves_array_return_and_skips_later_operands() {
    check_value(
        indoc::indoc! {r#"
            function Run(stop : Int) : Int[] {
                mutable count = 0;
                while ({
                    while count < 3 {
                        set count += 1;
                        if count == stop { return [count, 42]; }
                    }
                    count
                }) < ({
                    if count == stop { fail "operand after condition return"; }
                    3
                }) {
                    fail "condition must return or be false";
                }
                [count, 7]
            }
            @EntryPoint()
            operation Main() : (Int[], Int[]) { (Run(2), Run(9)) }
        "#},
        Value::Tuple(
            vec![
                Value::Array(vec![Value::Int(2), Value::Int(42)].into()),
                Value::Array(vec![Value::Int(3), Value::Int(7)].into()),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn while_operand_return_preserves_repeated_eager_order() {
    // Decimal digits record the first operand (1), second operand (2), and body (3).
    let source = indoc::indoc! {r#"
        namespace Test {
            function Below(value : Int, limit : Int) : Bool { value < limit }
            function Comparison(stop : Int) : Int {
                mutable count = 0;
                mutable marker = 0;
                while ({
                    set marker = marker * 10 + 1;
                    if count == stop { return marker; }
                    count
                }) < ({
                    if count == stop { fail "later comparison operand after return"; }
                    set marker = marker * 10 + 2;
                    3
                }) {
                    set marker = marker * 10 + 3;
                    set count = count + 1;
                }
                marker
            }
            function Call(stop : Int) : Int {
                mutable count = 0;
                mutable marker = 0;
                while Below({
                    set marker = marker * 10 + 1;
                    if count == stop { return marker; }
                    count
                }, {
                    if count == stop { fail "later call argument after return"; }
                    set marker = marker * 10 + 2;
                    3
                }) {
                    set marker = marker * 10 + 3;
                    set count = count + 1;
                }
                marker
            }
            @EntryPoint()
            operation Main() : (Int, Int, Int, Int) {
                (Comparison(2), Comparison(5), Call(2), Call(5))
            }
        }
    "#};
    check_value(
        source,
        Value::Tuple(
            vec![
                Value::Int(1_231_231),
                Value::Int(12_312_312_312),
                Value::Int(1_231_231),
                Value::Int(12_312_312_312),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn while_operand_return_preserves_short_circuit_and_body_suppression() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Forbidden() : Bool { fail "skipped operand evaluated" }
            function RightReturn() : Int {
                while false and Forbidden() { fail "false condition body"; }
                while not (true or Forbidden()) { fail "short circuit body"; }
                while true and ({ return 73; true }) { fail "body after right return"; }
                0
            }
            function LeftReturn() : Int {
                while ({ return 91; false }) or Forbidden() { fail "body after left return"; }
                0
            }
            @EntryPoint()
            operation Main() : (Int, Int) { (RightReturn(), LeftReturn()) }
        }
    "#};
    check_value(
        source,
        Value::Tuple(vec![Value::Int(73), Value::Int(91)].into(), None),
    );
}

/// Generates syntactically valid Q# programs with return statements at
/// various positions covering all `return_unify` dispatch categories
/// (structured, flag, no-return). Each program wraps one of 12 template
/// patterns in a `namespace Test { function Main() : Int { ... } }` shell.
#[allow(clippy::too_many_lines)]
#[cfg(feature = "slow-proptest-tests")]
fn return_pattern_strategy() -> impl Strategy<Value = String> {
    let cmp = || 0..10i64;
    let val = || 0..100i64;
    let bound = || 1..6i64;
    let idx = || 0..5i64;

    prop_oneof![
        // 1. No-return baseline: pure if-else expression.
        (cmp(), cmp(), val(), val()).prop_map(|(a, b, c, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    if {a} > {b} {{ {c} }} else {{ {d} }}
                }}
            }}
        "}),
        // 2. Single guard clause.
        (cmp(), cmp(), val(), val()).prop_map(|(a, b, c, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    if {a} > {b} {{ return {c}; }}
                    {d}
                }}
            }}
        "}),
        // 3. Both branches return.
        (cmp(), cmp(), val(), val()).prop_map(|(a, b, c, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    if {a} > {b} {{ return {c}; }} else {{ return {d}; }}
                }}
            }}
        "}),
        // 4. Two guard clauses with fallthrough.
        (cmp(), cmp(), cmp(), cmp(), val(), val(), val()).prop_map(
            |(a, b, c, d, e, f, g)| formatdoc! {"
                namespace Test {{
                    function Main() : Int {{
                        if {a} > {b} {{ return {e}; }}
                        if {c} > {d} {{ return {f}; }}
                        {g}
                    }}
                }}
            "}
        ),
        // 5. While with early return.
        (bound(), idx(), val(), val()).prop_map(|(n, t, v, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    mutable x = 0;
                    while x < {n} {{
                        if x == {t} {{ return {v}; }}
                        x += 1;
                    }}
                    {d}
                }}
            }}
        "}),
        // 6. For loop with early return.
        (bound(), idx(), val(), val()).prop_map(|(n, t, v, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    for i in 0..{n} {{
                        if i == {t} {{ return {v}; }}
                    }}
                    {d}
                }}
            }}
        "}),
        // 7. Nested if with return.
        (cmp(), cmp(), cmp(), cmp(), val(), val(), val()).prop_map(
            |(a, b, c, d, e, f, g)| formatdoc! {"
                namespace Test {{
                    function Main() : Int {{
                        if {a} > {b} {{
                            if {c} > {d} {{ return {e}; }}
                            {f}
                        }} else {{
                            {g}
                        }}
                    }}
                }}
            "}
        ),
        // 8. Block expression with return.
        (cmp(), cmp(), val(), val(), val()).prop_map(|(a, b, c, d, e)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    let x = {{
                        if {a} > {b} {{ return {c}; }}
                        {d}
                    }};
                    x + {e}
                }}
            }}
        "}),
        // 9. Return in else branch only.
        (cmp(), cmp(), val(), val()).prop_map(|(a, b, c, d)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    if {a} > {b} {{ {c} }} else {{ return {d}; }}
                }}
            }}
        "}),
        // 10. Multiple returns with mutable computation.
        (cmp(), cmp(), cmp(), cmp(), val(), val(), val(), val()).prop_map(
            |(a, b, c, d, e, f, g, h)| formatdoc! {"
                namespace Test {{
                    function Main() : Int {{
                        mutable result = 0;
                        if {a} > {b} {{ return {e}; }}
                        result = {f};
                        if {c} > {d} {{ return {g}; }}
                        result + {h}
                    }}
                }}
            "}
        ),
        // 11. Triple nested if-return.
        (
            cmp(),
            cmp(),
            cmp(),
            cmp(),
            cmp(),
            cmp(),
            val(),
            val(),
            val(),
            val()
        )
            .prop_map(|(a, b, c, d, e, f, g, h, i, j)| formatdoc! {"
                namespace Test {{
                    function Main() : Int {{
                        if {a} > {b} {{
                            if {c} > {d} {{
                                if {e} > {f} {{ return {g}; }}
                                return {h};
                            }}
                            {i}
                        }} else {{
                            return {j};
                        }}
                    }}
                }}
            "}),
        // 12. While with accumulator and conditional return.
        (bound(), idx()).prop_map(|(n, t)| formatdoc! {"
            namespace Test {{
                function Main() : Int {{
                    mutable acc = 0;
                    mutable i = 0;
                    while i < {n} {{
                        if i > {t} {{ return acc; }}
                        acc = acc + i;
                        i += 1;
                    }}
                    acc
                }}
            }}
        "}),
    ]
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn differential_return_unify(source in return_pattern_strategy()) {
        check_semantic_equivalence(&source);
    }
}

mod nested_operand_order {
    use super::check_value;
    use indoc::indoc;
    use qsc_eval::val::Value;

    #[test]
    fn nested_short_circuit_call_operand_return_suppresses_later_operands_and_body() {
        check_value(
            indoc! {r#"
                namespace Test {
                    function Below(value : Int, limit : Int) : Bool { value < limit }
                    function Forbidden() : Bool { fail "short circuit tail executed" }
                    @EntryPoint()
                    operation Main() : Int {
                        mutable marker = 0;
                        while false or (true and Below({
                            set marker = marker * 10 + 1;
                            let value = 10 + {
                                set marker = marker * 10 + 2;
                                return marker;
                                0
                            };
                            value
                        }, {
                            fail "later call operand executed";
                            100
                        })) or Forbidden() {
                            fail "loop body executed after return";
                        }
                        -1
                    }
                }
            "#},
            Value::Int(12),
        );
    }

    #[test]
    fn nested_short_circuit_call_operand_preserves_fallthrough_and_repeated_order() {
        // Record argument/body order as digits; a disabled condition records nothing.
        check_value(
            indoc! {r#"
                namespace Test {
                    function Below(value : Int, limit : Int) : Bool { value < limit }
                    function Run(stop : Int, enabled : Bool) : Int {
                        mutable marker = 0;
                        mutable checks = 0;
                        while false or (enabled and Below({
                            set marker = marker * 10 + 1;
                            let value = 0 + {
                                if checks == stop { return marker; }
                                checks
                            };
                            value
                        }, {
                            set marker = marker * 10 + 2;
                            2
                        })) {
                            set marker = marker * 10 + 3;
                            set checks += 1;
                        }
                        marker
                    }
                    @EntryPoint()
                    operation Main() : (Int, Int, Int) {
                        (Run(1, true), Run(5, true), Run(0, false))
                    }
                }
            "#},
            Value::Tuple(
                vec![Value::Int(1231), Value::Int(12_312_312), Value::Int(0)].into(),
                None,
            ),
        );
    }

    #[test]
    fn skipped_nested_condition_operands_do_not_return_or_fail() {
        check_value(
            indoc! {r#"
                namespace Test {
                    function Truth(value : Bool) : Bool { value }
                    function Below(value : Int, limit : Int) : Bool { value < limit }
                    @EntryPoint()
                    operation Main() : Int {
                        mutable marker = 1;
                        if (Truth(false) and Below({
                            set marker = 9;
                            return marker;
                            0
                        }, {
                            fail "skipped comparison operand executed";
                            10
                        })) or (Truth(true) or Below({
                            return 8;
                            0
                        }, 10)) {
                            set marker = marker * 10 + 2;
                        } else {
                            fail "wrong condition branch";
                        }
                        set marker = marker * 10 + 3;
                        marker
                    }
                }
            "#},
            Value::Int(123),
        );
    }

    #[test]
    fn repeated_condition_preserves_array_snapshot_before_mutation_and_return() {
        // Encode condition count, current array value, and body history. The first
        // operand must read the array before the second operand updates it.
        check_value(
            indoc! {r#"
                namespace Test {
                    function Run(stop : Int) : Int {
                        mutable values = [0, 7];
                        mutable checks = 0;
                        mutable history = 0;
                        while (values[0] + {
                            set checks += 1;
                            set values w/= 0 <- values[0] + 1;
                            if checks == stop {
                                return checks * 10000 + values[0] * 100 + history;
                            }
                            0
                        }) < 2 {
                            set history = history * 10 + values[0];
                        }
                        900000 + checks * 10000 + values[0] * 100 + history
                    }
                    @EntryPoint()
                    operation Main() : (Int, Int) { (Run(3), Run(9)) }
                }
            "#},
            Value::Tuple(vec![Value::Int(30312), Value::Int(930_312)].into(), None),
        );
    }
}
