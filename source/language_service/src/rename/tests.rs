// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{get_rename, prepare_rename};
use crate::{
    Encoding,
    test_utils::{compile_notebook_with_markers, compile_with_markers},
};
use expect_test::{Expect, expect};

/// Asserts that the rename locations given at the cursor position matches the expected rename locations.
/// The cursor position is indicated by a `↘` marker in the source text.
/// The expected rename location ranges are indicated by `◉` markers in the source text.
fn check(source_with_markers: &str) {
    let (compilation, cursor_position, target_spans) =
        compile_with_markers(source_with_markers, true);
    let actual = get_rename(&compilation, "<source>", cursor_position, Encoding::Utf8)
        .into_iter()
        .map(|l| l.range)
        .collect::<Vec<_>>();
    for target in &target_spans {
        assert!(actual.contains(target));
    }
    assert_eq!(target_spans.len(), actual.len());
}

/// Asserts that the prepare rename given at the cursor position returns None.
/// The cursor position is indicated by a `↘` marker in the source text.
fn assert_no_rename(source_with_markers: &str) {
    let (compilation, cursor_position, _) = compile_with_markers(source_with_markers, true);
    let actual = prepare_rename(&compilation, "<source>", cursor_position, Encoding::Utf8);
    assert!(actual.is_none());
}

fn check_notebook(cells_with_markers: &[(&str, &str)], expect: &Expect) {
    let (compilation, cell_uri, position, _) = compile_notebook_with_markers(cells_with_markers);
    let actual = get_rename(&compilation, &cell_uri, position, Encoding::Utf8);
    expect.assert_debug_eq(&actual);
}

fn check_prepare_notebook(cells_with_markers: &[(&str, &str)], expect: &Expect) {
    let (compilation, cell_uri, position, _) = compile_notebook_with_markers(cells_with_markers);
    let actual = prepare_rename(&compilation, &cell_uri, position, Encoding::Utf8);
    expect.assert_debug_eq(&actual);
}

#[test]
fn callable_def() {
    check(
        r#"
        namespace Test {
            operation ◉Fo↘o◉(x : Int, y : Int, z : Int) : Unit {
                ◉Foo◉(x, y, z);
            }
            operation Bar(x : Int, y : Int, z : Int) : Unit {
                ◉Foo◉(x, y, z);
            }
        }
    "#,
    );
}

#[test]
fn callable_ref() {
    check(
        r#"
        namespace Test {
            operation ◉Foo◉(x : Int, y : Int, z : Int) : Unit {
                ◉Foo◉(x, y, z);
            }
            operation Bar(x : Int, y : Int, z : Int) : Unit {
                ◉Fo↘o◉(x, y, z);
            }
        }
    "#,
    );
}

#[test]
fn parameter_def() {
    check(
        r#"
        namespace Test {
            operation Foo(◉↘x◉ : Int, y : Int, z : Int) : Unit {
                let temp = ◉x◉;
                Foo(◉x◉, y, z);
            }
        }
    "#,
    );
}

#[test]
fn parameter_ref() {
    check(
        r#"
        namespace Test {
            operation Foo(◉x◉ : Int, y : Int, z : Int) : Unit {
                let temp = ◉x◉;
                Foo(◉↘x◉, y, z);
            }
        }
    "#,
    );
}

#[test]
fn local_def() {
    check(
        r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {
                let ◉t↘emp◉ = x;
                Foo(◉temp◉, y, ◉temp◉);
            }
        }
    "#,
    );
}

#[test]
fn local_ref() {
    check(
        r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {
                let ◉temp◉ = x;
                Foo(◉t↘emp◉, y, ◉temp◉);
            }
        }
    "#,
    );
}

#[test]
fn udt_def() {
    check(
        r#"
        namespace Test {
            newtype ◉F↘oo◉ = (fst : Int, snd : Int);
            operation Bar(x : ◉Foo◉) : Unit {
                let temp = ◉Foo◉(1, 2);
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn udt_constructor_ref() {
    check(
        r#"
        namespace Test {
            newtype ◉Foo◉ = (fst : Int, snd : Int);
            operation Bar(x : ◉Foo◉) : Unit {
                let temp = ◉F↘oo◉(1, 2);
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn udt_ref() {
    check(
        r#"
        namespace Test {
            newtype ◉Foo◉ = (fst : Int, snd : Int);
            operation Bar(x : ◉F↘oo◉) : Unit {
                let temp = ◉Foo◉(1, 2);
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn udt_field_def() {
    check(
        r#"
        namespace Test {
            newtype Foo = (◉f↘st◉ : Int, snd : Int);
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let a = temp::◉fst◉;
                let b = Zip()::◉fst◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn udt_field_ref() {
    check(
        r#"
        namespace Test {
            newtype Foo = (◉fst◉ : Int, snd : Int);
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let a = temp::◉f↘st◉;
                let b = Zip()::◉fst◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn udt_field_complex_ref() {
    check(
        r#"
        namespace Test {
            newtype Foo = (◉fst◉ : Int, snd : Int);
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let a = temp::◉fst◉;
                let b = Zip()::◉f↘st◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn struct_def() {
    check(
        r#"
        namespace Test {
            struct ◉F↘oo◉ { fst : Int, snd : Int }
            operation Bar(x : ◉Foo◉) : Unit {
                let temp = ◉Foo◉(1, 2);
                let temp = new ◉Foo◉ { fst = 1, snd = 2 };
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn struct_fn_constructor_ref() {
    check(
        r#"
        namespace Test {
            struct ◉Foo◉ { fst : Int, snd : Int }
            operation Bar(x : ◉Foo◉) : Unit {
                let temp = ◉F↘oo◉(1, 2);
                let temp = new ◉Foo◉ { fst = 1, snd = 2 };
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn struct_constructor_ref() {
    check(
        r#"
        namespace Test {
            struct ◉Foo◉ { fst : Int, snd : Int }
            operation Bar(x : ◉Foo◉) : Unit {
                let temp = ◉Foo◉(1, 2);
                let temp = new ◉F↘oo◉ { fst = 1, snd = 2 };
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn struct_ref() {
    check(
        r#"
        namespace Test {
            struct ◉Foo◉ { fst : Int, snd : Int }
            operation Bar(x : ◉F↘oo◉) : Unit {
                let temp = ◉Foo◉(1, 2);
                let temp = new ◉F↘oo◉ { fst = 1, snd = 2 };
                Bar(temp);
            }
        }
    "#,
    );
}

#[test]
fn struct_field_def() {
    check(
        r#"
        namespace Test {
            struct Foo { ◉f↘st◉ : Int, snd : Int }
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let temp = new Foo { ◉fst◉ = 1, snd = 2 };
                let a = temp::◉fst◉;
                let b = Zip()::◉fst◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn struct_field_cons_ref() {
    check(
        r#"
        namespace Test {
            struct Foo { ◉fst◉ : Int, snd : Int }
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let temp = new Foo { ◉f↘st◉ = 1, snd = 2 };
                let a = temp::◉fst◉;
                let b = Zip()::◉fst◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn struct_field_ref() {
    check(
        r#"
        namespace Test {
            struct Foo { ◉fst◉ : Int, snd : Int }
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let temp = new Foo { ◉fst◉ = 1, snd = 2 };
                let a = temp::◉f↘st◉;
                let b = Zip()::◉fst◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn struct_field_complex_ref() {
    check(
        r#"
        namespace Test {
            struct Foo { ◉fst◉ : Int, snd : Int }
            operation Bar(x : Foo) : Unit {
                let temp = Foo(1, 2);
                let temp = new Foo { ◉fst◉ = 1, snd = 2 };
                let a = temp::◉fst◉;
                let b = Zip()::◉f↘st◉;
            }
            operation Zip() : Foo {
                Foo(1, 2)
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_def() {
    check(
        r#"
        namespace Test {
            struct A { b : B }
            struct B { ◉↘c◉ : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = a.b.◉c◉.i;
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_ref() {
    check(
        r#"
        namespace Test {
            struct A { b : B }
            struct B { ◉c◉ : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = a.b.◉↘c◉.i;
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_first_def() {
    check(
        r#"
        namespace Test {
            struct A { b : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(◉↘a◉ : A) : Unit {
                let x = ◉a◉.b.c.i;
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_first_ref() {
    check(
        r#"
        namespace Test {
            struct A { b : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(◉a◉ : A) : Unit {
                let x = ◉↘a◉.b.c.i;
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_with_expr_def() {
    check(
        r#"
        namespace Test {
            struct A { ◉↘b◉ : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = { a.◉b◉ }.c.i;
            }
        }
    "#,
    );
}

#[test]
fn struct_field_path_with_expr_ref() {
    check(
        r#"
        namespace Test {
            struct A { ◉b◉ : B }
            struct B { c : C }
            struct C { i : Int }
            operation Foo(a : A) : Unit {
                let x = { a.◉↘b◉ }.c.i;
            }
        }
    "#,
    );
}

#[test]
fn no_rename_namespace() {
    assert_no_rename(
        r#"
        namespace Te↘st {
            operation Foo() : Unit {}

        }
    "#,
    );
}

#[test]
fn no_rename_keyword() {
    assert_no_rename(
        r#"
        namespace Test {
            ope↘ration Foo() : Unit {}

        }
    "#,
    );
}

#[test]
fn no_rename_non_udt_type() {
    assert_no_rename(
        r#"
        namespace Test {
            operation Foo() : Un↘it {}

        }
    "#,
    );
}

#[test]
fn no_rename_string() {
    assert_no_rename(
        r#"
        namespace Test {
            operation Foo() : Unit {
                let temp = "He↘llo World!"
            }

        }
    "#,
    );
}

#[test]
fn no_rename_comment() {
    assert_no_rename(
        r#"
        namespace Test {
            // He↘llo World!
            operation Foo() : Unit {}

        }
    "#,
    );
}

#[test]
fn no_rename_std_item() {
    assert_no_rename(
        r#"
        namespace Test {
            operation Foo() : Unit {
                F↘ake();
            }

        }
    "#,
    );
}

#[test]
fn no_rename_non_id_character() {
    assert_no_rename(
        r#"
        namespace Test {
            operation Foo() ↘: Unit {
                Fake();
            }

        }
    "#,
    );
}

#[test]
fn no_rename_std_udt_return_type() {
    assert_no_rename(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : U↘dt {
        }
    }
    "#,
    );
}

#[test]
fn no_rename_std_struct_return_type() {
    assert_no_rename(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : FakeS↘truct {}
    }
    "#,
    );
}

#[test]
fn ty_param_def() {
    check(
        r#"
        namespace Test {
            operation Foo<'◉↘T◉>(x : '◉T◉) : '◉T◉ { x }
        }
    "#,
    );
}

#[test]
fn ty_param_ref() {
    check(
        r#"
        namespace Test {
            operation Foo<'◉T◉>(x : '◉↘T◉) : '◉T◉ { x }
        }
    "#,
    );
}

#[test]
fn notebook_rename_defined_in_later_cell() {
    check_prepare_notebook(
        &[
            ("cell1", "C↘allee();"),
            ("cell2", "operation Callee() : Unit {}"),
        ],
        &expect![[r#"
            None
        "#]],
    );
}

#[test]
fn notebook_rename_across_cells() {
    check_notebook(
        &[
            ("cell1", "operation Callee() : Unit {}"),
            ("cell2", "◉C↘allee◉();"),
        ],
        &expect![[r#"
            [
                Location {
                    source: "cell1",
                    range: Range {
                        start: Position {
                            line: 0,
                            column: 10,
                        },
                        end: Position {
                            line: 0,
                            column: 16,
                        },
                    },
                },
                Location {
                    source: "cell2",
                    range: Range {
                        start: Position {
                            line: 0,
                            column: 0,
                        },
                        end: Position {
                            line: 0,
                            column: 6,
                        },
                    },
                },
            ]
        "#]],
    );
}

#[test]
fn on_declaration_with_aliased_export_finds_only_matching_names() {
    check(
        r#"
        namespace Test {
            operation ◉F↘oo◉() : Unit {
            }
            export ◉Foo◉ as Bar;
        }
        namespace Other {
            import Test.Bar;
            import Test.Bar as Baz;
            operation X() : Unit {
                Bar();
                Baz();
            }
        }
    "#,
    );
}

#[test]
fn on_export_alias_finds_only_matching_names() {
    check(
        r#"
        namespace Test {
            operation Foo() : Unit {
            }
            export Foo as Bar;
        }
        namespace Other {
            import Test.Bar;
            import Test.Bar as ◉Ba↘z◉;
            operation X() : Unit {
                Bar();
                ◉Baz◉();
            }
        }
    "#,
    );
}

#[test]
fn on_export_alias_usage_finds_only_matching_names() {
    check(
        r#"
        namespace Test {
            operation Foo() : Unit {
            }
            export Foo as Bar;
        }
        namespace Other {
            import Test.Bar;
            import Test.Bar as ◉Baz◉;
            operation X() : Unit {
                Bar();
                ◉B↘az◉();
            }
        }
    "#,
    );
}
