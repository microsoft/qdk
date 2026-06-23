// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Flag-strategy tests: specializations, while-body returns, local-init retypes,
//! and flag-fallback edge cases.

use super::*;

#[test]
fn adjoint_spec_hoist_in_call_arg() {
    // Return in a Call argument inside an explicit `adjoint` specialization.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Inner(x : Int, q : Qubit) : Unit is Adj {
                body ... { X(q); }
                adjoint self;
            }
            operation Outer(n : Int, q : Qubit) : Unit is Adj {
                body ... { Inner(n, q); }
                adjoint ... {
                    Inner((return ()), q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Adjoint Outer(1, q);
            }
        }
    "#},
        &expect![[r#"
            operation Inner(x : Int, q : Qubit) : Unit is Adj {
                body ... {
                    X(q);
                }
                adjoint ... {
                    X(q);
                }
            }
            operation Outer(n : Int, q : Qubit) : Unit is Adj {
                body ... {
                    Inner(n, q);
                }
                adjoint ... {
                    let _ : ((Int, Qubit) => Unit is Adj) = Inner;
                    ()
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Adjoint Outer(1, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_spec_hoist_in_call_arg() {
    // Return in a Call argument inside an explicit `controlled` specialization.
    // Disposition: documented contract. Snapshot keeps current callable
    // signature text, while round-trip compilation confirms validity.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            operation Outer(n : Int, q : Qubit) : Unit is Ctl {
                body ... { H(q); }
                controlled (ctls, ...) {
                    Controlled H(ctls, (return ()));
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (c, q) = (Qubit(), Qubit());
                Controlled Outer([c], (1, q));
            }
        }
    "#},
        &expect![[r#"
            operation Outer(n : Int, q : Qubit) : Unit is Ctl {
                body ... {
                    H(q);
                }
                controlled (ctls, ...) {
                    let _ : ((Qubit[], Qubit) => Unit is Adj + Ctl) = Controlled H;
                    let _ : Qubit[] = ctls;
                    ()
                }
            }
            operation Main() : Unit {
                let _generated_ident_53 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_55 : Qubit = __quantum__rt__qubit_allocate();
                let (c : Qubit, q : Qubit) = (_generated_ident_53, _generated_ident_55);
                Controlled Outer([c], (1, q));
                __quantum__rt__qubit_release(_generated_ident_55);
                __quantum__rt__qubit_release(_generated_ident_53);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_adjoint_spec_hoist_in_call_arg() {
    // Return in a Call argument inside an explicit `controlled adjoint`
    // specialization.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            operation Outer(n : Int, q : Qubit) : Unit is Adj + Ctl {
                body ... { H(q); }
                adjoint ... { H(q); }
                controlled (ctls, ...) { Controlled H(ctls, q); }
                controlled adjoint (ctls, ...) {
                    Controlled H(ctls, (return ()));
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (c, q) = (Qubit(), Qubit());
                Controlled Adjoint Outer([c], (1, q));
            }
        }
    "#},
        &expect![[r#"
            operation Outer(n : Int, q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    let _ : ((Qubit[], Qubit) => Unit is Adj + Ctl) = Controlled H;
                    let _ : Qubit[] = ctls;
                    ()
                }
            }
            operation Main() : Unit {
                let _generated_ident_71 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_73 : Qubit = __quantum__rt__qubit_allocate();
                let (c : Qubit, q : Qubit) = (_generated_ident_71, _generated_ident_73);
                Controlled Adjoint Outer([c], (1, q));
                __quantum__rt__qubit_release(_generated_ident_73);
                __quantum__rt__qubit_release(_generated_ident_71);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_body_with_call_arg_return() {
    // While body containing a Call-argument Return. The outer transform
    // routes this through the flag-based path because the Return sits
    // inside a while body.
    // Disposition: documented contract. Snapshot keeps historical identifier
    // spellings, while round-trip compilation confirms generated Q# validity.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let _ = Add((return 42), 2);
                    i += 1;
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            function Add(a : Int, b : Int) : Int {
                a + b
            }
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                while not __has_returned and i < 3 {
                    let _ : ((Int, Int) -> Int) = Add;
                    let _ : Int = 0;
                    {
                        __ret_val = 42;
                        __has_returned = true;
                    };
                    if not __has_returned {
                        i += 1;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
            -1
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn local_init_retype_in_call_arg_fix() {
    // `let x = if c { return 1 } else { 0 }; Identity(x);` — after hoist +
    // if-else transform, the local `x` must hold an Int (the transformed
    // initializer's new type), not the diverging type from the pre-transform
    // Return.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Identity(x : Int) : Int { x }
            function Main() : Int {
                let c = true;
                let x = if c { return 1 } else { 0 };
                Identity(x)
            }
        }
    "#},
        &expect![[r#"
            function Identity(x : Int) : Int {
                x
            }
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let c : Bool = true;
                let x : Int = if c {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    }

                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        Identity(x)
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_block_middle_of_block_fix() {
    // `{ if c { return 1; } 2 }; let y = 3; y` — a nested Block expression
    // containing an if-return-then-value sits in the middle of the outer
    // block. Regression for middle-of-block nested-block rewrite must
    // produce a Block whose trailing expression preserves the outer block's
    // structural invariants.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let c = true;
                let _unused = {
                    if c { return 1; }
                    2
                };
                let y = 3;
                y
            }
        }
    "#},
        &expect![[r#"
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let c : Bool = true;
                let _unused : Int = {
                    if c {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    2
                };
                let y : Int = if not __has_returned {
                    3
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        y
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flag_fallback_handles_arrow_return() {
    // A callable-valued Return inside a while body forces the flag-based
    // fallback to synthesize a default of arrow type. `create_default_value`
    // handles this by synthesizing a fail-bodied callable item of matching
    // signature and using `Var(Res::Item(..))` as the `__ret_val` seed; the
    // fail-bodied callable is never actually invoked because `__has_returned`
    // guards every read of `__ret_val`.
    let source = indoc! {r#"
        namespace Test {
            function MakeAdder(n : Int) : (Int -> Int) {
                mutable i = 0;
                while i < 3 {
                    if i == n {
                        return (x -> x + 1);
                    }
                    i += 1;
                }
                x -> x
            }
            @EntryPoint()
            function Main() : Int {
                let f = MakeAdder(1);
                f(10)
            }
        }
    "#};
    let _ = compile_return_unified(source);
    check_no_returns_q(
        source,
        &expect![[r#"
            function MakeAdder(n : Int) : (Int -> Int) {
                mutable __has_returned : Bool = false;
                mutable __ret_val : (Int -> Int) = __return_unify_fail_5;
                mutable i : Int = 0;
                while not __has_returned and i < 3 {
                    if i == n {
                        {
                            __ret_val = / * closure item = 3 captures = [] * / _lambda_3;
                            __has_returned = true;
                        };
                    }

                    if not __has_returned {
                        i += 1;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        / * closure item = 4 captures = [] * / _lambda_4
                    } else {
                        __ret_val
                    }
                }

            }
            function Main() : Int {
                let f : (Int -> Int) = MakeAdder(1);
                f(10)
            }
            function _lambda_3(x : Int, ) : Int {
                x + 1
            }
            function _lambda_4(x : Int, ) : Int {
                x
            }
            function __return_unify_fail_5(_ : Int) : Int {
                fail $"callable init expr"
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flag_fallback_supports_post_return_range_local_initializer() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return i;
                    }
                    i += 1;
                }
                let r = 0..3;
                r.Start
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    assert!(
        rendered.contains("let r : Range = if not _.has_returned {"),
        "post-return range local initializers should be guarded by flag-lowering",
    );
    // The post-return `let r` initializer is guarded by flag-lowering and
    // `r` is read by the trailing `r.Start`, so the binding stays live
    // (the `dead_local` simplifier rule does not eliminate it). After the
    // `let_folding` rule fires, the `_.trailing_result` binding is inlined
    // into the trailing merge, preserving the bind-then-check pattern as
    // `if _.has_returned _.ret_val else { if not _.has_returned { <tail> } else _.ret_val }`
    // (the synthesized names carry the `.` sentinel in the verbatim Debug dump).
    assert!(
        rendered.contains(
            "if _.has_returned _.ret_val else {\n            if not _.has_returned {\n                r::Start\n            } else _.ret_val\n        }"
        ),
        "final trailing expression should preserve the bind-then-check pattern (now inlined into the trailing merge)\n{rendered}",
    );
}
