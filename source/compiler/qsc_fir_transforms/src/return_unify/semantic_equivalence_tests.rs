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

use crate::test_utils::{
    TraceOp::{QubitAllocate, QubitRelease, Reset},
    compile_to_fir, try_eval_fir_entry_with_trace,
};
use qsc_fir::{
    fir::{ExprKind, PatKind, StmtKind},
    ty::{GenericArg, Prim, Ty},
};

#[test]
fn non_defaultable_tails_preserve_returned_payload_and_run_effects_only_on_fallthrough() {
    for stop in [false, true] {
        for (output, returned, tail, read, tag, expected_tag) in [
            ("Qubit", "q", "q", "value", "0", 0),
            (
                "(Qubit, Int)",
                "(q, 7)",
                "(q, 9)",
                "Fst(value)",
                "Snd(value)",
                if stop { 7 } else { 9 },
            ),
            (
                "Payload",
                "new Payload { Q = q, Tag = 7 }",
                "new Payload { Q = q, Tag = 9 }",
                "value.Q",
                "value.Tag",
                if stop { 7 } else { 9 },
            ),
        ] {
            let source = format!(
                r#"
                struct Payload {{ Q : Qubit, Tag : Int }}
                operation Tail(q : Qubit) : {output} {{
                    Message("tail");
                    use temporary = Qubit();
                    {tail}
                }}
                operation Run(stop : Bool, q : Qubit, other : Qubit) : {output} {{
                    {{ if stop {{ return {returned}; }} Tail(other) }}
                }}
                @EntryPoint() operation Main() : (Result, Result, Int) {{
                    use q = Qubit();
                    use other = Qubit();
                    let value = Run({stop}, q, other);
                    X({read});
                    (MResetZ(q), MResetZ(other), {tag})
                }}
            "#
            );
            // The helper compares messages and allocation/release/gate traces as
            // well as the explicit value, so a skipped Tail must do no work.
            check_value(
                &source,
                Value::Tuple(
                    vec![
                        Value::Result(qsc_eval::val::Result::Val(stop)),
                        Value::Result(qsc_eval::val::Result::Val(!stop)),
                        Value::Int(expected_tag),
                    ]
                    .into(),
                    None,
                ),
            );
        }
    }
}

#[test]
fn non_block_conditional_returns_skip_remaining_operands_and_preserve_fallthrough_values() {
    for stop in [false, true] {
        for (body, expected) in [
            (
                "let v = stop ? 1 + (return 1) | 2; v",
                if stop { 1 } else { 2 },
            ),
            (
                "let v = stop ? Inc(return 1) | 2; v",
                if stop { 1 } else { 2 },
            ),
            (
                "let v = stop ? Inc(2) | Inc(return 1); v",
                if stop { 3 } else { 1 },
            ),
            (
                "let v = stop ? Inc(return 1) | Inc(return 2); v",
                if stop { 1 } else { 2 },
            ),
            (
                "let v = stop ? Inc(stop ? (return 4) | 0) | 2; v",
                if stop { 4 } else { 2 },
            ),
            (
                "if false {} elif (stop ? (return 1) | false) {} 0",
                i64::from(stop),
            ),
            (
                "while (stop ? Truth(return 1) | false) {} 0",
                i64::from(stop),
            ),
        ] {
            let source = format!(
                r#"
                function Inc(x : Int) : Int {{ Message("inc"); x + 1 }}
                function Truth(b : Bool) : Bool {{ Message("truth"); b }}
                function Run(stop : Bool) : Int {{ {body} }}
                @EntryPoint() operation Main() : Int {{ Run({stop}) }}
            "#
            );
            check_value(&source, Value::Int(expected));
        }
    }
}

#[test]
fn logical_assignment_returns_preserve_all_short_circuit_paths_and_generate_qir() {
    for (operator, rhs) in [
        ("and", "r or (return false)"),
        ("or", "r and (return true)"),
    ] {
        for initial in [false, true] {
            for measured_one in [false, true] {
                let preparation = if measured_one { "X(q);" } else { "" };
                // Keep the qubit in Main: this isolates returning RHS lowering
                // from conditional cleanup in a resource-owning callable.
                let source = format!(
                    r#"
                    function Run(r : Bool, initial : Bool) : Bool {{
                        mutable value = initial;
                        set value {operator}= {{ Message("rhs"); {rhs} }};
                        Message("continuation");
                        value
                    }}
                    @EntryPoint() operation Main() : Bool {{
                        use q = Qubit();
                        {preparation}
                        Run(MResetZ(q) == One, {initial})
                    }}
                "#
                );
                let expected = if operator == "and" {
                    initial && measured_one
                } else {
                    initial || measured_one
                };
                check_value(&source, Value::Bool(expected));
                let qir = crate::test_utils::generate_qir(&source);
                assert!(qir.contains("@ENTRYPOINT__main"), "{qir}");
            }
        }
    }
}

#[test]
fn short_circuit_assignment_skips_disabled_rhs_and_calls_after_return() {
    for (operator, initial, expected) in [("and", false, false), ("or", true, true)] {
        for enabled in [false, true] {
            let initial = if enabled { !initial } else { initial };
            let source = format!(
                r#"
                function Use(b : Bool) : Bool {{ fail "call after return"; }}
                @EntryPoint() operation Main() : Bool {{
                    mutable value = {initial};
                    set value {operator}= Use(return {{ Message("rhs"); {expected} }});
                    Message("continuation");
                    value
                }}
            "#
            );
            check_value(&source, Value::Bool(expected));
        }
    }
}

#[test]
fn nested_returns_preserve_operand_effect_order_and_do_not_overwrite_the_inner_return() {
    for (a, b, expected) in [
        (false, false, 1_949),
        (false, true, 19),
        (true, false, 123),
        (true, true, 12),
    ] {
        let source = format!(
            r#"
            function Identity(x : Int) : Int {{ Message("identity"); x }}
            @EntryPoint() operation Main() : Int {{
                mutable marker = 1;
                let value = {a}
                    ? Identity({{
                        set marker = marker * 10 + 2;
                        if {b} {{ return marker; }}
                        marker
                    }})
                    | Identity({{
                        set marker = marker * 10 + 9;
                        if {b} {{ return marker; }}
                        marker
                    }});
                set marker = marker * 10 + 3;
                return value + (Identity({a} ? (return marker) | marker) * 10);
            }}
        "#
        );
        // The marker encodes operand order; receiver output also proves that
        // Identity is not called when its argument returns from Main.
        check_value(&source, Value::Int(expected));
    }
}

#[test]
fn logical_assignment_returning_int_preserves_bool_rhs_type_and_generates_qir() {
    for measured_one in [false, true] {
        let preparation = if measured_one { "X(q);" } else { "" };
        let source = format!(
            r#"
            function Run(r : Bool) : Int {{
                mutable value = true;
                set value and= (r or (return 17));
                if value {{ 3 }} else {{ 5 }}
            }}
            @EntryPoint() operation Main() : Int {{
                use q = Qubit();
                {preparation}
                Run(MResetZ(q) == One)
            }}
        "#
        );
        check_value(&source, Value::Int(if measured_one { 3 } else { 17 }));
        let qir = crate::test_utils::generate_qir(&source);
        assert!(qir.contains("@ENTRYPOINT__main"), "{qir}");
    }
}

#[test]
fn non_defaultable_failing_tail_is_skipped_after_return_and_preserves_failure_on_fallthrough() {
    for stop in [false, true] {
        let source = format!(
            r#"
            operation Tail(q : Qubit) : Qubit {{ fail "tail failure"; }}
            operation Run(q : Qubit) : Qubit {{
                {{ if {stop} {{ return q; }} Tail(q) }}
            }}
            @EntryPoint() operation Main() : Result {{
                use q = Qubit();
                MResetZ(Run(q))
            }}
        "#
        );
        if stop {
            check_value(&source, Value::Result(qsc_eval::val::Result::Val(false)));
        } else {
            check_preserved_failure(&source, "tail failure", vec![QubitAllocate(0)]);
        }
    }
}

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
    let source = format!("@EntryPoint() operation Main() : Int {{ {body} }}");
    check_preserved_failure(&source, "expected", Vec::new());
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

fn check_preserved_failure(source: &str, message: &str, trace: Vec<crate::test_utils::TraceOp>) {
    use crate::test_utils::{
        PipelineStage, compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };

    let (store, package_id) = compile_to_fir(source);
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
                message.into(),
                (
                    qsc_lowerer::map_fir_package_to_hir(fail_span.package),
                    fail_span.span
                )
                    .into()
            )
        )),
        trace,
    );
    assert_eq!(try_eval_fir_entry_with_trace(&store, package_id), expected);
    for stage in [PipelineStage::ReturnUnify, PipelineStage::Full] {
        let (mut store, package_id) = compile_and_run_pipeline_to(source, stage);
        crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package_id, &[]);
        assert_eq!(
            try_eval_fir_entry_with_trace(&store, package_id),
            expected,
            "failure must survive {stage:?}: {source}"
        );
    }
}

#[test]
fn failing_for_iterables_preserve_user_failure_and_source_location() {
    for body in [
        "let value = for item : String in fail \"stop\" {};",
        "for item : String in fail \"stop\" {}",
        // Explicit array context is the compatibility control for the ordinary path.
        "let items : String[] = fail \"stop\"; for item in items {}",
    ] {
        let source = format!("@EntryPoint() operation Main() : Unit {{ {body} }}");
        check_preserved_failure(&source, "stop", Vec::new());
    }
}

#[test]
fn return_in_for_iterable_returns_seventeen() {
    check_value(
        "@EntryPoint() operation Main() : Int { for item : String in return 17 {} 0 }",
        Value::Int(17),
    );
}

#[test]
fn generated_adjoint_of_failing_for_iterable_reports_failure_before_gates() {
    for iterable in [
        "for item : String in fail \"iter\" { X(q); }",
        "let items : String[] = fail \"iter\"; for item in items { X(q); }",
    ] {
        let source = format!(
            "operation A(q : Qubit) : Unit is Adj {{ {iterable} }}
             @EntryPoint() operation Main() : Unit {{
                 use q = Qubit();
                 Adjoint A(q);
                 Reset(q);
             }}"
        );
        check_preserved_failure(
            &source,
            "iter",
            vec![crate::test_utils::TraceOp::QubitAllocate(0)],
        );
    }
}

#[test]
fn loop_control_in_qubit_argument_preserves_singleton_type_and_skips_call() {
    for transfer in ["break", "continue"] {
        for take_transfer in [true, false] {
            let source = format!(
                "operation UseQubit(q : Qubit) : Unit {{ Reset(q); }}
                 @EntryPoint() operation Main() : Unit {{
                     use q = Qubit();
                     mutable keepGoing = true;
                     while keepGoing {{
                         UseQubit(if {take_transfer} {{
                             keepGoing = false;
                             {transfer}
                         }} else {{ q }});
                         keepGoing = false;
                     }}
                 }}"
            );
            let (store, package_id) = compile_to_fir(&source);
            let package = store.get(package_id);
            // Check the generated singleton before FIR cleanup can hide an invalid
            // type on a skipped branch. Its element must keep the moved qubit's type.
            let mut singletons = 0;
            for (_, expr) in &package.exprs {
                let (ExprKind::Array(elements) | ExprKind::ArrayLit(elements)) = &expr.kind else {
                    continue;
                };
                let [element] = elements.as_slice() else {
                    continue;
                };
                let element = package.exprs.get(*element).expect("array element");
                if element.ty == Ty::Prim(Prim::Qubit) {
                    singletons += 1;
                    assert_eq!(expr.ty, Ty::Array(Box::new(element.ty.clone())), "{source}");
                }
            }
            assert!(
                singletons > 0,
                "fixture must exercise array backing: {source}"
            );
            let trace = if take_transfer {
                vec![QubitAllocate(0), QubitRelease(0)]
            } else {
                vec![QubitAllocate(0), Reset(0), QubitRelease(0)]
            };
            assert_eq!(
                try_eval_fir_entry_with_trace(&store, package_id),
                (Ok(Value::unit()), trace),
                "{source}"
            );
            check_value(&source, Value::unit());
        }
    }
}

#[test]
fn divergent_for_iterables_generate_typed_array_captures_and_length_calls() {
    for (pattern, element) in [
        ("item : Int", Ty::Prim(Prim::Int)),
        ("item : String", Ty::Prim(Prim::String)),
        ("item : Int[]", Ty::Array(Box::new(Ty::Prim(Prim::Int)))),
        (
            "(number : Int, text : String)",
            Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::String)]),
        ),
    ] {
        for body in [
            format!("let value = for {pattern} in fail \"stop\" {{}};"),
            format!("for {pattern} in fail \"stop\" {{}}"),
            format!("for {pattern} in return 17 {{}}"),
        ] {
            let source = format!("@EntryPoint() operation Main() : Int {{ {body} 0 }}");
            check_generated_array_loop_types(&source, &element);
        }
    }
}

fn check_generated_array_loop_types(source: &str, element: &qsc_fir::ty::Ty) {
    // Inspect the freshly lowered FIR: evaluating a divergent iterable never
    // reaches Length, and later passes may remove the ill-typed generated code.
    let (store, package_id) = compile_to_fir(source);
    let package = store.get(package_id);
    let array_ty = Ty::Array(Box::new(element.clone()));
    let mut captures = 0;
    let mut lengths = 0;
    for (_, stmt) in &package.stmts {
        let StmtKind::Local(_, pat, initializer) = &stmt.kind else {
            continue;
        };
        let pat = package.pats.get(*pat).expect("binding pattern must exist");
        let PatKind::Bind(ident) = &pat.kind else {
            continue;
        };
        let initializer = package
            .exprs
            .get(*initializer)
            .expect("binding initializer must exist");
        if ident.name.starts_with(".array_id") {
            captures += 1;
            assert_eq!(pat.ty, array_ty, "{source}");
            assert_eq!(initializer.ty, array_ty, "{source}");
        }
        if ident.name.starts_with(".len_id") {
            lengths += 1;
            let ExprKind::Call(callee, argument) = &initializer.kind else {
                panic!("generated Length binding must be a call");
            };
            assert_eq!(
                package
                    .exprs
                    .get(*argument)
                    .expect("argument must exist")
                    .ty,
                array_ty,
                "{source}"
            );
            let ExprKind::Var(_, generics) =
                &package.exprs.get(*callee).expect("callee must exist").kind
            else {
                panic!("generated Length callee must be a variable");
            };
            assert_eq!(generics, &[GenericArg::Ty(element.clone())], "{source}");
        }
    }
    assert!(captures > 0, "fixture must lower an array loop: {source}");
    assert_eq!(captures, lengths, "{source}");
}
