// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{mut_visit::MutVisitor, validate::Validator, visit::Visitor};

use crate::loop_normalize::LoopNormalize;

/// Compiles `file`, runs [`LoopNormalize`] once over the package, asserts the
/// package still validates and no rejection diagnostics were produced, then
/// snapshots the transformed package.
fn check(file: &str, expect: &Expect) {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    assert!(errors.is_empty(), "unexpected rejection errors: {errors:?}");
    Validator::default().visit_package(&unit.package);
    expect.assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}

/// Compiles `file`, runs [`LoopNormalize`] once, snapshots the rejection
/// diagnostics it produced, and returns the transformed package text.
fn check_errors(file: &str, expect: &Expect) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    // The package must remain structurally valid even on the rejection path.
    Validator::default().visit_package(&unit.package);
    expect.assert_debug_eq(&errors);
    crate::qsharp_gen::write_package_qsharp(&store, &unit.package)
}

/// Compiles `file`, runs [`LoopNormalize`] once, records the package, runs it a
/// second time, asserts the package is unchanged, confirming the pass is
/// idempotent, and returns the stable package text.
fn check_idempotent(file: &str) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);
    let after_first = crate::qsharp_gen::write_package_qsharp(&store, &unit.package);

    LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);
    let after_second = crate::qsharp_gen::write_package_qsharp(&store, &unit.package);

    assert_eq!(
        after_first, after_second,
        "second run of LoopNormalize changed the package"
    );
    after_second
}

/// Compiles `file`, runs [`LoopNormalize`] once, validates the result, and
/// returns the generated Q# text for targeted structural assertions.
fn normalize_to_string(file: &str) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    assert!(errors.is_empty(), "unexpected rejection errors: {errors:?}");
    Validator::default().visit_package(&unit.package);
    crate::qsharp_gen::write_package_qsharp(&store, &unit.package)
}

fn operand_temp_bind_count(package: &str) -> usize {
    package.matches("let _operand_tmp").count()
}

#[test]
fn logical_assign_if_rhs_is_reshaped() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable cond = false;
                mutable keepGoing = false;
                while cond {
                    keepGoing and= if cond { break } else { true };
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "reshaped assignment RHS should be lifted inside its conditional branch\n{package}"
    );

    expect![[r#"
        operation Main() : Unit {
            mutable cond = false;
            mutable keepGoing = false;
            while cond {
                if keepGoing {
                    let _operand_tmp_36 = if cond {
                        break
                    } else {
                        true
                    };
                    keepGoing = _operand_tmp_36;
                };
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn update_field_evaluates_replacement_before_record_when_hoisting() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            newtype Pair = (A : Int, B : Int);

            operation Main() : Unit {
                mutable cond = true;
                mutable marker = 0;
                while cond {
                    let updated = { marker += 1; Pair(1, 2) } w/ B <- if cond { break } else { 3 };
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "only the replacement operand should be hoisted before a record update\n{package}"
    );

    expect![[r#"
        // newtype Pair
        operation Main() : Unit {
            mutable cond = true;
            mutable marker = 0;
            while cond {
                let _operand_tmp_45 = if cond {
                    break
                } else {
                    3
                };
                let updated = {
                    marker += 1;
                    Pair(1, 2)
                } w/::B <- _operand_tmp_45;
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn update_index_evaluates_index_and_replacement_before_container_when_hoisting() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable cond = true;
                mutable marker = 0;
                while cond {
                    let updated = { marker += 1; [1, 2] } w/ if cond { break } else { 0 } <- 3;
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "only the index operand should be hoisted before an array update\n{package}"
    );

    expect![[r#"
        operation Main() : Unit {
            mutable cond = true;
            mutable marker = 0;
            while cond {
                let _operand_tmp_43 = if cond {
                    break
                } else {
                    0
                };
                let updated = {
                    marker += 1;
                    [1, 2]
                } w/ _operand_tmp_43 <- 3;
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn hoist_break_in_call_argument() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = Foo;
                    let _operand_tmp_37 = if cond {
                        break
                    } else {
                        3
                    };
                    _operand_tmp_33(_operand_tmp_37);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_call_argument() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { continue } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = Foo;
                    let _operand_tmp_37 = if cond {
                        continue
                    } else {
                        3
                    };
                    _operand_tmp_33(_operand_tmp_37);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_binop_operand() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    mutable acc = 0;
                    while cond {
                        let y = acc + (if cond { break } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                mutable acc = 0;
                while cond {
                    let _operand_tmp_33 = acc;
                    let _operand_tmp_37 = if cond {
                        break
                    } else {
                        3
                    };
                    let y = _operand_tmp_33 + _operand_tmp_37;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_operand_block() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo({ if cond { break }; 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_34 = Foo;
                    let _operand_tmp_38 = {
                        if cond {
                            break
                        };
                        3
                    };
                    _operand_tmp_34(_operand_tmp_38);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_nested_operand_blocks() {
    check(
        indoc! {"
            namespace Test {
                function Bar(x : Int) : Int { x }
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(Bar(if cond { break } else { 3 }));
                    }
                }
            }
        "},
        &expect![[r#"
            function Bar(x : Int) : Int {
                x
            }
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_43 = Bar;
                    let _operand_tmp_47 = if cond {
                        break
                    } else {
                        3
                    };
                    Foo(_operand_tmp_43(_operand_tmp_47));
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_tuple_operand() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : (Int, Int)) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo((1, if cond { break } else { 3 }));
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : (Int, Int)) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_35 = 1;
                    let _operand_tmp_39 = if cond {
                        break
                    } else {
                        3
                    };
                    Foo(_operand_tmp_35, _operand_tmp_39);
                }
            }
        "#]],
    );
}

#[test]
fn idempotent_after_hoisting_break() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(if cond { break } else { 3 });
                }
            }
        }
    "});

    expect![[r#"
        operation Foo(x : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_33 = Foo;
                let _operand_tmp_37 = if cond {
                    break
                } else {
                    3
                };
                _operand_tmp_33(_operand_tmp_37);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn idempotent_after_hoisting_nested_operands() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            function Bar(x : Int) : Int { x }
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(Bar(if cond { break } else { 3 }));
                }
            }
        }
    "});

    expect![[r#"
        function Bar(x : Int) : Int {
            x
        }
        operation Foo(x : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_43 = Bar;
                let _operand_tmp_47 = if cond {
                    break
                } else {
                    3
                };
                Foo(_operand_tmp_43(_operand_tmp_47));
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn preserves_type_of_surface_if() {
    // The `if` is the direct initializer of the `let`, a statement position rather
    // than an operand, so it is left in place and `x` keeps its `Int` type.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let x = if cond { break } else { 3 };
                        let y = x + 1;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let x = if cond {
                        break
                    } else {
                        3
                    };
                    let y = x + 1;
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_statement_position_break() {
    // A break that is already a statement, inside a statement-position `if`, is
    // not in operand position, so nothing is hoisted.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        if cond { break }
                        Foo(3);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    if cond {
                        break
                    }
                    Foo(3);
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_operand_block_without_control_flow() {
    // An operand-position block that contains no escaping control flow is left
    // untouched.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo({ let z = 1; z });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo({
                        let z = 1;
                        z
                    });
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_break_bound_to_nested_loop() {
    // The break binds to the inner `while`, so it does not escape the operand
    // block and no hoist is performed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { while cond { break }; 3 } else { 4 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(if cond {
                        while cond {
                            break
                        };
                        3
                    } else {
                        4
                    });
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_tuple_qubit_initializer_preserves_evaluation_order() {
    check(
        indoc! {"
            namespace Test {
                operation Length(value : Int) : Int { value }
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        use (first, second) = (
                            Qubit[Length(1)],
                            Qubit[if cond { break } else { Length(2) }]
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Length(value : Int) : Int {
                value
            }
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_46 = Length(1);
                    let _operand_tmp_50 = if cond {
                        break
                    } else {
                        Length(2)
                    };
                    use (first, second) = (Qubit[_operand_tmp_46], Qubit[_operand_tmp_50]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_qubit_operand_block_array_backed() {
    // Lifting the operand block introduces a temporary of type `Qubit`, which
    // has no classical default for the break path. Rather than reject it, the
    // pass array-backs the temp as `Qubit[]`: the block's trailing value `q` is
    // wrapped as `[q]`, and the operand slot reads it back through
    // `.operand_tmp_<id>[0]`. The later desugar seeds the break path with the
    // universal `[]` default and guards the read, so `[]` is never indexed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(q : Qubit) : Unit {}
                operation Main() : Unit {
                    use q = Qubit();
                    mutable cond = true;
                    while cond {
                        Foo({ if cond { break }; q });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    let _operand_tmp_38 = Foo;
                    let _operand_tmp_42 = {
                        if cond {
                            break
                        };
                        [q]
                    };
                    _operand_tmp_38(_operand_tmp_42[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_arrow_operand_block_array_backed() {
    // An arrow-typed operand value-block has no classical default, so it is
    // array-backed as `(Qubit => Unit)[]`, which lets the desugar accept it
    // uniformly with the other array-backed operand types.
    check(
        indoc! {"
            namespace Test {
                operation Bar(q : Qubit) : Unit {}
                operation Foo(op : Qubit => Unit) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Bar });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Bar(q : Qubit) : Unit {}
            operation Foo(op : (Qubit => Unit)) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_39 = Foo;
                    let _operand_tmp_43 = if cond {
                        break
                    } else {
                        [Bar]
                    };
                    _operand_tmp_39(_operand_tmp_43[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_udt_operand_block_array_backed() {
    // A user-defined-type operand value-block is array-backed as `Pair[]`,
    // uniformly with `Qubit` and arrow types and without constructing a `Pair`
    // default, so the normalize pass and the desugar handle it consistently.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Foo(p : Pair) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Pair(1, 2) });
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Foo(p : Pair) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_38 = Foo;
                    let _operand_tmp_42 = if cond {
                        break
                    } else {
                        [Pair(1, 2)]
                    };
                    _operand_tmp_38(_operand_tmp_42[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_tuple_with_qubit_operand_array_backed() {
    // A tuple containing a `Qubit` is non-defaultable but representable, so the
    // whole operand is array-backed as `(Int, Qubit)[]`; the trailing tuple
    // value `(1, q)` is wrapped as `[(1, q)]` without decomposing the tuple.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : (Int, Qubit)) : Unit {}
                operation Main() : Unit {
                    use q = Qubit();
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { (1, q) });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : (Int, Qubit)) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    let _operand_tmp_39 = Foo;
                    let _operand_tmp_43 = if cond {
                        break
                    } else {
                        [(1, q)]
                    };
                    _operand_tmp_39(_operand_tmp_43[0]::Item < 0 >, _operand_tmp_43[0]::Item < 1 >);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_controlled_call_preserves_control_tuple() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(q : Qubit) : Unit is Ctl {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Controlled Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit is Ctl {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_25 = Controlled Foo;
                    let _operand_tmp_29 = break;
                    _operand_tmp_25(_operand_tmp_29[0]::Item < 0 >, _operand_tmp_29[0]::Item < 1 >);
                }
            }
        "#]],
    );
}

#[test]
fn idempotent_after_array_backing_qubit_operand() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    Foo({ if cond { break }; q });
                }
            }
        }
    "});

    expect![[r#"
        operation Foo(q : Qubit) : Unit {}
        operation Main() : Unit {
            use q = Qubit();
            mutable cond = true;
            while cond {
                let _operand_tmp_38 = Foo;
                let _operand_tmp_42 = {
                    if cond {
                        break
                    };
                    [q]
                };
                _operand_tmp_38(_operand_tmp_42[0]);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn reject_break_in_unrepresentable_operand_block() {
    // A type-parameter-typed operand value-block is conservatively excluded from
    // array-backing. This matches the `return_unify` transform, which treats
    // unresolved leaves such as type parameters as the sole rejecting case, so
    // the pass records its defensive rejection. Such an operand cannot occur for
    // a well-typed program post-typecheck once callables are monomorphized.
    let package = check_errors(
        indoc! {"
            namespace Test {
                operation Foo<'T>(x : 'T, g : 'T => Unit) : Unit {
                    mutable cond = true;
                    while cond {
                        g(if cond { break } else { x });
                    }
                }
            }
        "},
        &expect![[r#"
            [
                UnsupportedType(
                    "Param<\"'T\": 0>",
                    Span {
                        lo: 136,
                        hi: 164,
                    },
                ),
            ]
        "#]],
    );

    expect![[r#"
        operation Foo(x : 'T, g : ('T => Unit)) : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_31 = g;
                let _operand_tmp_35 = if cond {
                    break
                } else {
                    x
                };
                _operand_tmp_31(_operand_tmp_35);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn hoist_bare_break_in_call_argument() {
    // A bare `break` sitting directly in a call-argument slot is itself the
    // escaping control flow, so it is lifted to its own spine temp; the later
    // desugar guards the call behind the break flag.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = Foo;
                    let _operand_tmp_28 = break;
                    _operand_tmp_24(_operand_tmp_28);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_continue_in_call_argument() {
    // A bare `continue` operand is lifted identically to a bare `break`; the
    // two are handled uniformly by the operand lift.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = Foo;
                    let _operand_tmp_28 = continue;
                    _operand_tmp_24(_operand_tmp_28);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_assign_value() {
    // The value operand of an assignment is lifted, so a bare `break` there is
    // exposed at statement position before the assignment.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable x = 0;
                    mutable cond = true;
                    while cond {
                        x = break;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable x = 0;
                mutable cond = true;
                while cond {
                    let _operand_tmp_22 = break;
                    x = _operand_tmp_22;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_index_operand() {
    // The index operand of an array access is lifted, so a bare `break` used as
    // an index is exposed at statement position and the access is later guarded.
    // The access is consumed by a call so its divergent result type is fixed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    let arr = [1, 2, 3];
                    mutable cond = true;
                    while cond {
                        Foo(arr[break]);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                let arr = [1, 2, 3];
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = arr;
                    let _operand_tmp_37 = break;
                    Foo(_operand_tmp_33[_operand_tmp_37]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_if_condition() {
    // An `if` condition is an unconditional operand site, so a bare `break` in
    // the condition is lifted to a spine temp ahead of the `if`.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let y = if break { 1 } else { 2 };
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_27 = break;
                    let y = if _operand_tmp_27 {
                        1
                    } else {
                        2
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_short_circuit_lhs() {
    // The left operand of a short-circuit `or` evaluates unconditionally, so a
    // bare `break` there is lifted to a spine temp.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = break or y;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = break;
                    let z = _operand_tmp_24 or y;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_short_circuit_or_rhs() {
    // The right operand of `or` is conditional, so when it buries escaping
    // control flow the `BinOp` is reshaped into `if y { true } else { <rhs> }`
    // and the buried `break` reaches a statement boundary inside the else block.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = y or Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let z = if y {
                        true
                    } else {
                        let _operand_tmp_41 = Foo;
                        let _operand_tmp_45 = break;
                        _operand_tmp_41(_operand_tmp_45)
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_short_circuit_and_rhs() {
    // The right operand of `and` is conditional, so when it buries escaping
    // control flow the `BinOp` is reshaped into `if y { <rhs> } else { false }`
    // and the buried `continue` reaches a statement boundary inside the then
    // block. Mirrors the `or` reshape with the branches swapped.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = y and Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let z = if y {
                        let _operand_tmp_41 = Foo;
                        let _operand_tmp_45 = continue;
                        _operand_tmp_41(_operand_tmp_45)
                    } else {
                        false
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_compound_short_circuit_and_assign_rhs() {
    // A compound `and=` whose right operand buries a `break` in a bare operand
    // position (`Foo(break)`, not a statement-carrying wrapper) is reshaped into
    // `if b { b = Foo(break) }`, so the buried `break` reaches a statement
    // boundary inside the guarded assignment block instead of running `Foo` with
    // a default on the divergence path. The guard preserves the short-circuit:
    // the assignment runs only when `b` is already true.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable b = true;
                    mutable cond = true;
                    while cond {
                        b and= Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable b = true;
                mutable cond = true;
                while cond {
                    if b {
                        let _operand_tmp_37 = Foo;
                        let _operand_tmp_41 = break;
                        b = _operand_tmp_37(_operand_tmp_41);
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_compound_short_circuit_or_assign_rhs() {
    // A compound `or=` whose right operand buries a `continue` in a bare operand
    // position is reshaped into `if not b { b = Foo(continue) }`: the `or`
    // short-circuits when `b` is already true, so the assignment, and the buried
    // `continue`, runs only when `b` is false.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable b = false;
                    mutable cond = true;
                    while cond {
                        b or= Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable b = false;
                mutable cond = true;
                while cond {
                    if (not b) {
                        let _operand_tmp_38 = Foo;
                        let _operand_tmp_42 = continue;
                        b = _operand_tmp_38(_operand_tmp_42);
                    };
                }
            }
        "#]],
    );
}

#[test]
fn array_back_non_defaultable_let_rhs_break() {
    // A `let` binding whose value buries a `break` and whose type has no
    // classical default, such as `Pair`, is array-backed: the initializer becomes a
    // `Pair[]` temp whose `then` branch is a bare break and whose `else` branch is
    // the singleton `[Pair(1, 2)]`, and the binding reads it back through
    // `.operand_tmp_<id>[0]`, so no `Pair` default is needed on the divergence path.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Foo(p : Pair) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let x = if cond { break } else { Pair(1, 2) };
                        Foo(x);
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Foo(p : Pair) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_42 = if cond {
                        break
                    } else {
                        [Pair(1, 2)]
                    };
                    let x = _operand_tmp_42[0];
                    Foo(x);
                }
            }
        "#]],
    );
}

#[test]
fn array_back_discarded_value_block_break() {
    // A non-Unit block used as a statement with its result discarded, written
    // `{ … };`, whose value buries a `break` and has no classical default such as
    // `Pair`, is array-backed in place: the block's value type becomes `Pair[]`,
    // with its `then` branch a bare break and its `else` branch the singleton
    // `[Pair(1, 2)]`, so the buried break desugars with the universal `[]` default
    // instead of a `Pair` default. The value stays discarded; no temp binding is
    // introduced.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        { if cond { break } else { Pair(1, 2) } };
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    {
                        if cond {
                            break
                        } else {
                            [Pair(1, 2)]
                        }
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_return_operand_block() {
    // A `return` operand may bury an escaping `break`; the operand is lifted to
    // a temp while the `return` node stays in place, so the buried `break` is
    // exposed without hoisting the `return` itself.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Int {
                    mutable cond = true;
                    while cond {
                        return { if cond { break }; 5 };
                    }
                    0
                }
            }
        "},
        &expect![[r#"
            operation Main() : Int {
                mutable cond = true;
                while cond {
                    let _operand_tmp_29 = {
                        if cond {
                            break
                        };
                        5
                    };
                    return _operand_tmp_29;
                }
                0
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_for_iterable_nested_in_outer_loop() {
    // A `for` iterable is evaluated once in the enclosing loop scope. A `break`
    // buried in a compound iterable binds to the outer `while`, so the iterable
    // is lifted to a spine temp ahead of the `for` and the buried `break` is
    // exposed without hoisting the `for` itself.
    check(
        indoc! {"
            namespace Test {
                function F(a : Int[], b : Int) : Int[] { a }
                operation G() : Int { 0 }
                operation Main() : Unit {
                    let arr = [1, 2, 3];
                    mutable cond = true;
                    while cond {
                        for j in F({ if cond { break }; arr }, G()) {
                            let k = j;
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            function F(a : Int[], b : Int) : Int[] {
                a
            }
            operation G() : Int {
                0
            }
            operation Main() : Unit {
                let arr = [1, 2, 3];
                mutable cond = true;
                while cond {
                    let _operand_tmp_65 = {
                        if cond {
                            break
                        };
                        arr
                    };
                    for j in F(_operand_tmp_65, G()) {
                        let k = j;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_core_udt_operand_block_array_backed() {
    // A user-defined type defined in another package (`Complex`, from the core
    // library) is array-backed just like a local newtype: array-backing needs
    // only the universal `[]` default of `Complex[]`, never a default of the
    // user-defined type itself, so the operand is representable regardless of
    // which package defines it. This is the cross-package companion to
    // `hoist_break_in_udt_operand_block_array_backed`.
    check(
        indoc! {"
            namespace Test {
                operation Foo(c : Complex) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Complex(1.0, 2.0) });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(c : Complex) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_37 = Foo;
                    let _operand_tmp_41 = if cond {
                        break
                    } else {
                        [Complex(1., 2.)]
                    };
                    _operand_tmp_37(_operand_tmp_41[0]);
                }
            }
        "#]],
    );
}
