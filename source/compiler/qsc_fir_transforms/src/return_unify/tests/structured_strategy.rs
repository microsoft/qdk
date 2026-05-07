// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn no_op_function_without_returns() {
    // A function with no return statements should pass through unchanged.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1;
                x + 2
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let x : Int = 1;
                    x + 2
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_trailing_return() {
    // `return x;` as the last statement should be simplified to just `x`.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 42;
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    42
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guard_clause_pattern() {
    // `if cond { return a; } b` → `if cond { a } else { b }`
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if true {
                        1
                    } else {
                        0
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn multiple_guard_clauses() {
    // Three sequential if-return → nested if-else chain.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                if false {
                    return 2;
                }
                if true {
                    return 3;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if true {
                        1
                    } else {
                        if false {
                            2
                        } else {
                            if true {
                                3
                            } else {
                                0
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn both_branches_return() {
    // `if cond { return a; } else { return b; }` → `if cond { a } else { b }`
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    return 2;
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if true {
                        1
                    } else {
                        2
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn both_branches_return_with_qubit_scope() {
    // Both branches return inside a qubit scope — tests interaction with
    // `replace_qubit_allocation` which inserts release calls.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Bool {
                use q = Qubit();
                let r = M(q);
                Reset(q);
                if r == One {
                    return true;
                } else {
                    return false;
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Bool {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let r : Result = M(q);
                    Reset(q);
                    if r == One {
                        {
                            let
                            @generated_ident_43 : Bool = true;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_43
                        }

                    } else {
                        {
                            let
                            @generated_ident_55 : Bool = false;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_55
                        }

                    }

                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_in_nested_block() {
    // `{ { return x; } }` → `{ { x } }`
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                {
                    {
                        return 10;
                    }
                };
                5
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    {
                        {
                            10
                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn unit_returning_with_return() {
    // `return ();` patterns in Unit-returning operations.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Unit {
                if true {
                    return ();
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Unit {
                body {
                    if true {
                        ()
                    } else {}

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn explicit_specialization_bodies_are_return_unified() {
    check_structure(
        indoc! {r#"
            namespace Test {
                operation Foo(n : Int, q : Qubit) : Unit is Adj + Ctl {
                    body ... {
                        if n == 0 {
                            return ();
                        }
                        H(q);
                    }
                    adjoint ... {
                        if n == 1 {
                            return ();
                        }
                        X(q);
                    }
                    controlled (ctls, ...) {
                        if Length(ctls) == 0 {
                            return ();
                        }
                        Controlled H(ctls, q);
                    }
                    controlled adjoint (ctls, ...) {
                        if Length(ctls) == 1 {
                            return ();
                        }
                        Controlled X(ctls, q);
                    }
                }

                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    Foo(1, q);
                }
            }
        "#},
        &["Foo", "Main"],
        &expect![[r#"
callable Foo: input_ty=(Int, Qubit), output_ty=Unit
    body: block_ty=Unit
        [0] Expr If(cond=BinOp(Eq)[ty=Bool], then=Block[ty=Unit], else=Block[ty=Unit])
    adj: block_ty=Unit
        [0] Expr If(cond=BinOp(Eq)[ty=Bool], then=Block[ty=Unit], else=Block[ty=Unit])
    ctl: block_ty=Unit
        [0] Expr If(cond=BinOp(Eq)[ty=Bool], then=Block[ty=Unit], else=Block[ty=Unit])
    ctl_adj: block_ty=Unit
        [0] Expr If(cond=BinOp(Eq)[ty=Bool], then=Block[ty=Unit], else=Block[ty=Unit])
callable Main: input_ty=Unit, output_ty=Unit
    body: block_ty=Unit
        [0] Local(Immutable, q: Qubit): Call[ty=Qubit]
        [1] Semi Call[ty=Unit]
        [2] Semi Call[ty=Unit]"#]],
    );
}

#[test]
fn simulatable_intrinsic_body_is_return_unified() {
    check_structure(
        indoc! {r#"
            namespace Test {
                @SimulatableIntrinsic()
                operation Foo() : Int {
                    mutable i = 0;
                    while i < 3 {
                        if i == 1 {
                            return i;
                        }
                        i += 1;
                    }
                    -1
                }

                @EntryPoint()
                operation Main() : Int {
                    Foo()
                }
            }
        "#},
        &["Foo", "Main"],
        &expect![[r#"
            callable Foo: input_ty=Unit, output_ty=Int
                simulatable: block_ty=Int
                    [0] Local(Mutable, __has_returned: Bool): Lit(Bool(false))
                    [1] Local(Mutable, __ret_val: Int): Lit(Int(0))
                    [2] Local(Mutable, i: Int): Lit(Int(0))
                    [3] Expr While[ty=Unit]
                    [4] Local(Immutable, __trailing_result: Int): UnOp(Neg)[ty=Int]
                    [5] Expr If(cond=Var[ty=Bool], then=Var[ty=Int], else=Var[ty=Int])
            callable Main: input_ty=Unit, output_ty=Int
                body: block_ty=Int
                    [0] Expr Call[ty=Int]"#]],
    );
}

#[test]
fn already_normalized_idempotency() {
    // Running on already-normalized code (no returns) produces no changes.
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    1
                } else {
                    2
                }
            }
        }
    "#};
    // Snapshot pins the stable output; any divergence fails the check.
    check_no_returns_q(
        source,
        &expect![[r#"
        // namespace Test
        function Main() : Int {
            body {
                if true {
                    1
                } else {
                    2
                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn return_value_is_complex_expression() {
    // `return f(x) + g(y);` style complex expression.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                if true {
                    return Add(1, 2) + Add(3, 4);
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    a + b
                }
            }
            function Main() : Int {
                body {
                    if true {
                        Add(1, 2) + Add(3, 4)
                    } else {
                        0
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_in_else_branch_only() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    1
                } else {
                    return 2;
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if not true {
                        2
                    } else {
                        1
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_bool_in_dynamic_branch() {
    // Quantum operation with dynamic branch using measurement.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Bool {
                use q = Qubit();
                if M(q) == One {
                    return true;
                }
                false
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Bool {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if M(q) == One {
                        {
                            let
                            @generated_ident_32 : Bool = true;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_32
                        }

                    } else {
                        let
                        @generated_ident_44 : Bool = false;
                        __quantum__rt__qubit_release(q);
                        @generated_ident_44
                    }

                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn multiple_returns_in_helper_function() {
    // Helper function called from entry point with multiple returns.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Classify(x : Int) : Int {
                if x > 0 {
                    return 1;
                }
                if x < 0 {
                    return -1;
                }
                0
            }
            function Main() : Int {
                Classify(5)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Classify(x : Int) : Int {
                body {
                    if x > 0 {
                        1
                    } else {
                        if x < 0 {
            -1
                        } else {
                            0
                        }

                    }

                }
            }
            function Main() : Int {
                body {
                    Classify(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_unit_after_side_effects() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Unit {
                use q = Qubit();
                H(q);
                if M(q) == One {
                    X(q);
                    return ();
                }
                Y(q);
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    H(q);
                    if M(q) == One {
                        X(q);
                        {
                            let
                            @generated_ident_42 : Unit = ();
                            __quantum__rt__qubit_release(q);
                            @generated_ident_42
                        }

                    } else {
                        Y(q);
                        __quantum__rt__qubit_release(q);
                    }

                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn bare_return_with_dead_code() {
    // `return x; dead_code;` — apply_bare_return must truncate statements
    // after the return.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                H(q);
                return 42;
                let x = 1;
                x + 2
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    H(q);
                    {
                        let
                        @generated_ident_33 : Int = 42;
                        __quantum__rt__qubit_release(q);
                        @generated_ident_33
                    }

                }
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_if_with_returns_at_different_levels() {
    // Returns at two levels of if nesting: the innermost if-return is lifted
    // first, then the outer if-return is lifted.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    if false {
                        return 1;
                    }
                    return 2;
                }
                3
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        if false {
                            {
                                __ret_val = 1;
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            {
                                __ret_val = 2;
                                __has_returned = true;
                            };
                        };
                    }

                    let __trailing_result : Int = 3;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_tuple_value() {
    // Return of a compound (tuple) type exercises type propagation
    // through strip_returns_from_expr.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                if true {
                    return (1, true);
                }
                (0, false)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : (Int, Bool) {
                body {
                    if true {
                        (1, true)
                    } else {
                        (0, false)
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guard_clause_with_existing_else_and_remaining() {
    // if-return with an existing else body AND remaining statements after
    // the if — exercises apply_if_then_return's else prepend path.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    let _ = 0;
                }
                2
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if true {
                        1
                    } else {
                        {
                            let _ : Int = 0;
                        };
                        2
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn deeply_nested_block_with_return() {
    // Return inside multiple levels of nested blocks exercises
    // NestedBlock recursion in classify_return_stmt.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = {
                    if true {
                        return 10;
                    }
                    5
                };
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let x : Int = {
                        if true {
                            {
                                __ret_val = 10;
                                __has_returned = true;
                            };
                        }

                        5
                    };
                    let __trailing_result : Int = x;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_after_dynamic_branch_with_dead_code() {
    // Dynamic branch followed by early return followed by dead code.
    // Exercises BareReturn truncation after a non-classical if-else.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Unit {
                use q = Qubit();
                if M(q) == One {
                    X(q);
                } else {
                    H(q);
                }
                H(q);
                return ();
                Y(q);
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if M(q) == One {
                        X(q);
                    } else {
                        H(q);
                    }

                    H(q);
                    {
                        let
                        @generated_ident_48 : Unit = ();
                        __quantum__rt__qubit_release(q);
                        @generated_ident_48
                    }

                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn for_loop_with_early_return() {
    // For loops desugar to a block wrapping locals + while in FIR.
    // The While is nested inside a Block expression, so transform_while_stmt
    // must descend through Block wrappers to find and transform it.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                for i in 0..10 {
                    if i == 5 {
                        return i;
                    }
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        let
                        @range_id_30 : Range = 0..10;
                        mutable
                        @index_id_33 : Int =
                        @range_id_30::Start;
                        let
                        @step_id_38 : Int =
                        @range_id_30::Step;
                        let
                        @end_id_43 : Int =
                        @range_id_30::End;
                        while not __has_returned and
                        @step_id_38 > 0 and
                        @index_id_33 <=
                        @end_id_43 or
                        @step_id_38 < 0 and
                        @index_id_33 >=
                        @end_id_43 {
                            let i : Int =
                            @index_id_33;
                            if i == 5 {
                                {
                                    __ret_val = i;
                                    __has_returned = true;
                                };
                            }

                            if not __has_returned {
                                @index_id_33 +=
                                @step_id_38;
                            };
                        }

                    }

                    let __trailing_result : Int = -1;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_qubit_scope_return_updates_outer_block_type() {
    check_structure(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Result {
                use outer = Qubit() {
                    use qubit = Qubit() {
                        let result = MResetZ(qubit);
                        Reset(outer);
                        return result;
                    }
                }
            }
        }
    "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Result
                body: block_ty=Result
                    [0] Expr Block[ty=Result]"#]],
    );
}

#[test]
fn early_return_in_qubit_array_scope_preserves_release_order() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use qs = Qubit[2];
                if flag {
                    return 1;
                }
                0
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let body_block_id = find_body_block_id(package, "Foo");
    let body_block = package.get_block(body_block_id);
    let has_path_local_array_release = body_block.stmts.iter().any(|&stmt_id| {
        stmt_tree_calls_named_callable(&store, package, stmt_id, "ReleaseQubitArray")
    });
    assert!(
        has_path_local_array_release,
        "Foo body should preserve ReleaseQubitArray on value-producing paths"
    );

    let has_unconditional_array_release_suffix = body_block
        .stmts
        .iter()
        .any(|&stmt_id| stmt_calls_named_callable(&store, package, stmt_id, "ReleaseQubitArray"));
    assert!(
        !has_unconditional_array_release_suffix,
        "Foo body should not keep an unconditional ReleaseQubitArray suffix after path-local releases"
    );
}

#[test]
fn classify_semi_return_and_expr_return_produce_same_shape() {
    let semi_source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                return 1;
            }
        }
    "#};
    let expr_source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                return 1
            }
        }
    "#};

    let (semi_store, semi_pkg_id) = compile_return_unified(semi_source);
    let (expr_store, expr_pkg_id) = compile_return_unified(expr_source);

    let semi_summary = summarize_callable(semi_store.get(semi_pkg_id), "Main");
    let expr_summary = summarize_callable(expr_store.get(expr_pkg_id), "Main");
    assert_eq!(
        semi_summary, expr_summary,
        "Semi-Return and Expr-Return callables must produce identical post-return_unify shapes",
    );
}

/// Flag-guarded stmt type check: `guard_stmt_with_flag` requires a
/// Unit-typed inner stmt. Passing a non-Unit `StmtKind::Expr` must trip
/// the debug assertion. Gated on debug builds because `debug_assert!` is
/// elided in release.
#[cfg(debug_assertions)]
#[test]
fn outer_return_wrapping_if_with_stmt_return_in_else_does_not_loop() {
    check_structure(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    1
                } else {
                    return M(q) == One ? 0 | 1;
                };
            }
        }
    "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Int
                body: block_ty=Int
                    [0] Local(Immutable, q: Qubit): Call[ty=Qubit]
                    [1] Expr Block[ty=Int]"#]],
    );
}

#[test]
fn outer_return_wrapping_if_with_stmt_return_in_else_full_pipeline() {
    // Verify the full pipeline (including PostAll invariant checks) succeeds
    // now that If expression types and Pat types are synchronized after
    // return replacement.
    let source = indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    1
                } else {
                    return M(q) == One ? 0 | 1;
                };
            }
        }
    "#};

    let _ = compile_and_run_pipeline_to(source, PipelineStage::Full);
}

#[test]
fn recursive_function_with_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Factorial(n : Int) : Int {
                if n <= 1 {
                    return 1;
                }
                n * Factorial(n - 1)
            }
            function Main() : Int {
                Factorial(5)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Factorial(n : Int) : Int {
                body {
                    if n <= 1 {
                        1
                    } else {
                        n * Factorial(n - 1)
                    }

                }
            }
            function Main() : Int {
                body {
                    Factorial(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn fail_and_return_in_same_control_flow() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let c = true;
                if c {
                    return 42;
                } else {
                    fail "unreachable";
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let c : Bool = true;
                    if c {
                        42
                    } else {
                        fail $"unreachable";
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

// Arrow-typed return in structured path

#[test]
fn arrow_typed_return_in_structured_path() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : (Int -> Int) {
                if flag {
                    return x -> x + 1;
                }
                x -> x * 2
            }
            function Main() : Int {
                let f = Choose(true);
                f(10)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Choose(flag : Bool) : (Int -> Int) {
                body {
                    if flag {
                        / * closure item = 3 captures = [] * / < lambda >
                    } else {
                        / * closure item = 4 captures = [] * / < lambda >
                    }

                }
            }
            function Main() : Int {
                body {
                    let f : (Int -> Int) = Choose(true);
                    f(10)
                }
            }
            function < lambda > (x : Int, ) : Int {
                body {
                    x + 1
                }
            }
            function < lambda > (x : Int, ) : Int {
                body {
                    x * 2
                }
            }
            // entry
            Main()
        "#]],
    );
}

// semantic test omitted: the program returns callable values which
// trigger a defunctionalization convergence failure in the full pipeline.
// The structural test above validates that return_unify handles this pattern.

// Qubit return + while — triggers the error path

#[test]
fn simple_if_expr_init_with_return_stays_structured() {
    // Simple return directly in an if-branch initializer — the structured
    // strategy handles this via strip_returns_from_expr without flags.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 10;
                }
                let x = if false { return 20; } else { 30 };
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    if true {
                        10
                    } else {
                        if false {
                            20
                        } else {
                            let x : Int = {
                                30
                            };
                            x
                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}
