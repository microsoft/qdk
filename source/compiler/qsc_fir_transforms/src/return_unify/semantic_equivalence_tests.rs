// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::test_utils::check_semantic_equivalence;
use indoc::formatdoc;
use proptest::prelude::*;

#[test]
fn exploration_structural_while_nested_condition_return() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
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
    "#});
}

#[test]
fn exploration_structural_reduced_while_operand_return() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                while ({ return 42; 0 }) < 1 {}
                0
            }
        }
    "#});
}

#[test]
fn exploration_structural_reduced_while_block_control() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                while ({ return 42; 0 < 1 }) {}
                0
            }
        }
    "#});
}

#[test]
fn while_operand_return_preserves_repeated_eager_order() {
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
    check_while_condition_result(source, "(1231231, 12312312312, 1231231, 12312312312)");
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
    check_while_condition_result(source, "(73, 91)");
}

fn check_while_condition_result(source: &str, expected: &str) {
    use crate::test_utils::{compile_to_fir, try_eval_fir_entry_with_trace};
    let (store, package) = compile_to_fir(source);
    let (result, trace) = try_eval_fir_entry_with_trace(&store, package);
    assert_eq!(result.expect("original must succeed").to_string(), expected);
    assert!(trace.is_empty());
    check_semantic_equivalence(source);
}

mod round8_control {
    use super::check_while_condition_result;
    use indoc::indoc;

    #[test]
    fn repeat_fixup_tuple_return_preserves_body_continue_and_skips_suffix() {
        let source = indoc! {r#"
            namespace Test {
                operation Run(stop : Int) : Int {
                    mutable attempt = 0;
                    mutable marker = 0;
                    repeat {
                        set attempt += 1;
                        set marker = marker * 10 + 1;
                        if attempt == 1 { continue; }
                        set marker = marker * 10 + 2;
                    } until attempt >= 3
                    fixup {
                        let (before, after) = (marker, {
                            set marker = marker * 10 + 3;
                            if attempt == stop { return marker; }
                            marker
                        });
                        if before * 10 + 3 != after { fail "fixup snapshot changed"; }
                        if attempt == stop { fail "fixup suffix executed after return"; }
                        set marker = marker * 10 + 4;
                    }
                    marker
                }
                @EntryPoint()
                operation Main() : (Int, Int, Int) { (Run(1), Run(2), Run(9)) }
            }
        "#};
        check_while_condition_result(source, "(13, 134123, 134123412)");
    }

    #[test]
    fn for_tuple_initializer_preserves_continue_break_and_nested_local_return() {
        let source = indoc! {r#"
            namespace Test {
                function Run(stop : Int) : Int {
                    mutable marker = 0;
                    mutable total = 0;
                    for step in 1..4 {
                        let (before, after) = (marker, {
                            set marker = marker * 10 + step;
                            if step == 1 { continue; }
                            let value = {
                                if step == stop { return 900000 + marker * 100 + total; }
                                if step == 3 { break; }
                                marker
                            };
                            value
                        });
                        set total += before + after;
                        set marker = marker * 10 + 8;
                    }
                    marker * 100 + total
                }
                @EntryPoint()
                operation Main() : (Int, Int, Int, Int) {
                    (Run(1), Run(2), Run(3), Run(9))
                }
            }
        "#};
        check_while_condition_result(source, "(128313, 901200, 1028313, 128313)");
    }

    #[test]
    fn compound_assignment_rhs_return_preserves_old_value_across_loop_control() {
        let source = indoc! {r#"
            namespace Test {
                function Run(stop : Int) : Int {
                    mutable value = 7;
                    mutable marker = 0;
                    set value += {
                        set value = 40;
                        for step in 1..3 {
                            set marker = marker * 10 + step;
                            if step == 1 { continue; }
                            if step == stop { return value * 100 + marker; }
                            if step == 2 { break; }
                        }
                        5
                    };
                    value * 100 + marker
                }
                @EntryPoint()
                operation Main() : (Int, Int, Int) { (Run(1), Run(2), Run(9)) }
            }
        "#};
        check_while_condition_result(source, "(1212, 4012, 1212)");
    }

    #[test]
    fn lazy_array_operands_preserve_snapshots_and_skip_returns_and_loop_control() {
        let source = indoc! {r#"
            namespace Test {
                function Run(enabled : Bool, stop : Int) : Int {
                    mutable marker = 0;
                    mutable total = 0;
                    for step in 1..3 {
                        let values = [marker, (if enabled and ({
                            set marker = marker * 10 + step;
                            if step == 1 { continue; }
                            if step == stop { return marker; }
                            step == 2
                        }) {
                            set marker = marker * 10 + 4;
                            marker
                        } else {
                            if enabled { break; }
                            7
                        }), {
                            if enabled and step == stop { fail "array tail executed after return"; }
                            set marker = marker * 10 + 5;
                            marker
                        }];
                        set total += values[0] + values[1] + values[2];
                    }
                    marker * 1000 + total
                }
                @EntryPoint()
                operation Main() : (Int, Int, Int, Int) {
                    (Run(false, 1), Run(true, 2), Run(true, 3), Run(true, 9))
                }
            }
        "#};
        check_while_condition_result(source, "(555696, 12, 12453, 12454370)");
    }

    #[test]
    fn tuple_assignment_rhs_preserves_snapshot_and_skips_write_on_control_transfer() {
        let source = indoc! {r#"
            namespace Test {
                function Run(stop : Int) : Int {
                    mutable first = 2;
                    mutable second = 3;
                    mutable marker = 0;
                    for step in 1..3 {
                        set (first, second) = (second, {
                            set marker = marker * 10 + step;
                            if step == 1 { continue; }
                            let next = if step == stop {
                                return 900000 + first * 10000 + second * 100 + marker;
                                0
                            } else {
                                if step == 3 { break; }
                                set second = 9;
                                first + 4
                            };
                            next
                        });
                        set marker = marker * 10 + 5;
                    }
                    first * 10000 + second * 100 + marker
                }
                @EntryPoint()
                operation Main() : (Int, Int, Int, Int) {
                    (Run(1), Run(2), Run(3), Run(9))
                }
            }
        "#};
        check_while_condition_result(source, "(31853, 920312, 931853, 31853)");
    }
}

/// Generates syntactically valid Q# programs with return statements at
/// various positions covering all `return_unify` dispatch categories
/// (structured, flag, no-return). Each program wraps one of 12 template
/// patterns in a `namespace Test { function Main() : Int { ... } }` shell.
#[allow(clippy::too_many_lines)]
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn differential_return_unify(source in return_pattern_strategy()) {
        check_semantic_equivalence(&source);
    }
}

mod nested_operand_order {
    use crate::test_utils::{
        check_semantic_equivalence, compile_to_fir, try_eval_fir_entry_with_trace,
    };
    use indoc::indoc;
    use qsc_eval::val::Value;

    fn check_value(source: &str, expected: Value) {
        let (store, package) = compile_to_fir(source);
        let (result, trace) = try_eval_fir_entry_with_trace(&store, package);
        assert_eq!(result, Ok(expected), "original Q# must succeed");
        assert!(
            trace.is_empty(),
            "classical fixture must have no quantum effects"
        );
        check_semantic_equivalence(source);
    }

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

    #[test]
    fn copy_source_return_suppresses_all_replacements() {
        check_value(
            indoc! {r#"
                namespace Test {
                    struct Pair { First : Int, Second : Int }
                    function Run(stop : Bool) : Int {
                        mutable marker = 0;
                        let updated = new Pair {
                            ...{
                                set marker = marker * 10 + 1;
                                if stop { return marker; }
                                new Pair { First = 2, Second = 3 }
                            },
                            Second = {
                                if stop { fail "replacement executed after source return"; }
                                set marker = marker * 10 + 2;
                                4
                            },
                            First = {
                                set marker = marker * 10 + 3;
                                5
                            }
                        };
                        marker * 100 + updated.First * 10 + updated.Second
                    }
                    @EntryPoint()
                    operation Main() : (Int, Int) { (Run(true), Run(false)) }
                }
            "#},
            Value::Tuple(vec![Value::Int(1), Value::Int(12354)].into(), None),
        );
    }

    #[test]
    fn failing_copy_source_precedes_failing_replacement_when_all_fields_are_replaced() {
        let source = indoc! {r#"
            namespace Test {
                struct Pair { First : Int, Second : Int }
                function Source() : Pair { fail "copy source failed" }
                @EntryPoint()
                operation Main() : Int {
                    let updated = new Pair {
                        ...Source(),
                        Second = {
                            fail "replacement failed before source";
                            4
                        },
                        First = 5
                    };
                    updated.First + updated.Second
                }
            }
        "#};
        let (store, package) = compile_to_fir(source);
        let (result, trace) = try_eval_fir_entry_with_trace(&store, package);
        let error = result.expect_err("original Q# must fail in the copy source");
        assert!(
            error.starts_with("UserFail(\"copy source failed\""),
            "unexpected original failure: {error}"
        );
        assert!(trace.is_empty());
        check_semantic_equivalence(source);
    }

    #[test]
    fn replacement_return_suppresses_later_fields_in_source_order() {
        check_value(
            indoc! {r#"
                namespace Test {
                    struct Pair { First : Int, Second : Int }
                    function Run(stop : Bool) : Int {
                        mutable marker = 0;
                        let updated = new Pair {
                            ...{
                                set marker = marker * 10 + 1;
                                new Pair { First = 2, Second = 3 }
                            },
                            Second = {
                                set marker = marker * 10 + 2;
                                if stop { return marker; }
                                4
                            },
                            First = {
                                if stop { fail "later field executed after replacement return"; }
                                set marker = marker * 10 + 3;
                                5
                            }
                        };
                        marker * 100 + updated.First * 10 + updated.Second
                    }
                    @EntryPoint()
                    operation Main() : (Int, Int) { (Run(true), Run(false)) }
                }
            "#},
            Value::Tuple(vec![Value::Int(12), Value::Int(12354)].into(), None),
        );
    }

    #[test]
    fn copy_source_keeps_aggregate_snapshot_when_replacement_mutates_source() {
        check_value(
            indoc! {r#"
                namespace Test {
                    struct Packet { Values : Int[], Tag : Int, Kept : Int }
                    function Run(stop : Bool) : Int {
                        mutable packet = new Packet { Values = [2, 3], Tag = 4, Kept = 5 };
                        let updated = new Packet {
                            ...packet,
                            Tag = {
                                set packet = new Packet { Values = [8, 9], Tag = 6, Kept = 7 };
                                if stop { return 70; }
                                packet.Tag
                            }
                        };
                        let snapshot = updated.Values[0] * 100 + updated.Tag * 10 + updated.Kept;
                        snapshot * 1000 + packet.Values[0] * 100 + packet.Tag * 10 + packet.Kept
                    }
                    @EntryPoint()
                    operation Main() : (Int, Int) { (Run(false), Run(true)) }
                }
            "#},
            Value::Tuple(vec![Value::Int(265_867), Value::Int(70)].into(), None),
        );
    }

    #[test]
    fn discarded_tuple_value_preserves_return_and_suppresses_later_effects() {
        check_value(
            indoc! {r#"
                namespace Test {
                    @EntryPoint()
                    operation Main() : Int {
                        mutable marker = 0;
                        ({
                            set marker = marker * 10 + 1;
                            11
                        }, {
                            set marker = marker * 10 + 2;
                            return marker;
                            22
                        }, {
                            fail "later discarded tuple operand executed";
                            33
                        });
                        fail "statement after discarded tuple executed";
                        -1
                    }
                }
            "#},
            Value::Int(12),
        );
    }
}
