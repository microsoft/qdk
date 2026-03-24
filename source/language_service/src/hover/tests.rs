// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::get_hover;
use crate::test_utils::{compile_notebook_with_markers, compile_with_markers};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc::line_column::Encoding;

/// Asserts that the hover text at the given cursor position matches the expected hover text.
/// The cursor position is indicated by a `↘` marker in the source text.
/// The expected hover span is indicated by two `◉` markers in the source text.
fn check(source_with_markers: &str, expect: &Expect) {
    let (compilation, cursor_position, target_spans) =
        compile_with_markers(source_with_markers, true);
    let actual = get_hover(&compilation, "<source>", cursor_position, Encoding::Utf8)
        .expect("Expected a hover.");
    assert_eq!(&actual.span, &target_spans[0]);
    expect.assert_eq(&actual.contents);
}

/// Asserts that there is no hover for the given test case.
fn check_none(source_with_markers: &str) {
    let (compilation, cursor_position, _) = compile_with_markers(source_with_markers, true);
    let actual = get_hover(&compilation, "<source>", cursor_position, Encoding::Utf8);
    assert!(actual.is_none());
}

fn check_notebook(cells_with_markers: &[(&str, &str)], expect: &Expect) {
    let (compilation, cell_uri, position, target_spans) =
        compile_notebook_with_markers(cells_with_markers);

    let actual =
        get_hover(&compilation, &cell_uri, position, Encoding::Utf8).expect("Expected a hover.");
    assert_eq!(&actual.span, &target_spans[0].range);
    expect.assert_eq(&actual.contents);
}

fn check_notebook_none(cells_with_markers: &[(&str, &str)]) {
    let (compilation, cell_uri, position, _) = compile_notebook_with_markers(cells_with_markers);

    let actual = get_hover(&compilation, &cell_uri, position, Encoding::Utf8);
    assert!(actual.is_none());
}

#[test]
fn attr() {
    check(
        indoc! {r#"
        namespace Test {
            @◉Entr↘yPoint◉()
            operation Bar() : Unit {}
        }
    "#},
        &expect![[r#"
            attribute ```EntryPoint```

            Indicates that the callable is the entry point to a program."#]],
    );
}

#[test]
fn attr_with_arg() {
    check(
        indoc! {r#"
        namespace Test {
            @◉Con↘fig◉(BackwardsBranching)
            operation Bar() : Unit {}
        }
    "#},
        &expect![[r#"
            attribute ```Config```

            Provides pre-processing information about when an item should be included in compilation.

            Valid arguments are `Base`, `Adaptive`, `IntegerComputations`, `FloatingPointComputations`, `BackwardsBranching`, `HigherLevelConstructs`, `QubitReset`, and `Unrestricted`.

            The `not` operator is also supported to negate the attribute, e.g. `not Adaptive`."#]],
    );
}

#[test]
fn callable_unit_types() {
    check(
        indoc! {r#"
        namespace Test {
            /// Doc comment
            /// with multiple lines!
            operation ◉B↘ar◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Bar() : Unit
            ```
            ---
            Doc comment
            with multiple lines!
        "#]],
    );
}

#[test]
fn callable_with_callable_types() {
    check(
        indoc! {r#"
        namespace Test {
            /// Doc comment!
            operation ◉F↘oo◉(x : (Int => Int)) : (Int => Int) {x}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo(x : (Int => Int)) : (Int => Int)
            ```
            ---
            Doc comment!
        "#]],
    );
}

#[test]
fn callable_with_type_params() {
    check(
        indoc! {r#"
        namespace Test {
            /// Doc comment!
            operation ◉F↘oo◉<'A, 'B>(a : 'A, b : 'B) : 'B { b }
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo<'A, 'B>(a : 'A, b : 'B) : 'B
            ```
            ---
            Doc comment!
        "#]],
    );
}

#[test]
fn callable_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit { ◉B↘ar◉(); }

            operation Bar() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Bar() : Unit
            ```
        "#]],
    );
}

#[test]
fn callable_with_type_params_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let temp = ◉B↘ar◉(1, 2.0);
            }

            operation Bar<'A, 'B>(a : 'A, b : 'B) : 'B { b }
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Bar<'A, 'B>(a : 'A, b : 'B) : 'B
            ```
        "#]],
    );
}

#[test]
fn callable_unit_types_functors() {
    check(
        indoc! {r#"
        namespace Test {
            /// Doc comment!
            operation ◉F↘oo◉() : Unit is Ctl {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit is Ctl
            ```
            ---
            Doc comment!
        "#]],
    );
}

#[test]
fn callable_with_callable_types_functors() {
    check(
        indoc! {r#"
        namespace Test {
            /// Doc comment!
            operation ◉F↘oo◉(x : (Int => Int is Ctl + Adj)) : (Int => Int is Adj) is Adj {x}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo(x : (Int => Int is Adj + Ctl)) : (Int => Int is Adj) is Adj
            ```
            ---
            Doc comment!
        "#]],
    );
}

#[test]
fn callable_ref_functors() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit { ◉B↘ar◉(); }

            operation Bar() : Unit is Adj {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Bar() : Unit is Adj
            ```
        "#]],
    );
}

#[test]
fn callable_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(◉↘x◉ : Int) : Unit { let y = x; }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn callable_param_with_type_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'A>(◉↘x◉ : 'A) : Unit { let y = x; }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            x : 'A
            ```
        "#]],
    );
}

#[test]
fn callable_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int) : Unit { let y = ◉↘x◉; }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn callable_param_with_type_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'A>(x : 'A) : Unit { let y = ◉↘x◉; }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            x : 'A
            ```
        "#]],
    );
}

#[test]
fn callable_spec_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x: Int): Unit is Ctl {
                body ... { let y = x; }
                controlled (◉↘ctrl◉, ...) { let z = ctrl; }
            }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            ctrl : Qubit[]
            ```
        "#]],
    );
}

#[test]
fn callable_spec_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x: Int): Unit is Ctl {
                body ... { let y = x; }
                controlled (ctrl, ...) { let z = ◉↘ctrl◉; }
            }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            ctrl : Qubit[]
            ```
        "#]],
    );
}

#[test]
fn identifier() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let ◉↘x◉ = 3;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_with_type_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'A>(a : 'A) : Unit {
                let ◉↘x◉ = a;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            x : 'A
            ```
        "#]],
    );
}

#[test]
fn identifier_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let x = 3;
                let y = ◉↘x◉;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_with_type_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'A>(a : 'A) : Unit {
                let x = a;
                let y = ◉↘x◉;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            x : 'A
            ```
        "#]],
    );
}

#[test]
fn identifier_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let (x, ◉↘y◉) = (3, 1.4);
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            y : Double
            ```
        "#]],
    );
}

#[test]
fn identifier_tuple_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let (x, y) = (3, 1.4);
                let z = ◉↘y◉;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            y : Double
            ```
        "#]],
    );
}

#[test]
fn identifier_for_loop() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                for ◉↘i◉ in 0..10 {
                    let y = i;
                }
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            i : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_for_loop_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                for i in 0..10 {
                    let y = ◉↘i◉;
                }
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            i : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_nested_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let x = 3;
                if true {
                    let y = ◉↘x◉;
                }
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn lambda() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let a = 3;
                let ◉la↘mbda◉ = (x, y) => a;
                let b = lambda(1.2, "yes");
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            lambda : ((Double, String) => Int)
            ```
        "#]],
    );
}

#[test]
fn lambda_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let a = 3;
                let lambda = (x, y) => a;
                let b = ◉la↘mbda◉(1.2, "yes");
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            lambda : ((Double, String) => Int)
            ```
        "#]],
    );
}

#[test]
fn lambda_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let a = 3;
                let lambda = (x, ◉↘y◉) => a;
                let b = lambda(1.2, "yes");
            }
        }
    "#},
        &expect![[r#"
            lambda parameter
            ```qsharp
            y : String
            ```
        "#]],
    );
}

#[test]
fn lambda_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let lambda = (x, y) => ◉↘y◉;
                let a = lambda(1.2, "yes");
            }
        }
    "#},
        &expect![[r#"
            lambda parameter
            ```qsharp
            y : String
            ```
        "#]],
    );
}

#[test]
fn lambda_closure_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let a = 3;
                let lambda = (x, y) => ◉↘a◉;
                let b = lambda(1.2, "yes");
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            a : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_udt() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (fst : Int, snd : Int);
            operation Foo() : Unit {
                let a = Pair(3, 4);
                let b = ◉↘a◉;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            a : Pair
            ```
        "#]],
    );
}

#[test]
fn udt() {
    check(
        indoc! {r#"
        namespace Test {
            newtype ◉P↘air◉ = (Int, snd : Int);
        }
    "#},
        &expect![[r#"
            user-defined type of `Test`
            ```qsharp
            newtype Pair = (Int, snd : Int)
            ```
        "#]],
    );
}

#[test]
fn udt_ref() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Bar = (fst: Int, (snd : Int, Double, fourth: String), Double, sixth: Int);
            operation Foo() : ◉B↘ar◉ {
                Bar(3, (4, 2.1, "Yes"), 4.7, 2)
            }
        }
    "#},
        &expect![[r#"
            user-defined type of `Test`
            ```qsharp
            newtype Bar = (fst : Int, (snd : Int, Double, fourth : String), Double, sixth : Int)
            ```
        "#]],
    );
}

#[test]
fn udt_ref_nested_udt() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (fst: Int, snd: Int);
            newtype Bar = (fst: Int, (snd : Int, Double, fourth: Pair), Double, sixth: Int);
            operation Foo() : ◉B↘ar◉ {
                Bar(3, (4, 2.1, Pair(14, 15)), 4.7, 2)
            }
        }
    "#},
        &expect![[r#"
            user-defined type of `Test`
            ```qsharp
            newtype Bar = (fst : Int, (snd : Int, Double, fourth : Pair), Double, sixth : Int)
            ```
        "#]],
    );
}

#[test]
fn udt_anno_ref() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (Int, snd : Int);
            operation Foo() : Unit {
                let a : ◉P↘air◉ = Pair(3, 4);
            }
        }
    "#},
        &expect![[r#"
            user-defined type of `Test`
            ```qsharp
            newtype Pair = (Int, snd : Int)
            ```
        "#]],
    );
}

#[test]
fn udt_constructor() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (Int, snd : Int);
            operation Foo() : Unit {
                let a = ◉P↘air◉(3, 4);
            }
        }
    "#},
        &expect![[r#"
            user-defined type of `Test`
            ```qsharp
            newtype Pair = (Int, snd : Int)
            ```
        "#]],
    );
}

#[test]
fn udt_field() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (Int, ◉s↘nd◉ : Int);
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn udt_field_ref() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Pair = (Int, snd : Int);
            operation Foo() : Unit {
                let a = Pair(3, 4);
                let b = a::◉s↘nd◉;
            }
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn identifier_struct() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a = new Pair { fst = 3, snd = 4 };
                let b = ◉↘a◉;
            }
        }
    "#},
        &expect![[r#"
            local
            ```qsharp
            a : Pair
            ```
        "#]],
    );
}

#[test]
fn struct_def() {
    check(
        indoc! {r#"
        namespace Test {
            struct ◉P↘air◉ { fst : Int, snd : Int }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Pair { fst : Int, snd : Int }
            ```
        "#]],
    );
}

#[test]
fn struct_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : ◉Pa↘ir◉ {
                new Pair { fst = 3, snd = 4 }
            }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Pair { fst : Int, snd : Int }
            ```
        "#]],
    );
}

#[test]
fn struct_ref_nested_struct() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            struct Bar { fst: Int, snd : Pair }
            operation Foo() : ◉B↘ar◉ {
                new Bar { fst = 1, snd = new Pair { fst = 2, snd = 3 } }
            }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Bar { fst : Int, snd : Pair }
            ```
        "#]],
    );
}

#[test]
fn struct_anno_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a : ◉P↘air◉ = new Pair { fst = 3, snd = 4 };
            }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Pair { fst : Int, snd : Int }
            ```
        "#]],
    );
}

#[test]
fn struct_constructor() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a = new ◉P↘air◉ { fst = 3, snd = 4 };
            }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Pair { fst : Int, snd : Int }
            ```
        "#]],
    );
}

#[test]
fn struct_fn_constructor() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a = ◉P↘air◉(3, 4);
            }
        }
    "#},
        &expect![[r#"
            struct of `Test`
            ```qsharp
            struct Pair { fst : Int, snd : Int }
            ```
        "#]],
    );
}

#[test]
fn struct_field() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, ◉s↘nd◉ : Int }
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn struct_field_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a = new Pair { fst = 3, snd = 4 };
                let b = a::◉s↘nd◉;
            }
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn struct_field_ref_with_doc() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair {
                fst : Int,
                /// This is the second field!
                snd : Int
            }
            operation Foo() : Unit {
                let a = new Pair { fst = 3, snd = 4 };
                let b = a::◉s↘nd◉;
            }
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            /// This is the second field!
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn struct_field_cons_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct Pair { fst : Int, snd : Int }
            operation Foo() : Unit {
                let a = new Pair { fst = 3, ◉s↘nd◉ = 4 };
            }
        }
    "#},
        &expect![[r#"
            field of `Pair`
            ```qsharp
            snd : Int
            ```
        "#]],
    );
}

#[test]
fn struct_field_path_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct A { b : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = a.b.◉↘c◉.i;
            }
        }
    "#},
        &expect![[r#"
            field of `B`
            ```qsharp
            c : C
            ```
        "#]],
    );
}

#[test]
fn struct_field_path_first_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct A { b : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = ◉↘a◉.b.c.i;
            }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            a : A
            ```
        "#]],
    );
}

#[test]
fn struct_field_path_with_expr_ref() {
    check(
        indoc! {r#"
        namespace Test {
            struct A { b : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = { a.◉↘b◉ }.c.i;
            }
        }
    "#},
        &expect![[r#"
            field of `A`
            ```qsharp
            b : B
            ```
        "#]],
    );
}

#[test]
fn primitive_type() {
    check_none(indoc! {r#"
        namespace Test {
            newtype Pair = (◉I↘nt◉, snd : Int);
            operation Foo() : Unit {
                let a = Pair(3, 4);
                let b = a::snd;
            }
        }
    "#});
}

#[test]
fn foreign_call() {
    check(
        indoc! {r#"
        namespace Test {
            open FakeStdLib;
            operation Foo() : Unit {
                ◉F↘ake◉();
            }
        }
    "#},
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            operation Fake() : Unit
            ```
        "#]],
    );
}

#[test]
fn foreign_call_functors() {
    check(
        indoc! {r#"
        namespace Test {
            open FakeStdLib;
            operation Foo() : Unit {
                ◉F↘akeCtlAdj◉();
            }
        }
    "#},
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            operation FakeCtlAdj() : Unit is Adj + Ctl
            ```
        "#]],
    );
}

#[test]
fn foreign_call_with_param() {
    check(
        indoc! {r#"
        namespace Test {
            open FakeStdLib;
            operation Foo() : Unit {
                ◉FakeWi↘thParam◉(4);
            }
        }
    "#},
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            operation FakeWithParam(x : Int) : Unit
            ```
        "#]],
    );
}

#[test]
fn callable_summary() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Summary
            /// This is a
            /// multi-line summary!
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_summary_stuff_before() {
    check(
        indoc! {r#"
        namespace Test {
            /// not the summary
            /// # Summary
            /// This is a
            /// multi-line summary!
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_summary_other_header_before() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// This stuff is not the summary.
            /// # Summary
            /// This is a
            /// multi-line summary!
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_summary_other_header_after() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Summary
            /// This is a
            /// multi-line summary!
            /// # Not The Summary
            /// This stuff is not the summary.
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_summary_other_headers() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// This stuff is not the summary.
            /// # Summary
            /// This is a
            /// multi-line summary!
            /// # Also Not The Summary
            /// This stuff is also not the summary.
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_headers_but_no_summary() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// This stuff is not the summary.
            /// # Also Not The Summary
            /// This stuff is also not the summary.
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            # Not The Summary
            This stuff is not the summary.
            # Also Not The Summary
            This stuff is also not the summary.
        "#]],
    );
}

#[test]
fn callable_summary_only_header_matches() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// This stuff is not the # Summary.
            /// # Summary
            /// This is a
            /// multi-line # Summary!
            /// # Also Not The Summary
            /// This stuff is also not the # Summary.
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line # Summary!
        "#]],
    );
}

#[test]
fn callable_summary_successive_headers() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// # Summary
            /// This is a
            /// multi-line summary!
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
            ---
            This is a
            multi-line summary!
        "#]],
    );
}

#[test]
fn callable_empty_summary() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Not The Summary
            /// # Summary
            /// # Also Not The Summary
            operation ◉F↘oo◉() : Unit {}
        }
    "#},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Unit
            ```
        "#]],
    );
}

#[test]
fn callable_param_doc() {
    check(
        indoc! {r#"
        namespace Test {

            /// Doc string
            /// # Summary
            /// This is the summary
            /// # Input
            /// Input string
            /// ## x
            /// Doc string for `x`
            /// ### Note
            /// note for `x`
            /// ## other
            /// Doc string for `other`
            /// # Last
            /// Last string
            operation Foo(x: Int) : Unit {
                let y = ◉↘x◉;
            }
        }
    "#},
        &expect![[r#"
            parameter of `Foo`
            ```qsharp
            x : Int
            ```
            ---
            Doc string for `x`
            ### Note
            note for `x`
        "#]],
    );
}

#[test]
fn callable_generic_functor_display() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(op : (Qubit => Unit is Adj)) : Unit {}
                operation Main() : Unit {
                    ◉Fo↘o◉;
                }
            }
        "},
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo(op : (Qubit => Unit is Adj)) : Unit
            ```
        "#]],
    );
}

#[test]
fn udt_field_incorrect() {
    check_none(indoc! {r#"
        namespace Test {
            newtype Foo = (fst : Int, snd : Int);
            operation Bar() : Unit {
                let foo = Foo(1, 2);
                let x : Int = foo::◉n↘one◉;
            }
        }
    "#});
}

#[test]
fn std_udt_return_type() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation ◉Fo↘o◉() : Udt {
        }
    }
    "#,
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : Udt
            ```
        "#]],
    );
}

#[test]
fn std_callable_with_udt() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Udt {
            ◉Takes↘Udt◉()
        }
    }
    "#,
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            function TakesUdt(input : Udt) : Udt
            ```
        "#]],
    );
}

#[test]
fn struct_field_incorrect() {
    check_none(indoc! {r#"
        namespace Test {
            struct Foo { fst : Int, snd : Int }
            operation Bar() : Unit {
                let foo = new Foo { fst = 1, snd = 2 };
                let x : Int = foo::◉n↘one◉;
            }
        }
    "#});
}

#[test]
fn std_struct_return_type() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation ◉Fo↘o◉() : FakeStruct {}
    }
    "#,
        &expect![[r#"
            callable of `Test`
            ```qsharp
            operation Foo() : FakeStruct
            ```
        "#]],
    );
}

#[test]
fn std_callable_with_struct() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            ◉Takes↘Struct◉();
        }
    }
    "#,
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            function TakesStruct(input : FakeStruct) : FakeStruct
            ```
        "#]],
    );
}

#[test]
fn std_callable_with_type_param() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let temp = ◉FakeWi↘thTypeParam◉(3);
        }
    }
    "#,
        &expect![[r#"
            callable of `FakeStdLib`
            ```qsharp
            operation FakeWithTypeParam<'A>(a : 'A) : 'A
            ```
        "#]],
    );
}

#[test]
fn std_udt_udt_field() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Udt {
            let f = UdtWrapper(TakesUdt);
            f::inner::◉x◉↘
        }
    }
    "#,
        &expect![[r#"
            field of `Udt`
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn std_struct_struct_field() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : FakeStruct {
            let f = new StructWrapper { inner = new FakeStruct { x = 1, y = 2 } };
            f::inner::◉x◉↘
        }
    }
    "#,
        &expect![[r#"
            field of `FakeStruct`
            ```qsharp
            x : Int
            ```
        "#]],
    );
}

#[test]
fn ty_param_def() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<◉'↘T◉>(x : 'T) : 'T { x }
        }
    "#},
        &expect![[r#"
            type parameter of `Foo`
            ```qsharp
            'T
            ```
        "#]],
    );
}

#[test]
fn ty_param_ref() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'T>(x : ◉'↘T◉) : 'T { x }
        }
    "#},
        &expect![[r#"
            type parameter of `Foo`
            ```qsharp
            'T
            ```
        "#]],
    );
}

#[test]
fn notebook_callable_def_across_cells() {
    check_notebook(
        &[
            ("cell1", "operation Callee() : Unit {}"),
            ("cell2", "◉C↘allee◉();"),
        ],
        &expect![[r#"
            callable
            ```qsharp
            operation Callee() : Unit
            ```
        "#]],
    );
}

#[test]
fn notebook_callable_defined_in_later_cell() {
    check_notebook_none(&[
        ("cell1", "C↘allee();"),
        ("cell2", "operation Callee() : Unit {}"),
    ]);
}

#[test]
fn notebook_local_definition() {
    check_notebook(
        &[("cell1", "let x = 3;"), ("cell2", "let ◉↘y◉ = x + 1;")],
        &expect![[r#"
            local
            ```qsharp
            y : Int
            ```
        "#]],
    );
}

#[test]
fn notebook_local_reference() {
    check_notebook(
        &[("cell1", "let x = 3;"), ("cell2", "let y = ◉↘x◉ + 1;")],
        &expect![[r#"
            local
            ```qsharp
            x : Int
            ```
        "#]],
    );
}
