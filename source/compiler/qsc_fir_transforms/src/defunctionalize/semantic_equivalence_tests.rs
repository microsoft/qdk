// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Behavioral equivalence before and after the FIR pipeline: values, failures,
//! quantum effects, and receiver output. Structural and QIR assertions live in
//! the specialization, cross-package, and codegen test modules.

use indoc::formatdoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;

use super::test_cases;

#[test]
fn partial_application_evaluates_capture_before_following_statements() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_CAPTURE_TIMING, 18);
}

#[test]
fn partial_application_snapshots_mutating_capture_once() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_MUTATING_CAPTURE, 134);
}

#[test]
fn partial_application_indexed_capture_survives_assignment_rhs() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_INDEXED_CAPTURE, 3300);
}

#[test]
fn partial_application_indexed_angle_survives_assignment_rhs() {
    check_callable_result(
        r#"
        @EntryPoint() operation Main() : Int {
            use q=Qubit();
            mutable angles=[0.0];
            let op=Rx(angles[0],_);
            set angles w/= 0 <- { op(q); 3.141592653589793 };
            op(q);
            if MResetZ(q) == One { 1 } else { 0 }
        }
        "#,
        0,
    );
}

#[test]
fn partial_application_evaluates_captures_once_in_source_order() {
    check_callable_result(
        r#"
        function Logged(label : String, value : Int) : Int { Message(label); value }
        function Sum(a : Int, b : Int, x : Int) : Int { a+b+x }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            let f=Sum(Logged("first",2),Logged("second",5),_);
            Message("ready");
            100*Apply(f,1)+Apply(f,2)
        }
        "#,
        809,
    );
}

#[test]
fn partial_application_capture_failure_precedes_following_statements() {
    let source = r#"
        function Capture() : Int { fail "capture" }
        function Add(offset : Int, x : Int) : Int { offset+x }
        @EntryPoint() operation Main() : Int {
            let f=Add(Capture(),_);
            Message("must not run");
            f(1)
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the captured operand should fail");
    assert!(error.contains("capture"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn partial_application_capture_stays_inside_its_branch() {
    for flag in [true, false] {
        let source = indoc::formatdoc! {r#"
            function Logged(value : Int) : Int {{ Message("capture"); value }}
            function Add(offset : Int, x : Int) : Int {{ offset+x }}
            function Choose(flag : Bool) : Int {{
                if flag {{
                    let f=Add(Logged(17),_);
                    Message("ready");
                    f(1)
                }} else {{ 7 }}
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 18 } else { 7 });
    }
}

#[test]
fn partial_application_capture_runs_once_per_loop_iteration() {
    check_callable_result(
        r#"
        function Logged(value : Int) : Int { Message($"capture:{value}"); value }
        function Add(offset : Int, x : Int) : Int { offset+x }
        @EntryPoint() operation Main() : Int {
            mutable total=0;
            for i in 1..3 {
                let f=Add(Logged(i),_);
                Message("ready");
                set total+=f(1);
            }
            total
        }
        "#,
        9,
    );
}

#[test]
fn partial_application_functors_preserve_capture_evaluation() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for enabled in [false, true] {
            let args = if functor.matches("Controlled").count() == 2 {
                "[outer], ([inner], target)"
            } else {
                "[outer], target"
            };
            let source = indoc::formatdoc! {r#"
                function Angle() : Double {{ Message("capture"); 1.5707963267948966 }}
                @EntryPoint() operation Main() : Int {{
                    use outer=Qubit();
                    use inner=Qubit();
                    use target=Qubit();
                    if {enabled} {{ X(outer); X(inner); H(target); }}
                    let op=Ry(Angle(),_);
                    Message("ready");
                    {functor} op({args});
                    Reset(outer);
                    Reset(inner);
                    if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                }}
            "#};
            check_callable_result(&source, i64::from(enabled && !functor.contains("Adjoint")));
        }
    }
}

#[test]
fn direct_struct_fields_preserve_declaration_order() {
    for (source, expected) in test_cases::direct_struct_field_order_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_fields_preserve_initializer_evaluation_order() {
    check_callable_result(
        r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Log(label : String, n : Int) : Int { Message(label); n }
        function Read(p : Payload) : Int { 100*p.Head+p.F(p.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { Tail=Log("tail",7), F=Inc, Head=Log("head",4) })
        }
        "#,
        408,
    );
}

#[test]
fn struct_copy_factory_preserves_returned_fields() {
    for (source, expected) in test_cases::struct_copy_factory_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn nested_struct_copy_factory_preserves_returned_fields() {
    for (source, expected) in test_cases::nested_struct_copy_factory_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_copy_factory_preserves_returned_fields() {
    check_callable_result(test_cases::DIRECT_STRUCT_COPY_FACTORY, 408);
}

#[test]
fn type_constructor_arguments_preserve_underlying_fields() {
    for (source, expected) in test_cases::type_constructor_argument_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_copy_factory_runs_once_before_overrides() {
    check_callable_result(
        r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Original(n : Int) : Payload {
            Message("copy");
            new Payload { Head=n+1, F=Inc, Tail=2*n+1 }
        }
        function Override() : Int { Message("override"); 4 }
        function Read(payload : Payload) : Int { 100*payload.Head+payload.F(payload.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { ...Original(3), F=Inc, Head=Override() })
        }
        "#,
        408,
    );
}

#[test]
fn struct_copy_factory_failure_precedes_override_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Original() : Payload { fail "copy" }
        function Override() : Int { fail "override" }
        function Read(payload : Payload) : Int { 100*payload.Head+payload.F(payload.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { ...Original(), F=Inc, Head=Override() })
        }
    "#;
    let error =
        crate::test_utils::eval_qsharp_original(source).expect_err("the copy source should fail");
    assert!(error.contains("copy"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn stored_aggregate_arguments_preserve_creation_time_values() {
    for (source, expected) in test_cases::stored_aggregate_snapshot_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn conditional_stored_aggregate_arguments_preserve_creation_time_values() {
    for (source, expected) in test_cases::conditional_stored_aggregate_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn struct_branch_arguments_preserve_fields_and_captures() {
    for (source, expected) in test_cases::struct_branch_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn nested_struct_branch_arguments_preserve_fields_and_captures() {
    for (source, expected) in test_cases::nested_struct_branch_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn struct_branch_fields_preserve_evaluation_order() {
    for flag in [true, false] {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Read(payload : Payload) : Int {{
                1000*payload.Head+100*payload.F(3)+payload.Tail
            }}
            function Choose(flag : Bool) : Int {{
                let selected = if flag {{ Make(1) }} else {{ Make(3) }};
                Read(new Payload {{ Tail=Log("tail",7), F=selected, Head=Log("head",2) }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 2407 } else { 2607 });
    }
}

#[test]
fn struct_branch_copy_source_is_evaluated_once_before_overrides() {
    for flag in [true, false] {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Original() : Payload {{
                Message("copy");
                new Payload {{ Head=2, F=Make(0), Tail=7 }}
            }}
            function Read(payload : Payload) : Int {{
                1000*payload.Head+100*payload.F(3)+payload.Tail
            }}
            function Choose(flag : Bool) : Int {{
                let selected = if flag {{ Make(1) }} else {{ Make(3) }};
                Read(new Payload {{ ...Original(), F=selected, Head=Log("override",2) }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 2407 } else { 2607 });
    }
}

#[test]
fn struct_branch_copy_snapshot_precedes_mutating_override() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        function Read(payload : Payload) : Int {
            1000*payload.Head+100*payload.F(3)+payload.Tail
        }
        function Choose(flag : Bool) : Int {
            let selected = if flag { Inc } else { Twice };
            mutable original = new Payload { Head=2, F=Inc, Tail=7 };
            Read(new Payload {
                ...original,
                F=selected,
                Tail={ set original w/= Head <- 99; 7 }
            })
        }
        @EntryPoint() operation Main() : Int { Choose(true) }
    "#;
    check_callable_result(source, 2407);
}

#[test]
fn nested_local_struct_argument_keeps_initializer_effects_once() {
    let source = r#"
        struct Payload { F : Int -> Int, N : Int }
        function Inc(x : Int) : Int { x+1 }
        function Log(n : Int) : Int { Message("field"); n }
        function Read(pair : (Int, Payload)) : Int {
            let (prefix, payload) = pair;
            prefix+payload.F(payload.N)
        }
        @EntryPoint() operation Main() : Int {
            let pair = (9, new Payload { F=Inc, N=Log(3) });
            Message("ready");
            Read(pair)
        }
    "#;
    check_callable_result(source, 13);
}

#[test]
fn struct_branch_preserves_first_field_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function FailField(label : String) : Int { fail label }
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        function Read(payload : Payload) : Int { payload.F(payload.Head)+payload.Tail }
        function Choose(flag : Bool) : Int {
            let selected = if flag { Inc } else { Twice };
            Read(new Payload { Tail=FailField("tail"), F=selected, Head=FailField("head") })
        }
        @EntryPoint() operation Main() : Int { Choose(true) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the first field initializer should fail");
    assert!(error.contains("tail"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn single_controlled_branches_preserve_captures_and_effects() {
    for (source, expected) in test_cases::controlled_branch_cases("Controlled") {
        check_callable_result(&source, expected);
    }
}

#[test]
fn double_controlled_branches_preserve_captures_and_effects() {
    for (source, expected) in test_cases::controlled_branch_cases("Controlled Controlled") {
        check_callable_result(&source, expected);
    }
}

#[test]
fn adjoint_controlled_branches_preserve_captures_and_effects() {
    for functor in [
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, expected) in test_cases::controlled_branch_cases(functor) {
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn conditional_branches_preserve_independent_nested_payloads() {
    for (source, expected) in test_cases::nested_branch_payload_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_compound_capture_preserves_each_return_boundary() {
    for (source, expected) in test_cases::forwarded_capture_environment_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_compound_captures_keep_each_field_and_call_occurrence() {
    check_callable_result(test_cases::FORWARDED_MULTI_FIELD_CAPTURES, 441_901);
}

#[test]
fn forwarded_compound_capture_preserves_mutable_caller_snapshot() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int {
            let values = [n];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(n+1) }
        @EntryPoint() operation Main() : Int {
            mutable n = 2;
            let f = Forward(n);
            set n = 99;
            f(0)
        }
        "#,
        3,
    );
}

#[test]
fn producer_local_computation_survives_capture_rebinding() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int {
            let adjusted = n+1;
            let values = [adjusted];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(2*n) }
        @EntryPoint() operation Main() : Int {
            let f = Forward(3);
            f(0)
        }
        "#,
        7,
    );
}

#[test]
fn forwarded_compound_capture_keeps_effectful_operand_at_creation() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 2 }
        function Make(n : Int) : Int -> Int {
            let values = [n];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(n+1) }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Forward(Next(q));
            let first = f(0);
            let second = f(1);
            let flipped = MResetZ(q) == One;
            if flipped { 10*first+second } else { 0 }
        }
        "#,
        34,
    );
}

#[test]
fn conditional_hof_calls_preserve_embedded_and_runtime_captures() {
    for (source, expected) in test_cases::conditional_capture_layout_cases() {
        check_callable_result(&source, expected);
    }
}

/// Regression for controlled dispatch of a *capturing* closure passed to a
/// higher-order operation whose callable parameter is **not** the first
/// argument. The HOF applies `Controlled op(ctls, q)`, so rewrite must nest the
/// closure's captures inside the base input tuple beneath the control register
/// (`(ctls, (capture0, capture1, q))`) rather than appending them as trailing
/// top-level siblings of `(ctls, q)`. A mis-placed capture would either crash
/// downstream control/input splitting or diverge from the original semantics.
///
/// The control qubit is prepared |1> so the controlled rotation actually fires;
/// the captured angles are threaded through a partial application so the closure
/// carries two ordered captures across the control boundary (exercising the
/// multi-capture nesting order, not just placement).
#[test]
fn controlled_capturing_closure_nonzero_param_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation RotOp(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {
                Rx(a, q);
                Rz(b, q);
            }
            operation ApplyCtl(ctls : Qubit[], op : Qubit => Unit is Ctl, q : Qubit) : Unit {
                Controlled op(ctls, q);
            }
            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use q = Qubit();
                X(ctl);
                let a = 3.141592653589793;
                let b = 1.5707963267948966;
                let op = RotOp(a, b, _);
                ApplyCtl([ctl], op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for higher-order argument cleanup. `GetOp(q)` performs `X(q)`
/// before returning the named callable `X`; specializing `ApplyOp(op, q)`
/// consumes `op`, but cleanup must retain the initializer's effect.
#[test]
fn recorded_direct_rewrite_cleanup_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(q);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the removal gate on a rewritten higher-order argument. The
/// factory is a classical `function`, but that does not make its evaluation
/// safe to delete: its body can still fail. Cleanup must keep the
/// binding so the division failure stays observable.
#[test]
fn rewritten_hof_arg_cleanup_retains_fallible_function_factory() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function GetOp(divisor : Int) : Qubit => Unit {
                let ignored = 1 / divisor;
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(0);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for demoting a dead callable binding that must still run. The
/// captured angle comes from `GetAngle`, which flips the qubit, so dropping the
/// binding outright would lose that effect. Cleanup keeps the evaluation and
/// discards only the consumed callable value.
#[test]
fn demoted_dead_callable_binding_retains_capture_effect() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetAngle(q : Qubit) : Double {
                X(q);
                0.0
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = Rx(GetAngle(q), _);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for `prune_dead_callable_locals_in_block`. The initializer is
/// intentionally unused and never passed to a higher-order operation, so it
/// must be retained by the global dead-local pruner solely for its `X(q)`.
#[test]
fn global_dead_local_pruner_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let unused = GetOp(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Generates syntactically valid Q# programs exercising defunctionalization's
/// key code paths: lambda arguments, partial application, and direct callable
/// references passed to higher-order functions.
#[cfg(feature = "slow-proptest-tests")]
fn defunc_pattern_strategy() -> impl Strategy<Value = String> {
    let val = || 0..50i64;

    prop_oneof![
        // 1. Lambda passed as argument to a higher-order function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(x -> x + {a}, {b})
                }}
            }}
        "}),
        // 2. Partial application of a two-argument function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Add(x : Int, y : Int) : Int {{ x + y }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Add({a}, _), {b})
                }}
            }}
        "}),
        // 3. Direct callable reference as argument.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Double(x : Int) : Int {{ x * 2 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Double, {a})
                }}
            }}
        "}),
        // 4. Nested higher-order calls: function returning a lambda.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function MakeAdder(n : Int) : Int -> Int {{ x -> x + n }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(MakeAdder({a}), {b})
                }}
            }}
        "}),
    ]
}

/// Generates multi-capture closures whose two varying captures have distinct
/// values and are used in non-commutative operations, exercising capture order.
/// The three-capture case also includes a constant capture of `1`.
#[cfg(feature = "slow-proptest-tests")]
fn multi_capture_strategy() -> impl Strategy<Value = String> {
    // Use distinct non-zero values so swapped captures produce a different result.
    (2..20i64, 1..10i64)
        .prop_filter("a must differ from b", |(a, b)| a != b && *b != 0)
        .prop_flat_map(|(a, b)| {
            prop_oneof![
                // Two captures used in non-commutative subtraction.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a - b + x, 0)
                        }}
                    }}
                "}),
                // Two captures used in non-commutative division.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a / b + x, 0)
                        }}
                    }}
                "}),
                // Three captures in position-sensitive expression.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            let c = 1;
                            Apply(x -> (a - b) * c + x, 0)
                        }}
                    }}
                "}),
            ]
        })
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn proptest_defunctionalize_preserves_semantics(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]
    #[test]
    fn proptest_multi_capture_ordering_preserves_semantics(source in multi_capture_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

/// Regression for the `Multi ⊔ Multi` (nested dispatch on both sides) join: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are themselves dynamic conditionals, and the *same* callable (`X`)
/// reaches the local from both branches under different guards.
///
/// The lattice merge must not deduplicate the false-branch occurrence of `X`
/// by callable identity — doing so drops the `!outer && rb` dispatch arm and
/// makes that path fall through to the outer default (`Z`) instead of applying
/// `X`. The fixture pins `outer == false` (`a` stays |0>) and the false-branch
/// inner guard `rb == One` (`b` is |1>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// applies `Z(q)` (measuring `Zero`), diverging in both return value and effect
/// trace. The guards are pure reads of pre-measured `Result` locals so the
/// fixture isolates the lattice merge from condition-hoisting concerns.
#[test]
fn multi_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a stays |0> so the outer guard is false; b is |1> so the
                // false-branch inner guard is true — the dispatch arm the
                // identity-dedup would drop.
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             if rb == One { X } else { Y }
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Single ⊔ Multi` join: a callable-valued local is
/// selected by an outer dynamic `if` whose *true* branch is a single concrete
/// callable (`X`) and whose *false* branch is itself a dynamic conditional that
/// can also yield `X` (under its own guard).
///
/// The lattice merge must not deduplicate the true-branch `X` against the
/// occurrence already present in the false-branch `Multi` — doing so drops the
/// `outer` dispatch arm and reroutes the `outer == true` path through the
/// false-branch's inner guards instead of unconditionally applying `X`. The
/// fixture pins `outer == true` (`a` is |1>) and the false-branch inner guard
/// `rb == One` false (`b` stays |0>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// falls through to the false-branch default `Z(q)` (measuring `Zero`),
/// diverging in both return value and effect trace. The guards are pure reads
/// of pre-measured `Result` locals so the fixture isolates the lattice merge
/// from condition-hoisting concerns.
#[test]
fn single_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a is |1> so the outer guard is true — op must be the
                // true-branch `X`; b stays |0> so the false-branch inner guard
                // is false, the arm the identity-dedup would route through.
                X(a);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             X
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Multi ⊔ Multi` join's "unmodified variable" fast path: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are dynamic conditionals that yield the *same set of callables*
/// (`X`/`Z`) but under *different* inner guards (`rb` in the true branch, `rc`
/// in the false branch).
///
/// The merge must not treat the two branches as an unmodified variable just
/// because the callable identities coincide — the guards differ, so keeping the
/// true-branch chain drops the outer condition and reroutes the `outer == false`
/// path through the true branch's `rb` guard instead of the false branch's `rc`
/// guard. The fixture pins `outer == false` (`a` stays |0>), `rb == One`
/// (`b` is |1>), and `rc == Zero` (`c` stays |0>): the original applies `Z(q)`
/// (measuring `Zero`) while the buggy rewrite applies `X(q)` (measuring `One`),
/// diverging in both return value and effect trace.
#[test]
fn multi_multi_same_callables_different_guards_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                use c = Qubit();
                // a stays |0> (outer guard false); b is |1> (rb == One);
                // c stays |0> (rc == Zero).
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let rc = MResetZ(c);
                let op = if ra == One {
                             if rb == One { X } else { Z }
                         } else {
                             if rc == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// A conditional callable is bound from a mutable guard that is never reassigned.
/// Reading the guard at the apply site therefore selects the same callable as
/// at the binding site (`X`), preserving the result and quantum effect trace.
///
/// The reassigned-guard counterpart checks the raw pass's `DynamicCallable`
/// diagnostic in
/// `defunctionalize::tests::analysis::reaching_def_conditional_callable_reassigned_guard_dynamic`.
/// The full pipeline defers that diagnostic rather than rejecting it here.
#[test]
fn guard_var_never_reassigned_after_binding_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                let ra = MResetZ(a);
                // `flag` is mutable but never reassigned after the binding, so
                // hoisting its read to the apply site is safe and dispatch is
                // preserved.
                mutable flag = ra == One;
                let op = if flag { X } else { Z };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

#[test]
fn embedded_callable_identity_preserves_execution_results() {
    for (source, expected) in test_cases::embedded_callable_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn independently_created_equivalent_callable_captures_preserve_results() {
    let source = test_cases::embedded_callable_source("Inc", "Inc", "Apply(a, 3)*100+Apply(b, 3)");
    check_callable_result(&source, 505);
}

#[test]
fn factory_returned_partial_applications_preserve_operand_values_per_occurrence() {
    for call in [
        "first(0)*10000+second(0)",
        "Invoke(first, 0)*10000+Invoke(second, 0)",
    ] {
        let source = formatdoc! {r#"
            function Identity(x : Int) : Int {{ x }}
            function Compose(
                f : Int -> Int, a : Int[], b : Int[], c : Int[], d : Int[], x : Int
            ) : Int {{
                f(x)+1000*a[0]+100*b[0]+10*c[0]+d[0]
            }}
            function Make(
                f : Int -> Int, a : Int[], b : Int[], c : Int[], d : Int[]
            ) : Int -> Int {{
                Compose(f, a, b, c, d, _)
            }}
            function Invoke(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{
                let first = Make(Identity, [1], [2], [3], [4]);
                let second = Make(Identity, [5], [6], [7], [8]);
                {call}
            }}
        "#};
        check_callable_result(&source, 12_345_678);
    }
}

#[test]
fn foreign_nested_callable_captures_return_613() {
    let library = r#"
        namespace Lib {
            function Add(n : Int) : Int -> Int { x -> x+n }
            function Wrap(f : Int -> Int, n : Int) : Int -> Int { x -> f(x)+n }
            function Apply<'T,'U>(f : 'T -> 'U, x : 'T) : 'U { f(x) }
            export Add, Wrap, Apply;
        }
    "#;
    let source = r#"
        @EntryPoint() operation Main() : Int {
            let first = Lib.Wrap(Lib.Add(2), 3);
            let second = Lib.Wrap(Lib.Add(5), 7);
            Lib.Apply(first, 1)*100 + Lib.Apply(second, 1)
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(613))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn flat_closure_preserves_direct_and_forwarded_results() {
    crate::test_utils::check_semantic_equivalence(
        r#"
        function Flat(offset : Int, scale : Int) : Int -> Int {
            value -> (value+offset)*scale
        }
        function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
        @EntryPoint() operation Main() : (Int, Int) {
            let first = Flat(3, 2);
            (first(1), Invoke(first, 3))
        }
        "#,
    );
}

#[test]
fn nested_callable_capture_slots_can_be_removed_or_expanded() {
    for (first, second, argument, expected) in [
        ("Inc", "Twice", 3, 406),
        ("Make(2, 3)", "Inc", 3, 1104),
        ("Inc", "Make(1, 7)", 3, 422),
        ("Make(2, 3)", "Make(5, 7)", 1, 512),
    ] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Make(offset : Int, scale : Int) : Int -> Int {{
                x -> scale*x+offset
            }}
            function Combine(f : Int -> Int, g : Int -> Int, x : Int) : Int {{
                let combined = y -> 100*f(y)+g(y);
                combined(x)
            }}
            @EntryPoint() operation Main() : Int {{
                Combine({first}, {second}, {argument})
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn captured_lambda_functor_applications_preserve_quantum_effects() {
    for call in [
        "Combine(first, second, target)",
        "Adjoint Combine(first, second, target)",
        "Controlled Combine([control], (first, second, target))",
        "Controlled Adjoint Combine([control], (first, second, target))",
    ] {
        let source = formatdoc! {r#"
            operation Rotate(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {{
                Rx(a, q);
                Rz(b, q);
            }}
            operation Combine(
                first : Qubit => Unit is Adj + Ctl,
                second : Qubit => Unit is Adj + Ctl,
                q : Qubit
            ) : Unit is Adj + Ctl {{
                let combined : Qubit => Unit is Adj + Ctl =
                    target => {{ first(target); second(target); }};
                combined(q);
            }}
            @EntryPoint() operation Main() : Int {{
                use control = Qubit();
                use target = Qubit();
                X(control);
                let first = Rotate(0.4, 0.2, _);
                let second = Rotate(0.7, 0.3, _);
                {call};
                Reset(target);
                Reset(control);
                0
            }}
        "#};
        check_callable_result(&source, 0);
    }
}

#[test]
fn embedded_callable_identity_distinguishes_adjoint_application() {
    check_callable_result(
        r#"
        function Wrap(op : Qubit => Unit is Adj + Ctl) : Qubit => Unit is Adj + Ctl {
            q => { op(q); Z(q); }
        }
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint() operation Main() : Int {
            use a = Qubit();
            use b = Qubit();
            let forward = Wrap(S);
            let inverse = Wrap(Adjoint S);
            H(a);
            Apply(forward, a);
            Apply(forward, a);
            H(a);
            H(b);
            Apply(forward, b);
            Apply(inverse, b);
            H(b);
            let left = MResetZ(a);
            let right = MResetZ(b);
            if left == One and right == Zero { 10 } else { 0 }
        }
        "#,
        10,
    );
}

#[test]
fn branch_split_in_specialized_clone_preserves_both_capture_environments() {
    for (prepare, expected) in [("", 16), ("X(flag);", 13)] {
        for call in ["selected(10)", "Apply(selected, 10)"] {
            let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x+1 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Choose(seed : Int -> Int, flag : Bool) : Int {{
                    let first = seed(2);
                    let second = seed(5);
                    let selected = if flag {{
                        x -> x+first
                    }} else {{
                        x -> x+second
                    }};
                    {call}
                }}
                @EntryPoint() operation Main() : Int {{
                    use flag = Qubit();
                    {prepare}
                    Choose(Inc, MResetZ(flag) == One)
                }}
            "#};
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn returned_compound_capture_substitutions_survive_forwarding() {
    for (source, expected) in test_cases::compound_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_nested_tuple_preserves_callable_and_scalar_fields() {
    for (choose, expected) in [("true", 29), ("false", 39)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Read(pair : (Int, (Int -> Int, Int)), choose : Bool) : Int {{
                if choose {{
                    let (prefix, (f, x)) = pair;
                    prefix+f(x)
                }} else {{
                    let (prefix, (f, x)) = pair;
                    10+prefix+f(x)
                }}
            }}
            function Forward(pair : (Int, (Int -> Int, Int)), choose : Bool) : Int {{
                Read(pair, choose)
            }}
            @EntryPoint() operation Main() : Int {{
                Forward((9, (Make(17), 3)), {choose})
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_struct_preserves_callable_and_scalar_fields() {
    check_callable_result(
        r#"
        struct Pair { Apply : Int -> Int, Value : Int }
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Read(pair : Pair) : Int { pair.Apply(pair.Value) }
        function Forward(pair : Pair) : Int { Read(pair) }
        @EntryPoint() operation Main() : Int {
            Forward(new Pair { Apply = Make(17), Value = 3 })
        }
        "#,
        20,
    );
}

#[test]
fn callable_array_with_no_scalar_sibling_preserves_member_captures() {
    for (members, expected) in [("[]", 0), ("[Make(2)]", 3), ("[Make(2), Make(5)]", 36)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Read(functions : (Int -> Int)[]) : Int {{
                mutable result = 0;
                for f in functions {{
                    set result = 10*result+f(1);
                }}
                result
            }}
            @EntryPoint() operation Main() : Int {{ Read({members}) }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn foreign_forwarded_global_callable_retains_its_identity() {
    let library = r#"
        namespace Lib {
            function Forward<'T>(f : 'T -> 'T) : 'T -> 'T { f }
            export Forward;
        }
    "#;
    let source = r#"
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        @EntryPoint() operation Main() : Int {
            let a = Lib.Forward(Inc);
            let b = Lib.Forward(Twice);
            a(3)*100+b(3)
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(406))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn returned_record_copy_update_rebinds_factory_parameters() {
    check_callable_result(test_cases::RECORD_COPY_UPDATE, 1919);
}

#[test]
fn compound_capture_preserves_repeat_count_and_range_bounds() {
    check_callable_result(test_cases::COMPOUND_REPEAT_AND_RANGE, 31721);
}

#[test]
fn compound_capture_factory_preserves_array_repeat_failure() {
    let source = r#"
        function Make(count : Int) : Int -> Int {
            let values = [17, size = count];
            x -> values[0]+x
        }
        @EntryPoint() operation Main() : Int {
            let f = Make(-1);
            f(1)
        }
    "#;
    assert!(crate::test_utils::eval_qsharp_original(source).is_err());
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn entry_expression_captures_keep_their_own_scope() {
    let source = "namespace Test { function Apply(f : Int -> Int, x : Int) : Int { f(x) } }";
    let entry = "{ let offset = 17; let f = x -> x+offset; Test.Apply(f, 3) }";
    let (mut store, package) = crate::test_utils::compile_to_fir_with_entry(source, entry);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(20))
    );
    crate::test_utils::assert_full_pipeline_succeeds("entry capture", &mut store, package);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(20))
    );
}

#[test]
fn effectful_factory_operand_is_evaluated_once_before_repeated_calls() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 1 }
        function Make(values : Int[]) : Int -> Int { x -> values[0]+x }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Make([Next(q)]);
            let first = f(1);
            let second = f(2);
            let flipped = MResetZ(q) == One;
            if flipped { 100*first+second } else { 0 }
        }
        "#,
        203,
    );
}

#[test]
fn direct_factory_closure_preserves_nested_control_layers() {
    for call in [
        "Controlled Controlled op([outer], ([inner], target))",
        "Controlled Controlled Adjoint op([outer], ([inner], target))",
    ] {
        let source = formatdoc! {r#"
            operation Rotate(angle : Double, q : Qubit) : Unit is Adj + Ctl {{
                Rx(angle, q);
            }}
            function Make(angle : Double) : Qubit => Unit is Adj + Ctl {{
                Rotate(angle, _)
            }}
            @EntryPoint() operation Main() : Int {{
                use outer = Qubit();
                use inner = Qubit();
                use target = Qubit();
                X(outer);
                X(inner);
                let op = Make(3.141592653589793);
                {call};
                Reset(outer);
                Reset(inner);
                if MResetZ(target) == One {{ 1 }} else {{ 0 }}
            }}
        "#};
        check_callable_result(&source, 1);
    }
}

#[test]
fn forwarded_newtype_preserves_callable_and_payload() {
    check_callable_result(
        r#"
        newtype Pair = (Int -> Int, Int);
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Read(pair : Pair) : Int {
            let (f, value) = pair!;
            f(value)
        }
        function Forward(pair : Pair) : Int { Read(pair) }
        @EntryPoint() operation Main() : Int { Forward(Pair(Make(17), 3)) }
        "#,
        20,
    );
}

#[test]
fn nested_factory_closure_rebinds_captures_across_two_return_boundaries() {
    check_callable_result(
        r#"
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Wrap(f : Int -> Int) : Int -> Int { x -> f(x)+1 }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            let first = Wrap(Wrap(Make(2)));
            let second = Wrap(Wrap(Make(5)));
            100*Apply(first, 1)+Apply(second, 1)
        }
        "#,
        508,
    );
}

#[test]
fn closure_dispatch_preserves_tuple_argument_grouping() {
    for call in ["f(2, 3)", "Apply(f, (2, 3))"] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : (Int, Int) -> Int {{
                (a, b) -> 100*offset+10*a+b
            }}
            function Apply(f : (Int, Int) -> Int, pair : (Int, Int)) : Int {{ f(pair) }}
            @EntryPoint() operation Main() : Int {{
                let f = Make(7);
                {call}
            }}
        "#};
        check_callable_result(&source, 723);
    }
}

#[test]
fn factory_capture_keeps_scalar_value_before_caller_mutation() {
    for (source, expected) in test_cases::mutable_scalar_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn factory_capture_keeps_array_value_before_caller_copy_update() {
    check_callable_result(test_cases::MUTABLE_ARRAY_CAPTURE, 18);
}

#[test]
fn loop_factory_occurrences_keep_their_iteration_values() {
    check_callable_result(test_cases::LOOP_FACTORY_CAPTURES, 234);
}

#[test]
fn callable_factory_tuple_output_preserves_stronger_functors() {
    check_callable_result(
        r#"
        function Make() : ((Qubit => Unit is Adj + Ctl), Int) { (X, 7) }
        operation Consume(factory : Unit -> ((Qubit => Unit), Int), q : Qubit) : Int {
            let (op, tag) = factory();
            op(q);
            tag
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
        "#,
        7,
    );
}

#[test]
fn nested_callable_factory_output_preserves_destructured_types() {
    check_callable_result(
        r#"
        function Make() : (Int, ((Qubit => Unit is Adj + Ctl)[], Int)) {
            (3, ([X, Z], 7))
        }
        operation Consume(factory : Unit -> (Int, ((Qubit => Unit)[], Int)), q : Qubit) : Int {
            let (prefix, (ops, suffix)) = factory();
            for op in ops { op(q); }
            10*prefix+suffix
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
        "#,
        37,
    );
}

#[test]
fn copy_updated_capture_keeps_operation_effects_at_factory_time() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 7 }
        operation Make(q : Qubit) : Int -> Int {
            let values = [1, 2] w/ 1 <- Next(q);
            x -> 100*values[0]+10*values[1]+x
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Make(q);
            let first = f(3);
            let second = f(4);
            if MResetZ(q) == One { 1000*first+second } else { 0 }
        }
        "#,
        173_174,
    );
}

#[test]
fn callable_factory_early_returns_keep_distinct_environments() {
    for (source, expected) in test_cases::early_return_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn guarded_fallible_factory_is_not_evaluated_on_untaken_branch() {
    let source = r#"
        function Make(count : Int) : Int -> Int {
            let values = [17, size = count];
            x -> values[0]+x
        }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            use flag = Qubit();
            if MResetZ(flag) == One {
                Apply(Make(-1), 1)
            } else {
                Apply(Make(1), 1)
            }
        }
    "#;
    check_callable_result(source, 18);
}

#[test]
fn caller_shadowing_does_not_rebind_forwarded_closure() {
    check_callable_result(test_cases::SHADOWED_CAPTURE, 18116);
}

#[test]
fn immutable_capture_snapshot_preserves_value_after_caller_mutation() {
    check_callable_result(test_cases::IMMUTABLE_CAPTURE_SNAPSHOT, 18);
}

#[test]
fn branch_dispatch_does_not_replay_a_mutated_capture_operand() {
    for (prepare, expected) in [("", 6), ("X(flag);", 18)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{
                use flag = Qubit();
                {prepare}
                mutable offset = 17;
                let selected = if MResetZ(flag) == One {{ Make(offset) }} else {{ Make(5) }};
                set offset = 99;
                Apply(selected, 1)
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn foreign_callable_factory_output_retains_nested_functor_types() {
    let library = r#"
        namespace Lib {
            function Make() : (Int, ((Qubit => Unit is Adj + Ctl), Int)) {
                (3, (X, 7))
            }
            export Make;
        }
    "#;
    let source = r#"
        operation Consume(factory : Unit -> (Int, ((Qubit => Unit), Int)), q : Qubit) : Int {
            let (prefix, (op, suffix)) = factory();
            op(q);
            10*prefix+suffix
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Lib.Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(37))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn invalid_copy_updated_capture_preserves_factory_failure() {
    let source = r#"
        function Make(index : Int) : Int -> Int {
            let values = [17] w/ index <- 99;
            x -> values[0]+x
        }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int { Apply(Make(2), 1) }
    "#;
    for (label, (store, package)) in [
        ("original", crate::test_utils::compile_to_fir(source)),
        (
            "transformed",
            crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Full),
        ),
    ] {
        let error = crate::test_utils::try_eval_fir_entry(&store, package)
            .expect_err("the factory's invalid copy-update must fail");
        // Rematerializing the index can change its source span, not its failure.
        assert!(error.starts_with("IndexOutOfRange(2,"), "{label}: {error}");
    }
}

#[test]
fn mixed_branch_dispatch_preserves_constant_sibling_capture_and_effects() {
    for (prepare, expected) in [("", 603), ("X(flag);", 403)] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Make(offset : Int) : Int -> Int {{
                Message("make");
                x -> x+offset
            }}
            function Both(first : Int -> Int, second : Int -> Int, x : Int) : Int {{
                100*first(x)+second(x)
            }}
            @EntryPoint() operation Main() : Int {{
                use flag = Qubit();
                {prepare}
                let chosen = if MResetZ(flag) == One {{ Inc }} else {{ Twice }};
                let captured = Make(0);
                Message("ready");
                Both(chosen, captured, 3)
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn capture_free_callable_array_members_keep_distinct_embedded_identities() {
    for (source, expected) in test_cases::callable_array_identity_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn closure_retained_in_returned_callable_array_remains_executable() {
    check_callable_result(test_cases::RETURNED_CALLABLE_ARRAY, 407);
}

fn check_callable_result(source: &str, expected: i64) {
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(expected),
    );
}
