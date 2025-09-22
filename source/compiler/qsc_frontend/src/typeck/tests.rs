// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod bounded_polymorphism;

use crate::{
    compile::{self, Offsetter},
    resolve::{self, Resolver},
    typeck::Checker,
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_ast::{
    assigner::Assigner as AstAssigner,
    ast::{Block, Expr, Idents, NodeId, Package, Pat, Path, PathKind, QubitInit, TopLevelNode},
    mut_visit::MutVisitor,
    visit::{self, Visitor},
};
use qsc_data_structures::{index_map::IndexMap, language_features::LanguageFeatures, span::Span};
use qsc_hir::{assigner::Assigner as HirAssigner, ty::Ty};
use std::fmt::Write;

struct TyCollector<'a> {
    tys: &'a IndexMap<NodeId, Ty>,
    nodes: Vec<(NodeId, Span, Option<&'a Ty>)>,
}

impl<'a> Visitor<'a> for TyCollector<'a> {
    fn visit_block(&mut self, block: &'a Block) {
        let ty = self.tys.get(block.id);
        self.nodes.push((block.id, block.span, ty));
        visit::walk_block(self, block);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let ty = self.tys.get(expr.id);
        self.nodes.push((expr.id, expr.span, ty));
        visit::walk_expr(self, expr);
    }

    fn visit_path_kind(&mut self, path_kind: &'a PathKind) {
        visit::walk_path_kind(self, path_kind);
        if let PathKind::Err(Some(incomplete_path)) = path_kind {
            for part in incomplete_path.segments.iter() {
                let ty = self.tys.get(part.id);
                self.nodes.push((part.id, part.span, ty));
            }
        }
    }

    fn visit_path(&mut self, path: &'a Path) {
        visit::walk_path(self, path);
        let mut parts = path.iter().peekable();
        if self
            .tys
            .get(parts.peek().expect("should contain at least one part").id)
            .is_some()
        {
            for part in parts {
                let ty = self.tys.get(part.id);
                self.nodes.push((part.id, part.span, ty));
            }
        }
    }

    fn visit_pat(&mut self, pat: &'a Pat) {
        let ty = self.tys.get(pat.id);
        self.nodes.push((pat.id, pat.span, ty));
        visit::walk_pat(self, pat);
    }

    fn visit_qubit_init(&mut self, init: &'a QubitInit) {
        let ty = self.tys.get(init.id);
        self.nodes.push((init.id, init.span, ty));
        visit::walk_qubit_init(self, init);
    }
}

fn check(input: &str, entry_expr: &str, expect: &Expect) {
    check_with_error_option(input, entry_expr, expect, false);
}

fn check_allow_parse_errors(input: &str, entry_expr: &str, expect: &Expect) {
    check_with_error_option(input, entry_expr, expect, true);
}

fn check_with_error_option(input: &str, entry_expr: &str, expect: &Expect, allow_errors: bool) {
    let (package, tys, errors) = compile(input, entry_expr, allow_errors);
    let mut collector = TyCollector {
        tys: &tys.terms,
        nodes: Vec::new(),
    };
    collector.visit_package(&package);
    let mut actual = String::new();

    for (id, span, ty) in collector.nodes {
        let source = if (span.lo as usize) < input.len() {
            &input[span.lo as usize..span.hi as usize]
        } else {
            &entry_expr[span.lo as usize - input.len()..span.hi as usize - input.len()]
        };
        let ty = ty.unwrap_or(&Ty::Err);

        writeln!(actual, "#{id} {}-{} {source:?} : {ty}", span.lo, span.hi)
            .expect("string should be writable");
    }

    for error in errors {
        writeln!(actual, "{error:?}").expect("writing error to string should succeed");
    }

    expect.assert_eq(&actual);
}

fn compile(
    input: &str,
    entry_expr: &str,
    allow_errors: bool,
) -> (Package, super::Table, Vec<compile::Error>) {
    let mut package = parse(input, entry_expr, allow_errors);
    AstAssigner::new().visit_package(&mut package);
    let mut assigner = HirAssigner::new();

    let mut globals = resolve::GlobalTable::new();
    let mut errors = globals.add_local_package(&mut assigner, &package);
    let mut resolver = Resolver::new(globals, Vec::new());
    resolver.resolve(&mut assigner, &package);
    let (names, _, _, mut resolve_errors) = resolver.into_result();
    errors.append(&mut resolve_errors);

    let mut checker = Checker::new(super::GlobalTable::new());
    checker.check_package(&names, &package);
    let (tys, ty_errors) = checker.into_table();

    let errors = errors
        .into_iter()
        .map(Into::into)
        .chain(ty_errors.into_iter().map(Into::into))
        .map(compile::Error)
        .collect();
    (package, tys, errors)
}

fn parse(input: &str, entry_expr: &str, allow_errors: bool) -> Package {
    let (namespaces, errors) = qsc_parse::namespaces(input, None, LanguageFeatures::default());
    assert!(
        allow_errors || errors.is_empty(),
        "parsing input failed: {errors:#?}"
    );

    let entry = if entry_expr.is_empty() {
        None
    } else {
        let (mut entry, errors) = qsc_parse::expr(entry_expr, LanguageFeatures::default());
        let offset = input
            .len()
            .try_into()
            .expect("input length should fit into offset");
        assert!(errors.is_empty(), "parsing entry failed: {errors:#?}");
        Offsetter(offset).visit_expr(&mut entry);
        Some(entry)
    };

    Package {
        id: NodeId::default(),
        nodes: namespaces
            .into_iter()
            .map(TopLevelNode::Namespace)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        entry,
    }
}

#[test]
fn empty_callable() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {}
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-42 "{}" : Unit
        "#]],
    );
}

#[test]
fn return_constant() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int { 4 }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-44 "{ 4 }" : Int
            #12 41-42 "4" : Int
        "#]],
    );
}

#[test]
fn return_wrong_type() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int { true }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-47 "{ true }" : Bool
            #12 41-45 "true" : Bool
            Error(Type(Error(TyMismatch("Int", "Bool", Span { lo: 41, hi: 45 }))))
        "#]],
    );
}

#[test]
fn return_semi() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int { 4; }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-45 "{ 4; }" : Unit
            #12 41-42 "4" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 35, hi: 38 }))))
        "#]],
    );
}

#[test]
fn explicit_type_in_let_binding() {
    check(
        "",
        "{ let x : Int = 4; }",
        &expect![[r##"
            #1 0-20 "{ let x : Int = 4; }" : Unit
            #2 0-20 "{ let x : Int = 4; }" : Unit
            #4 6-13 "x : Int" : Int
            #9 16-17 "4" : Int
        "##]],
    );
}

#[test]
fn incorrect_explicit_type_in_let_binding_error() {
    check(
        "",
        "{ let x : Int = 4.0; }",
        &expect![[r##"
            #1 0-22 "{ let x : Int = 4.0; }" : Unit
            #2 0-22 "{ let x : Int = 4.0; }" : Unit
            #4 6-13 "x : Int" : Int
            #9 16-19 "4.0" : Double
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 16, hi: 19 }))))
        "##]],
    );
}

#[test]
fn incorrect_explicit_type_in_let_binding_used_later_correctly() {
    check(
        "",
        "{ let x : Int = 4.0; x + 1}",
        &expect![[r##"
            #1 0-27 "{ let x : Int = 4.0; x + 1}" : Int
            #2 0-27 "{ let x : Int = 4.0; x + 1}" : Int
            #4 6-13 "x : Int" : Int
            #9 16-19 "4.0" : Double
            #11 21-26 "x + 1" : Int
            #12 21-22 "x" : Int
            #15 25-26 "1" : Int
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 16, hi: 19 }))))
        "##]],
    );
}

#[test]
fn return_var() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    let x = 4;
                    x
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-75 "{\n        let x = 4;\n        x\n    }" : Int
            #12 53-54 "x" : Int
            #14 57-58 "4" : Int
            #16 68-69 "x" : Int
        "#]],
    );
}

#[test]
fn call_function() {
    check(
        indoc! {"
            namespace A {
                function Foo(x : Int) : Int { x }
                function Bar() : Int { Foo(4) }
            }
        "},
        "",
        &expect![[r#"
            #6 30-39 "(x : Int)" : Int
            #7 31-38 "x : Int" : Int
            #15 46-51 "{ x }" : Int
            #17 48-49 "x" : Int
            #23 68-70 "()" : Unit
            #27 77-87 "{ Foo(4) }" : Int
            #29 79-85 "Foo(4)" : Int
            #30 79-82 "Foo" : (Int -> Int)
            #33 82-85 "(4)" : Int
            #34 83-84 "4" : Int
        "#]],
    );
}

#[test]
fn call_generic_identity() {
    check(
        indoc! {"
            namespace A {
                function Identity<'T>(x : 'T) : 'T { x }
                function Foo() : Int { Identity(4) }
            }
        "},
        "",
        &expect![[r#"
            #7 39-47 "(x : 'T)" : Param<"'T": 0>
            #8 40-46 "x : 'T" : Param<"'T": 0>
            #14 53-58 "{ x }" : Param<"'T": 0>
            #16 55-56 "x" : Param<"'T": 0>
            #22 75-77 "()" : Unit
            #26 84-99 "{ Identity(4) }" : Int
            #28 86-97 "Identity(4)" : Int
            #29 86-94 "Identity" : (Int -> Int)
            #32 94-97 "(4)" : Int
            #33 95-96 "4" : Int
        "#]],
    );
}

#[test]
fn call_generic_length() {
    check(
        indoc! {"
            namespace Std.Core {
                function Length<'T>(xs : 'T[]) : Int { body intrinsic; }
            }
        "},
        "Length([true, false, true])",
        &expect![[r##"
            #8 44-55 "(xs : 'T[])" : ?
            #9 45-54 "xs : 'T[]" : ?
            #18 84-111 "Length([true, false, true])" : Int
            #19 84-90 "Length" : (Bool[] -> Int)
            #22 90-111 "([true, false, true])" : Bool[]
            #23 91-110 "[true, false, true]" : Bool[]
            #24 92-96 "true" : Bool
            #25 98-103 "false" : Bool
            #26 105-109 "true" : Bool
        "##]],
    );
}

#[test]
fn nested_generic_with_lambda() {
    check(
        indoc! {"
            namespace A {
                function Foo<'I, 'O>(f : 'I -> 'O, x : 'I) : 'O { f(x) }
                function Bar() : Unit {
                    let r0 = Foo(Foo, (() -> (), ()));
                    let r1 = Foo(Foo, (a -> (), ()));
                    let r2 = Foo(Foo, (b -> b, ()));
                }
        }"
        },
        "",
        &expect![[r##"
            #8 46-68 "(f : 'I -> 'O, x : 'I)" : ((Param<"'I": 0> -> Param<"'O": 1>), Param<"'I": 0>)
            #9 47-59 "f : 'I -> 'O" : (Param<"'I": 0> -> Param<"'O": 1>)
            #16 61-67 "x : 'I" : Param<"'I": 0>
            #22 74-82 "{ f(x) }" : Param<"'O": 1>
            #24 76-80 "f(x)" : Param<"'O": 1>
            #25 76-77 "f" : (Param<"'I": 0> -> Param<"'O": 1>)
            #28 77-80 "(x)" : Param<"'I": 0>
            #29 78-79 "x" : Param<"'I": 0>
            #35 103-105 "()" : Unit
            #39 113-262 "{\n            let r0 = Foo(Foo, (() -> (), ()));\n            let r1 = Foo(Foo, (a -> (), ()));\n            let r2 = Foo(Foo, (b -> b, ()));\n        }" : Unit
            #41 131-133 "r0" : Unit
            #43 136-160 "Foo(Foo, (() -> (), ()))" : Unit
            #44 136-139 "Foo" : (((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit)) -> Unit)
            #47 139-160 "(Foo, (() -> (), ()))" : ((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit))
            #48 140-143 "Foo" : (((Unit -> Unit), Unit) -> Unit)
            #51 145-159 "(() -> (), ())" : ((Unit -> Unit), Unit)
            #52 146-154 "() -> ()" : (Unit -> Unit)
            #53 146-148 "()" : Unit
            #54 152-154 "()" : Unit
            #55 156-158 "()" : Unit
            #57 178-180 "r1" : Unit
            #59 183-206 "Foo(Foo, (a -> (), ()))" : Unit
            #60 183-186 "Foo" : (((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit)) -> Unit)
            #63 186-206 "(Foo, (a -> (), ()))" : ((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit))
            #64 187-190 "Foo" : (((Unit -> Unit), Unit) -> Unit)
            #67 192-205 "(a -> (), ())" : ((Unit -> Unit), Unit)
            #68 193-200 "a -> ()" : (Unit -> Unit)
            #69 193-194 "a" : Unit
            #71 198-200 "()" : Unit
            #72 202-204 "()" : Unit
            #74 224-226 "r2" : Unit
            #76 229-251 "Foo(Foo, (b -> b, ()))" : Unit
            #77 229-232 "Foo" : (((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit)) -> Unit)
            #80 232-251 "(Foo, (b -> b, ()))" : ((((Unit -> Unit), Unit) -> Unit), ((Unit -> Unit), Unit))
            #81 233-236 "Foo" : (((Unit -> Unit), Unit) -> Unit)
            #84 238-250 "(b -> b, ())" : ((Unit -> Unit), Unit)
            #85 239-245 "b -> b" : (Unit -> Unit)
            #86 239-240 "b" : Unit
            #88 244-245 "b" : Unit
            #91 247-249 "()" : Unit
        "##]],
    );
}

#[test]
fn add_wrong_types() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit { 1 + [2]; }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-52 "{ 1 + [2]; }" : Unit
            #12 42-49 "1 + [2]" : Int
            #13 42-43 "1" : Int
            #14 46-49 "[2]" : Int[]
            #15 47-48 "2" : Int
            Error(Type(Error(TyMismatch("Int", "Int[]", Span { lo: 46, hi: 49 }))))
        "#]],
    );
}

#[test]
fn int_as_double_error() {
    check(
        indoc! {"
            namespace Microsoft.Quantum.Convert {
                function IntAsDouble(a : Int) : Double { body intrinsic; }
            }
        "},
        "Microsoft.Quantum.Convert.IntAsDouble(false)",
        &expect![[r#"
            #8 62-71 "(a : Int)" : ?
            #9 63-70 "a : Int" : ?
            #18 103-147 "Microsoft.Quantum.Convert.IntAsDouble(false)" : Double
            #19 103-140 "Microsoft.Quantum.Convert.IntAsDouble" : (Int -> Double)
            #25 140-147 "(false)" : Bool
            #26 141-146 "false" : Bool
            Error(Type(Error(TyMismatch("Int", "Bool", Span { lo: 103, hi: 147 }))))
        "#]],
    );
}

#[test]
fn length_type_error() {
    check(
        indoc! {"
            namespace Std.Core {
                function Length<'T>(xs : 'T[]) : Int { body intrinsic; }
            }
        "},
        "Length((1, 2, 3))",
        &expect![[r##"
            #8 44-55 "(xs : 'T[])" : ?
            #9 45-54 "xs : 'T[]" : ?
            #18 84-101 "Length((1, 2, 3))" : Int
            #19 84-90 "Length" : (?0[] -> Int)
            #22 90-101 "((1, 2, 3))" : (Int, Int, Int)
            #23 91-100 "(1, 2, 3)" : (Int, Int, Int)
            #24 92-93 "1" : Int
            #25 95-96 "2" : Int
            #26 98-99 "3" : Int
            Error(Type(Error(TyMismatch("?[]", "(Int, Int, Int)", Span { lo: 84, hi: 101 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 84, hi: 90 }))))
        "##]],
    );
}

#[test]
fn single_arg_for_tuple() {
    check(
        indoc! {"
            namespace Std.Intrinsic {
                operation Ry(theta : Double, qubit : Qubit) : () is Adj + Ctl {}
            }
        "},
        indoc! {"{
            use q = Qubit();
            Ry(q);
        }"},
        &expect![[r##"
            #7 42-73 "(theta : Double, qubit : Qubit)" : (Double, Qubit)
            #8 43-57 "theta : Double" : Double
            #13 59-72 "qubit : Qubit" : Qubit
            #22 92-94 "{}" : Unit
            #23 97-132 "{\n    use q = Qubit();\n    Ry(q);\n}" : Unit
            #24 97-132 "{\n    use q = Qubit();\n    Ry(q);\n}" : Unit
            #26 107-108 "q" : Qubit
            #28 111-118 "Qubit()" : Qubit
            #30 124-129 "Ry(q)" : Unit
            #31 124-126 "Ry" : ((Double, Qubit) => Unit is Adj + Ctl)
            #34 126-129 "(q)" : Qubit
            #35 127-128 "q" : Qubit
            Error(Type(Error(TyMismatch("(Double, Qubit)", "Qubit", Span { lo: 124, hi: 129 }))))
        "##]],
    );
}

#[test]
fn array_index_error() {
    check(
        "",
        "[1, 2, 3][false]",
        &expect![[r#"
            #1 0-16 "[1, 2, 3][false]" : ?0
            #2 0-9 "[1, 2, 3]" : Int[]
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 10-15 "false" : Bool
            Error(Type(Error(MissingClassHasIndex("Int[]", "Bool", Span { lo: 0, hi: 16 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 16 }))))
        "#]],
    );
}

#[test]
fn array_repeat_error() {
    check(
        "",
        "[4, size = true]",
        &expect![[r#"
            #1 0-16 "[4, size = true]" : Int[]
            #2 1-2 "4" : Int
            #3 11-15 "true" : Bool
            Error(Type(Error(TyMismatch("Int", "Bool", Span { lo: 11, hi: 15 }))))
        "#]],
    );
}

#[test]
fn assignop_error() {
    check(
        "",
        indoc! {"
            {
                mutable x = false;
                set x += 1;
                x
            }
        "},
        &expect![[r#"
            #1 0-48 "{\n    mutable x = false;\n    set x += 1;\n    x\n}" : Bool
            #2 0-48 "{\n    mutable x = false;\n    set x += 1;\n    x\n}" : Bool
            #4 14-15 "x" : Bool
            #6 18-23 "false" : Bool
            #8 29-39 "set x += 1" : Unit
            #9 33-34 "x" : Bool
            #12 38-39 "1" : Int
            #14 45-46 "x" : Bool
            Error(Type(Error(TyMismatch("Bool", "Int", Span { lo: 38, hi: 39 }))))
            Error(Type(Error(MissingClassAdd("Bool", Span { lo: 33, hi: 34 }))))
        "#]],
    );
}

#[test]
fn binop_add_invalid() {
    check(
        "",
        "(1, 3) + 5.4",
        &expect![[r#"
            #1 0-12 "(1, 3) + 5.4" : (Int, Int)
            #2 0-6 "(1, 3)" : (Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "3" : Int
            #5 9-12 "5.4" : Double
            Error(Type(Error(TyMismatch("(Int, Int)", "Double", Span { lo: 9, hi: 12 }))))
            Error(Type(Error(MissingClassAdd("(Int, Int)", Span { lo: 0, hi: 6 }))))
        "#]],
    );
}

#[test]
fn binop_add_mismatch() {
    check(
        "",
        "1 + 5.4",
        &expect![[r#"
            #1 0-7 "1 + 5.4" : Int
            #2 0-1 "1" : Int
            #3 4-7 "5.4" : Double
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 4, hi: 7 }))))
        "#]],
    );
}

#[test]
fn binop_andb_invalid() {
    check(
        "",
        "2.8 &&& 5.4",
        &expect![[r#"
            #1 0-11 "2.8 &&& 5.4" : Double
            #2 0-3 "2.8" : Double
            #3 8-11 "5.4" : Double
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 0, hi: 3 }))))
        "#]],
    );
}

#[test]
fn binop_andb_mismatch() {
    check(
        "",
        "28 &&& 54L",
        &expect![[r#"
            #1 0-10 "28 &&& 54L" : Int
            #2 0-2 "28" : Int
            #3 7-10 "54L" : BigInt
            Error(Type(Error(TyMismatch("Int", "BigInt", Span { lo: 7, hi: 10 }))))
        "#]],
    );
}

#[test]
fn binop_equal_callable() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit {}
                function B() : Unit {}
            }
        "},
        "Test.A == Test.B",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #10 41-43 "{}" : Unit
            #14 58-60 "()" : Unit
            #18 68-70 "{}" : Unit
            #19 73-89 "Test.A == Test.B" : Bool
            #20 73-79 "Test.A" : (Unit -> Unit)
            #24 83-89 "Test.B" : (Unit -> Unit)
            Error(Type(Error(MissingClassEq("(Unit -> Unit)", Span { lo: 73, hi: 79 }))))
        "#]],
    );
}

#[test]
fn binop_equal_tuple_arity_mismatch() {
    check(
        "",
        "(1, 2, 3) == (1, 2, 3, 4)",
        &expect![[r#"
            #1 0-25 "(1, 2, 3) == (1, 2, 3, 4)" : Bool
            #2 0-9 "(1, 2, 3)" : (Int, Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-25 "(1, 2, 3, 4)" : (Int, Int, Int, Int)
            #7 14-15 "1" : Int
            #8 17-18 "2" : Int
            #9 20-21 "3" : Int
            #10 23-24 "4" : Int
            Error(Type(Error(TyMismatch("(Int, Int, Int)", "(Int, Int, Int, Int)", Span { lo: 13, hi: 25 }))))
        "#]],
    );
}

#[test]
fn binop_equal_tuple_type_mismatch() {
    check(
        "",
        "(1, 2, 3) == (1, Zero, 3)",
        &expect![[r#"
            #1 0-25 "(1, 2, 3) == (1, Zero, 3)" : Bool
            #2 0-9 "(1, 2, 3)" : (Int, Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-25 "(1, Zero, 3)" : (Int, Result, Int)
            #7 14-15 "1" : Int
            #8 17-21 "Zero" : Result
            #9 23-24 "3" : Int
            Error(Type(Error(TyMismatch("Int", "Result", Span { lo: 13, hi: 25 }))))
        "#]],
    );
}

#[test]
fn binop_eq_mismatch() {
    check(
        "",
        "18L == 18",
        &expect![[r#"
            #1 0-9 "18L == 18" : Bool
            #2 0-3 "18L" : BigInt
            #3 7-9 "18" : Int
            Error(Type(Error(TyMismatch("BigInt", "Int", Span { lo: 7, hi: 9 }))))
        "#]],
    );
}

#[test]
fn binop_neq_mismatch() {
    check(
        "",
        "18L != 18",
        &expect![[r#"
            #1 0-9 "18L != 18" : Bool
            #2 0-3 "18L" : BigInt
            #3 7-9 "18" : Int
            Error(Type(Error(TyMismatch("BigInt", "Int", Span { lo: 7, hi: 9 }))))
        "#]],
    );
}

#[test]
fn binop_neq_tuple_type_mismatch() {
    check(
        "",
        "(1, 2, 3) != (1, Zero, 3)",
        &expect![[r#"
            #1 0-25 "(1, 2, 3) != (1, Zero, 3)" : Bool
            #2 0-9 "(1, 2, 3)" : (Int, Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-25 "(1, Zero, 3)" : (Int, Result, Int)
            #7 14-15 "1" : Int
            #8 17-21 "Zero" : Result
            #9 23-24 "3" : Int
            Error(Type(Error(TyMismatch("Int", "Result", Span { lo: 13, hi: 25 }))))
        "#]],
    );
}

#[test]
fn binop_neq_tuple_arity_mismatch() {
    check(
        "",
        "(1, 2, 3) != (1, 2, 3, 4)",
        &expect![[r#"
            #1 0-25 "(1, 2, 3) != (1, 2, 3, 4)" : Bool
            #2 0-9 "(1, 2, 3)" : (Int, Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-25 "(1, 2, 3, 4)" : (Int, Int, Int, Int)
            #7 14-15 "1" : Int
            #8 17-18 "2" : Int
            #9 20-21 "3" : Int
            #10 23-24 "4" : Int
            Error(Type(Error(TyMismatch("(Int, Int, Int)", "(Int, Int, Int, Int)", Span { lo: 13, hi: 25 }))))
        "#]],
    );
}

#[test]
fn binop_orb_invalid() {
    check(
        "",
        "2.8 ||| 5.4",
        &expect![[r#"
            #1 0-11 "2.8 ||| 5.4" : Double
            #2 0-3 "2.8" : Double
            #3 8-11 "5.4" : Double
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 0, hi: 3 }))))
        "#]],
    );
}

#[test]
fn binop_orb_mismatch() {
    check(
        "",
        "28 ||| 54L",
        &expect![[r#"
            #1 0-10 "28 ||| 54L" : Int
            #2 0-2 "28" : Int
            #3 7-10 "54L" : BigInt
            Error(Type(Error(TyMismatch("Int", "BigInt", Span { lo: 7, hi: 10 }))))
        "#]],
    );
}

#[test]
fn binop_xorb_invalid() {
    check(
        "",
        "2.8 ^^^ 5.4",
        &expect![[r#"
            #1 0-11 "2.8 ^^^ 5.4" : Double
            #2 0-3 "2.8" : Double
            #3 8-11 "5.4" : Double
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 0, hi: 3 }))))
        "#]],
    );
}

#[test]
fn binop_xorb_mismatch() {
    check(
        "",
        "28 ^^^ 54L",
        &expect![[r#"
            #1 0-10 "28 ^^^ 54L" : Int
            #2 0-2 "28" : Int
            #3 7-10 "54L" : BigInt
            Error(Type(Error(TyMismatch("Int", "BigInt", Span { lo: 7, hi: 10 }))))
        "#]],
    );
}

#[test]
fn let_tuple_arity_error() {
    check(
        "",
        "{ let (x, y, z) = (0, 1); }",
        &expect![[r#"
            #1 0-27 "{ let (x, y, z) = (0, 1); }" : Unit
            #2 0-27 "{ let (x, y, z) = (0, 1); }" : Unit
            #4 6-15 "(x, y, z)" : (Int, Int, ?2)
            #5 7-8 "x" : Int
            #7 10-11 "y" : Int
            #9 13-14 "z" : ?2
            #11 18-24 "(0, 1)" : (Int, Int)
            #12 19-20 "0" : Int
            #13 22-23 "1" : Int
            Error(Type(Error(TyMismatch("(?, ?, ?)", "(Int, Int)", Span { lo: 18, hi: 24 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 13, hi: 14 }))))
        "#]],
    );
}

#[test]
fn set_tuple_arity_error() {
    check(
        "",
        indoc! {"
            {
                mutable (x, y) = (0, 1);
                set (x, y) = (1, 2, 3);
                x
            }
        "},
        &expect![[r#"
            #1 0-66 "{\n    mutable (x, y) = (0, 1);\n    set (x, y) = (1, 2, 3);\n    x\n}" : Int
            #2 0-66 "{\n    mutable (x, y) = (0, 1);\n    set (x, y) = (1, 2, 3);\n    x\n}" : Int
            #4 14-20 "(x, y)" : (Int, Int)
            #5 15-16 "x" : Int
            #7 18-19 "y" : Int
            #9 23-29 "(0, 1)" : (Int, Int)
            #10 24-25 "0" : Int
            #11 27-28 "1" : Int
            #13 35-57 "set (x, y) = (1, 2, 3)" : Unit
            #14 39-45 "(x, y)" : (Int, Int)
            #15 40-41 "x" : Int
            #18 43-44 "y" : Int
            #21 48-57 "(1, 2, 3)" : (Int, Int, Int)
            #22 49-50 "1" : Int
            #23 52-53 "2" : Int
            #24 55-56 "3" : Int
            #26 63-64 "x" : Int
            Error(Type(Error(TyMismatch("(Int, Int)", "(Int, Int, Int)", Span { lo: 39, hi: 45 }))))
        "#]],
    );
}

#[test]
fn qubit_array_length_error() {
    check(
        "",
        "{ use q = Qubit[false]; }",
        &expect![[r#"
            #1 0-25 "{ use q = Qubit[false]; }" : Unit
            #2 0-25 "{ use q = Qubit[false]; }" : Unit
            #4 6-7 "q" : Qubit[]
            #6 10-22 "Qubit[false]" : Qubit[]
            #7 16-21 "false" : Bool
            Error(Type(Error(TyMismatch("Int", "Bool", Span { lo: 16, hi: 21 }))))
        "#]],
    );
}

#[test]
fn qubit_tuple_arity_error() {
    check(
        "",
        "{ use (q, q1) = (Qubit[3], Qubit(), Qubit()); }",
        &expect![[r#"
            #1 0-47 "{ use (q, q1) = (Qubit[3], Qubit(), Qubit()); }" : Unit
            #2 0-47 "{ use (q, q1) = (Qubit[3], Qubit(), Qubit()); }" : Unit
            #4 6-13 "(q, q1)" : (Qubit[], Qubit)
            #5 7-8 "q" : Qubit[]
            #7 10-12 "q1" : Qubit
            #9 16-44 "(Qubit[3], Qubit(), Qubit())" : (Qubit[], Qubit, Qubit)
            #10 17-25 "Qubit[3]" : Qubit[]
            #11 23-24 "3" : Int
            #12 27-34 "Qubit()" : Qubit
            #13 36-43 "Qubit()" : Qubit
            Error(Type(Error(TyMismatch("(Qubit[], Qubit, Qubit)", "(?, ?)", Span { lo: 6, hi: 13 }))))
        "#]],
    );
}

#[test]
fn for_loop_not_iterable() {
    check(
        "",
        "for i in (1, true, One) {}",
        &expect![[r#"
            #1 0-26 "for i in (1, true, One) {}" : Unit
            #2 4-5 "i" : ?0
            #4 9-23 "(1, true, One)" : (Int, Bool, Result)
            #5 10-11 "1" : Int
            #6 13-17 "true" : Bool
            #7 19-22 "One" : Result
            #8 24-26 "{}" : Unit
            Error(Type(Error(MissingClassIterable("(Int, Bool, Result)", Span { lo: 9, hi: 23 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 4, hi: 5 }))))
        "#]],
    );
}

#[test]
fn for_loop_body_should_be_unit_error() {
    check(
        "",
        "for i in [1, 2, 3] { 4 }",
        &expect![[r##"
        #1 0-24 "for i in [1, 2, 3] { 4 }" : Unit
        #2 4-5 "i" : Int
        #4 9-18 "[1, 2, 3]" : Int[]
        #5 10-11 "1" : Int
        #6 13-14 "2" : Int
        #7 16-17 "3" : Int
        #8 19-24 "{ 4 }" : Int
        #10 21-22 "4" : Int
        Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 19, hi: 24 }))))
    "##]],
    );
}

#[test]
fn for_loop_correct_explicit_type_works() {
    check(
        "",
        "for i : Int in 0..1 { i; }",
        &expect![[r##"
            #1 0-26 "for i : Int in 0..1 { i; }" : Unit
            #2 4-11 "i : Int" : Int
            #7 15-19 "0..1" : Range
            #8 15-16 "0" : Int
            #9 18-19 "1" : Int
            #10 20-26 "{ i; }" : Unit
            #12 22-23 "i" : Int
        "##]],
    );
}

#[test]
fn for_loop_incorrect_explicit_type_error() {
    check(
        "",
        "for i : Double in 0..1 { i; }",
        &expect![[r##"
            #1 0-29 "for i : Double in 0..1 { i; }" : Unit
            #2 4-14 "i : Double" : Double
            #7 18-22 "0..1" : Range
            #8 18-19 "0" : Int
            #9 21-22 "1" : Int
            #10 23-29 "{ i; }" : Unit
            #12 25-26 "i" : Double
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 18, hi: 22 }))))
        "##]],
    );
}

#[test]
fn repeat_loop_non_bool_condition_error() {
    check(
        "",
        "repeat { } until 1",
        &expect![[r##"
            #1 0-18 "repeat { } until 1" : Unit
            #2 7-10 "{ }" : Unit
            #3 17-18 "1" : Int
            Error(Type(Error(TyMismatch("Bool", "Int", Span { lo: 17, hi: 18 }))))
        "##]],
    );
}

#[test]
fn repeat_loop_body_should_be_unit_error() {
    check(
        "",
        "repeat { 1 } until false",
        &expect![[r##"
            #1 0-24 "repeat { 1 } until false" : Unit
            #2 7-12 "{ 1 }" : Int
            #4 9-10 "1" : Int
            #5 19-24 "false" : Bool
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 7, hi: 12 }))))
        "##]],
    );
}

#[test]
fn repeat_loop_fixup_should_be_unit_error() {
    check(
        "",
        "repeat { } until false fixup { 1 }",
        &expect![[r##"
            #1 0-34 "repeat { } until false fixup { 1 }" : Unit
            #2 7-10 "{ }" : Unit
            #3 17-22 "false" : Bool
            #4 29-34 "{ 1 }" : Int
            #6 31-32 "1" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 29, hi: 34 }))))
        "##]],
    );
}

#[test]
fn if_cond_error() {
    check(
        "",
        "if 4 {}",
        &expect![[r#"
            #1 0-7 "if 4 {}" : Unit
            #2 3-4 "4" : Int
            #3 5-7 "{}" : Unit
            Error(Type(Error(TyMismatch("Bool", "Int", Span { lo: 3, hi: 4 }))))
        "#]],
    );
}

#[test]
fn if_no_else_must_be_unit() {
    check(
        "",
        "if true { 4 }",
        &expect![[r#"
            #1 0-13 "if true { 4 }" : Int
            #2 3-7 "true" : Bool
            #3 8-13 "{ 4 }" : Int
            #5 10-11 "4" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 8, hi: 13 }))))
        "#]],
    );
}

#[test]
fn if_else_fail() {
    check(
        "",
        r#"if false {} else { fail "error"; }"#,
        &expect![[r#"
            #1 0-34 "if false {} else { fail \"error\"; }" : Unit
            #2 3-8 "false" : Bool
            #3 9-11 "{}" : Unit
            #4 12-34 "else { fail \"error\"; }" : Unit
            #5 17-34 "{ fail \"error\"; }" : Unit
            #7 19-31 "fail \"error\"" : Unit
            #8 24-31 "\"error\"" : String
        "#]],
    );
}

#[test]
fn if_cond_fail() {
    check(
        indoc! {r#"
            namespace A {
                function F() : Int {
                    if fail "error" {
                        "this type doesn't matter"
                    } else {
                        "foo"
                    }
                }
            }
        "#},
        "",
        &expect![[r#"
            #6 28-30 "()" : Unit
            #10 37-154 "{\n        if fail \"error\" {\n            \"this type doesn't matter\"\n        } else {\n            \"foo\"\n        }\n    }" : Int
            #12 47-148 "if fail \"error\" {\n            \"this type doesn't matter\"\n        } else {\n            \"foo\"\n        }" : Int
            #13 50-62 "fail \"error\"" : Bool
            #14 55-62 "\"error\"" : String
            #15 63-113 "{\n            \"this type doesn't matter\"\n        }" : String
            #17 77-103 "\"this type doesn't matter\"" : String
            #18 114-148 "else {\n            \"foo\"\n        }" : String
            #19 119-148 "{\n            \"foo\"\n        }" : String
            #21 133-138 "\"foo\"" : String
        "#]],
    );
}

#[test]
fn if_all_diverge() {
    check(
        indoc! {r#"
            namespace A {
                function F() : Int {
                    if fail "cond" {
                        fail "true"
                    } else {
                        fail "false"
                    }
                }
            }
        "#},
        "",
        &expect![[r#"
            #6 28-30 "()" : Unit
            #10 37-145 "{\n        if fail \"cond\" {\n            fail \"true\"\n        } else {\n            fail \"false\"\n        }\n    }" : Int
            #12 47-139 "if fail \"cond\" {\n            fail \"true\"\n        } else {\n            fail \"false\"\n        }" : Int
            #13 50-61 "fail \"cond\"" : Bool
            #14 55-61 "\"cond\"" : String
            #15 62-97 "{\n            fail \"true\"\n        }" : Int
            #17 76-87 "fail \"true\"" : Int
            #18 81-87 "\"true\"" : String
            #19 98-139 "else {\n            fail \"false\"\n        }" : Int
            #20 103-139 "{\n            fail \"false\"\n        }" : Int
            #22 117-129 "fail \"false\"" : Int
            #23 122-129 "\"false\"" : String
        "#]],
    );
}

#[test]
fn ternop_cond_error() {
    check(
        "",
        "7 ? 1 | 0",
        &expect![[r#"
            #1 0-9 "7 ? 1 | 0" : Int
            #2 0-1 "7" : Int
            #3 4-5 "1" : Int
            #4 8-9 "0" : Int
            Error(Type(Error(TyMismatch("Bool", "Int", Span { lo: 0, hi: 1 }))))
        "#]],
    );
}

#[test]
fn ternop_update_invalid_container() {
    check(
        "",
        "(1, 2, 3) w/ 2 <- 4",
        &expect![[r#"
            #1 0-19 "(1, 2, 3) w/ 2 <- 4" : (Int, Int, Int)
            #2 0-9 "(1, 2, 3)" : (Int, Int, Int)
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-14 "2" : Int
            #7 18-19 "4" : Int
            Error(Type(Error(MissingClassHasIndex("(Int, Int, Int)", "Int", Span { lo: 0, hi: 19 }))))
        "#]],
    );
}

#[test]
fn ternop_update_invalid_index() {
    check(
        "",
        "[1, 2, 3] w/ false <- 4",
        &expect![[r#"
            #1 0-23 "[1, 2, 3] w/ false <- 4" : Int[]
            #2 0-9 "[1, 2, 3]" : Int[]
            #3 1-2 "1" : Int
            #4 4-5 "2" : Int
            #5 7-8 "3" : Int
            #6 13-18 "false" : Bool
            #7 22-23 "4" : Int
            Error(Type(Error(MissingClassHasIndex("Int[]", "Bool", Span { lo: 0, hi: 23 }))))
        "#]],
    );
}

#[test]
fn ternop_update_array_index_var() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [2];
                    let i = 0;
                    let ys = xs w/ i <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-117 "{\n        let xs = [2];\n        let i = 0;\n        let ys = xs w/ i <- 3;\n    }" : Unit
            #10 52-54 "xs" : Int[]
            #12 57-60 "[2]" : Int[]
            #13 58-59 "2" : Int
            #15 74-75 "i" : Int
            #17 78-79 "0" : Int
            #19 93-95 "ys" : Int[]
            #21 98-110 "xs w/ i <- 3" : Int[]
            #22 98-100 "xs" : Int[]
            #25 104-105 "i" : Int
            #28 109-110 "3" : Int
        "#]],
    );
}

#[test]
fn ternop_update_array_index_expr() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [2];
                    let i = 0;
                    let ys = xs w/ i + 1 <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-121 "{\n        let xs = [2];\n        let i = 0;\n        let ys = xs w/ i + 1 <- 3;\n    }" : Unit
            #10 52-54 "xs" : Int[]
            #12 57-60 "[2]" : Int[]
            #13 58-59 "2" : Int
            #15 74-75 "i" : Int
            #17 78-79 "0" : Int
            #19 93-95 "ys" : Int[]
            #21 98-114 "xs w/ i + 1 <- 3" : Int[]
            #22 98-100 "xs" : Int[]
            #25 104-109 "i + 1" : Int
            #26 104-105 "i" : Int
            #29 108-109 "1" : Int
            #30 113-114 "3" : Int
        "#]],
    );
}

#[test]
fn ternop_update_udt_known_field_name() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ First <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #19 79-81 "()" : Unit
            #21 87-155 "{\n        let p = Pair(1, 2);\n        let q = p w/ First <- 3;\n    }" : Unit
            #23 101-102 "p" : UDT<"Pair": Item 1>
            #25 105-115 "Pair(1, 2)" : UDT<"Pair": Item 1>
            #26 105-109 "Pair" : ((Int, Int) -> UDT<"Pair": Item 1>)
            #29 109-115 "(1, 2)" : (Int, Int)
            #30 110-111 "1" : Int
            #31 113-114 "2" : Int
            #33 129-130 "q" : UDT<"Pair": Item 1>
            #35 133-148 "p w/ First <- 3" : UDT<"Pair": Item 1>
            #36 133-134 "p" : UDT<"Pair": Item 1>
            #39 138-143 "First" : ?
            #42 147-148 "3" : Int
        "#]],
    );
}

#[test]
fn ternop_update_udt_known_field_name_expr() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ First + 1 <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #19 79-81 "()" : Unit
            #21 87-159 "{\n        let p = Pair(1, 2);\n        let q = p w/ First + 1 <- 3;\n    }" : Unit
            #23 101-102 "p" : UDT<"Pair": Item 1>
            #25 105-115 "Pair(1, 2)" : UDT<"Pair": Item 1>
            #26 105-109 "Pair" : ((Int, Int) -> UDT<"Pair": Item 1>)
            #29 109-115 "(1, 2)" : (Int, Int)
            #30 110-111 "1" : Int
            #31 113-114 "2" : Int
            #33 129-130 "q" : UDT<"Pair": Item 1>
            #35 133-152 "p w/ First + 1 <- 3" : UDT<"Pair": Item 1>
            #36 133-134 "p" : UDT<"Pair": Item 1>
            #39 138-147 "First + 1" : ?
            #40 138-143 "First" : ?
            #43 146-147 "1" : Int
            #44 151-152 "3" : Int
            Error(Resolve(NotFound("First", Span { lo: 138, hi: 143 })))
        "#]],
    );
}

#[test]
fn ternop_update_udt_unknown_field_name() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ Third <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #19 79-81 "()" : Unit
            #21 87-155 "{\n        let p = Pair(1, 2);\n        let q = p w/ Third <- 3;\n    }" : Unit
            #23 101-102 "p" : UDT<"Pair": Item 1>
            #25 105-115 "Pair(1, 2)" : UDT<"Pair": Item 1>
            #26 105-109 "Pair" : ((Int, Int) -> UDT<"Pair": Item 1>)
            #29 109-115 "(1, 2)" : (Int, Int)
            #30 110-111 "1" : Int
            #31 113-114 "2" : Int
            #33 129-130 "q" : UDT<"Pair": Item 1>
            #35 133-148 "p w/ Third <- 3" : UDT<"Pair": Item 1>
            #36 133-134 "p" : UDT<"Pair": Item 1>
            #39 138-143 "Third" : ?
            #42 147-148 "3" : Int
            Error(Type(Error(MissingClassHasField("Pair", "Third", Span { lo: 133, hi: 148 }))))
        "#]],
    );
}

#[test]
fn ternop_update_udt_unknown_field_name_known_global() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Third() : () {}

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ Third <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #19 81-83 "()" : Unit
            #21 89-91 "{}" : Unit
            #25 109-111 "()" : Unit
            #27 117-185 "{\n        let p = Pair(1, 2);\n        let q = p w/ Third <- 3;\n    }" : Unit
            #29 131-132 "p" : UDT<"Pair": Item 1>
            #31 135-145 "Pair(1, 2)" : UDT<"Pair": Item 1>
            #32 135-139 "Pair" : ((Int, Int) -> UDT<"Pair": Item 1>)
            #35 139-145 "(1, 2)" : (Int, Int)
            #36 140-141 "1" : Int
            #37 143-144 "2" : Int
            #39 159-160 "q" : UDT<"Pair": Item 1>
            #41 163-178 "p w/ Third <- 3" : UDT<"Pair": Item 1>
            #42 163-164 "p" : UDT<"Pair": Item 1>
            #45 168-173 "Third" : ?
            #48 177-178 "3" : Int
            Error(Type(Error(MissingClassHasField("Pair", "Third", Span { lo: 163, hi: 178 }))))
        "#]],
    );
}

#[test]
fn ternop_update_array_range_takes_array() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [0, 1, 2];
                    let ys = xs w/ 0..1 <- [3, 4];
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-112 "{\n        let xs = [0, 1, 2];\n        let ys = xs w/ 0..1 <- [3, 4];\n    }" : Unit
            #10 52-54 "xs" : Int[]
            #12 57-66 "[0, 1, 2]" : Int[]
            #13 58-59 "0" : Int
            #14 61-62 "1" : Int
            #15 64-65 "2" : Int
            #17 80-82 "ys" : Int[]
            #19 85-105 "xs w/ 0..1 <- [3, 4]" : Int[]
            #20 85-87 "xs" : Int[]
            #23 91-95 "0..1" : Range
            #24 91-92 "0" : Int
            #25 94-95 "1" : Int
            #26 99-105 "[3, 4]" : Int[]
            #27 100-101 "3" : Int
            #28 103-104 "4" : Int
        "#]],
    );
}

#[test]
fn ternop_update_array_range_with_non_array_error() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [0, 1, 2];
                    let ys = xs w/ 0..1 <- 3;
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-107 "{\n        let xs = [0, 1, 2];\n        let ys = xs w/ 0..1 <- 3;\n    }" : Unit
            #10 52-54 "xs" : Int[]
            #12 57-66 "[0, 1, 2]" : Int[]
            #13 58-59 "0" : Int
            #14 61-62 "1" : Int
            #15 64-65 "2" : Int
            #17 80-82 "ys" : Int[]
            #19 85-100 "xs w/ 0..1 <- 3" : Int[]
            #20 85-87 "xs" : Int[]
            #23 91-95 "0..1" : Range
            #24 91-92 "0" : Int
            #25 94-95 "1" : Int
            #26 99-100 "3" : Int
            Error(Type(Error(TyMismatch("Int[]", "Int", Span { lo: 85, hi: 100 }))))
        "#]],
    );
}

#[test]
fn unop_bitwise_not_bool() {
    check(
        "",
        "~~~false",
        &expect![[r#"
            #1 0-8 "~~~false" : Bool
            #2 3-8 "false" : Bool
            Error(Type(Error(MissingClassInteger("Bool", Span { lo: 3, hi: 8 }))))
        "#]],
    );
}

#[test]
fn unop_bitwise_not_double() {
    check(
        "",
        "~~~2.0",
        &expect![[r#"
            #1 0-6 "~~~2.0" : Double
            #2 3-6 "2.0" : Double
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 3, hi: 6 }))))
        "#]],
    );
}

#[test]
fn unop_not_int() {
    check(
        "",
        "not 0",
        &expect![[r#"
            #1 0-5 "not 0" : Int
            #2 4-5 "0" : Int
            Error(Type(Error(TyMismatch("Bool", "Int", Span { lo: 4, hi: 5 }))))
        "#]],
    );
}

#[test]
fn unop_neg_bool() {
    check(
        "",
        "-false",
        &expect![[r##"
            #1 0-6 "-false" : Bool
            #2 1-6 "false" : Bool
            Error(Type(Error(MissingClassSigned("Bool", Span { lo: 1, hi: 6 }))))
        "##]],
    );
}

#[test]
fn unop_pos_bool() {
    check(
        "",
        "+false",
        &expect![[r##"
            #1 0-6 "+false" : Bool
            #2 1-6 "false" : Bool
            Error(Type(Error(MissingClassSigned("Bool", Span { lo: 1, hi: 6 }))))
        "##]],
    );
}

#[test]
fn while_cond_error() {
    check(
        "",
        "while Zero {}",
        &expect![[r#"
            #1 0-13 "while Zero {}" : Unit
            #2 6-10 "Zero" : Result
            #3 11-13 "{}" : Unit
            Error(Type(Error(TyMismatch("Bool", "Result", Span { lo: 6, hi: 10 }))))
        "#]],
    );
}

#[test]
fn while_body_should_be_unit_error() {
    check(
        "",
        "while true { 1 }",
        &expect![[r##"
            #1 0-16 "while true { 1 }" : Unit
            #2 6-10 "true" : Bool
            #3 11-16 "{ 1 }" : Int
            #5 13-14 "1" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 11, hi: 16 }))))
        "##]],
    );
}

#[test]
fn controlled_spec_impl() {
    check(
        indoc! {"
            namespace A {
                operation Foo(q : Qubit) : Unit is Ctl {
                    body ... {}
                    controlled (cs, ...) {}
                }
            }
        "},
        "",
        &expect![[r#"
            #6 31-42 "(q : Qubit)" : Qubit
            #7 32-41 "q : Qubit" : Qubit
            #17 72-75 "..." : Qubit
            #18 76-78 "{}" : Unit
            #20 98-107 "(cs, ...)" : (Qubit[], Qubit)
            #21 99-101 "cs" : Qubit[]
            #23 103-106 "..." : Qubit
            #24 108-110 "{}" : Unit
        "#]],
    );
}

#[test]
fn call_controlled() {
    check(
        indoc! {"
            namespace A {
                operation Foo(q : Qubit) : Unit is Ctl {
                    body ... {}
                    controlled (cs, ...) {}
                }
            }
        "},
        indoc! {"
            {
                use q1 = Qubit();
                use q2 = Qubit();
                Controlled A.Foo([q1], q2);
            }
        "},
        &expect![[r#"
            #6 31-42 "(q : Qubit)" : Qubit
            #7 32-41 "q : Qubit" : Qubit
            #17 72-75 "..." : Qubit
            #18 76-78 "{}" : Unit
            #20 98-107 "(cs, ...)" : (Qubit[], Qubit)
            #21 99-101 "cs" : Qubit[]
            #23 103-106 "..." : Qubit
            #24 108-110 "{}" : Unit
            #25 119-198 "{\n    use q1 = Qubit();\n    use q2 = Qubit();\n    Controlled A.Foo([q1], q2);\n}" : Unit
            #26 119-198 "{\n    use q1 = Qubit();\n    use q2 = Qubit();\n    Controlled A.Foo([q1], q2);\n}" : Unit
            #28 129-131 "q1" : Qubit
            #30 134-141 "Qubit()" : Qubit
            #32 151-153 "q2" : Qubit
            #34 156-163 "Qubit()" : Qubit
            #36 169-195 "Controlled A.Foo([q1], q2)" : Unit
            #37 169-185 "Controlled A.Foo" : ((Qubit[], Qubit) => Unit is Ctl)
            #38 180-185 "A.Foo" : (Qubit => Unit is Ctl)
            #42 185-195 "([q1], q2)" : (Qubit[], Qubit)
            #43 186-190 "[q1]" : Qubit[]
            #44 187-189 "q1" : Qubit
            #47 192-194 "q2" : Qubit
        "#]],
    );
}

#[test]
fn call_controlled_nested() {
    check(
        indoc! {"
            namespace A {
                operation Foo(q : Qubit) : Unit is Ctl {
                    body ... {}
                    controlled (cs, ...) {}
                }
            }
        "},
        indoc! {"
            {
                use q1 = Qubit();
                use q2 = Qubit();
                use q3 = Qubit();
                Controlled Controlled A.Foo([q1], ([q2], q3));
            }
        "},
        &expect![[r#"
            #6 31-42 "(q : Qubit)" : Qubit
            #7 32-41 "q : Qubit" : Qubit
            #17 72-75 "..." : Qubit
            #18 76-78 "{}" : Unit
            #20 98-107 "(cs, ...)" : (Qubit[], Qubit)
            #21 99-101 "cs" : Qubit[]
            #23 103-106 "..." : Qubit
            #24 108-110 "{}" : Unit
            #25 119-239 "{\n    use q1 = Qubit();\n    use q2 = Qubit();\n    use q3 = Qubit();\n    Controlled Controlled A.Foo([q1], ([q2], q3));\n}" : Unit
            #26 119-239 "{\n    use q1 = Qubit();\n    use q2 = Qubit();\n    use q3 = Qubit();\n    Controlled Controlled A.Foo([q1], ([q2], q3));\n}" : Unit
            #28 129-131 "q1" : Qubit
            #30 134-141 "Qubit()" : Qubit
            #32 151-153 "q2" : Qubit
            #34 156-163 "Qubit()" : Qubit
            #36 173-175 "q3" : Qubit
            #38 178-185 "Qubit()" : Qubit
            #40 191-236 "Controlled Controlled A.Foo([q1], ([q2], q3))" : Unit
            #41 191-218 "Controlled Controlled A.Foo" : ((Qubit[], (Qubit[], Qubit)) => Unit is Ctl)
            #42 202-218 "Controlled A.Foo" : ((Qubit[], Qubit) => Unit is Ctl)
            #43 213-218 "A.Foo" : (Qubit => Unit is Ctl)
            #47 218-236 "([q1], ([q2], q3))" : (Qubit[], (Qubit[], Qubit))
            #48 219-223 "[q1]" : Qubit[]
            #49 220-222 "q1" : Qubit
            #52 225-235 "([q2], q3)" : (Qubit[], Qubit)
            #53 226-230 "[q2]" : Qubit[]
            #54 227-229 "q2" : Qubit
            #57 232-234 "q3" : Qubit
        "#]],
    );
}

#[test]
fn call_controlled_error() {
    check(
        indoc! {"
            namespace A {
                operation Foo(q : Qubit) : Unit is Ctl {
                    body ... {}
                    controlled (cs, ...) {}
                }
            }
        "},
        indoc! {"
            {
                use q = Qubit();
                Controlled A.Foo([1], q);
            }
        "},
        &expect![[r#"
            #6 31-42 "(q : Qubit)" : Qubit
            #7 32-41 "q : Qubit" : Qubit
            #17 72-75 "..." : Qubit
            #18 76-78 "{}" : Unit
            #20 98-107 "(cs, ...)" : (Qubit[], Qubit)
            #21 99-101 "cs" : Qubit[]
            #23 103-106 "..." : Qubit
            #24 108-110 "{}" : Unit
            #25 119-173 "{\n    use q = Qubit();\n    Controlled A.Foo([1], q);\n}" : Unit
            #26 119-173 "{\n    use q = Qubit();\n    Controlled A.Foo([1], q);\n}" : Unit
            #28 129-130 "q" : Qubit
            #30 133-140 "Qubit()" : Qubit
            #32 146-170 "Controlled A.Foo([1], q)" : Unit
            #33 146-162 "Controlled A.Foo" : ((Qubit[], Qubit) => Unit is Ctl)
            #34 157-162 "A.Foo" : (Qubit => Unit is Ctl)
            #38 162-170 "([1], q)" : (Int[], Qubit)
            #39 163-166 "[1]" : Int[]
            #40 164-165 "1" : Int
            #41 168-169 "q" : Qubit
            Error(Type(Error(TyMismatch("Qubit", "Int", Span { lo: 146, hi: 170 }))))
        "#]],
    );
}

#[test]
fn adj_requires_unit_return() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : Int is Adj { 1 }
            }
        "},
        "",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #11 47-52 "{ 1 }" : Int
            #13 49-50 "1" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 36, hi: 39 }))))
        "#]],
    );
}

#[test]
fn ctl_requires_unit_return() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : Int is Ctl { 1 }
            }
        "},
        "",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #11 47-52 "{ 1 }" : Int
            #13 49-50 "1" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 36, hi: 39 }))))
        "#]],
    );
}

#[test]
fn adj_ctl_requires_unit_return() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : Int is Adj + Ctl { 1 }
            }
        "},
        "",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #13 53-58 "{ 1 }" : Int
            #15 55-56 "1" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 36, hi: 39 }))))
        "#]],
    );
}

#[test]
fn adj_non_adj() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : () is Ctl {}
            }
        "},
        "Adjoint A.Foo",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #9 46-48 "{}" : Unit
            #10 51-64 "Adjoint A.Foo" : (Unit => Unit is Ctl)
            #11 59-64 "A.Foo" : (Unit => Unit is Ctl)
            Error(Type(Error(MissingFunctor(Value(Adj), Value(Ctl), Span { lo: 59, hi: 64 }))))
        "#]],
    );
}

#[test]
fn ctl_non_ctl() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : () is Adj {}
            }
        "},
        "Controlled A.Foo",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #9 46-48 "{}" : Unit
            #10 51-67 "Controlled A.Foo" : ((Qubit[], Unit) => Unit is Adj)
            #11 62-67 "A.Foo" : (Unit => Unit is Adj)
            Error(Type(Error(MissingFunctor(Value(Ctl), Value(Adj), Span { lo: 62, hi: 67 }))))
        "#]],
    );
}

#[test]
fn fail_diverges() {
    check(
        "",
        indoc! {r#"
            if true {
                fail "true"
            } else {
                4
            }
        "#},
        &expect![[r#"
            #1 0-42 "if true {\n    fail \"true\"\n} else {\n    4\n}" : Int
            #2 3-7 "true" : Bool
            #3 8-27 "{\n    fail \"true\"\n}" : Int
            #5 14-25 "fail \"true\"" : Int
            #6 19-25 "\"true\"" : String
            #7 28-42 "else {\n    4\n}" : Int
            #8 33-42 "{\n    4\n}" : Int
            #10 39-40 "4" : Int
        "#]],
    );
}

#[test]
fn return_diverges() {
    check(
        indoc! {"
            namespace A {
                function Foo(x : Bool) : Int {
                    let x = if x {
                        return 1
                    } else {
                        true
                    };
                    2
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-40 "(x : Bool)" : Bool
            #7 31-39 "x : Bool" : Bool
            #15 47-153 "{\n        let x = if x {\n            return 1\n        } else {\n            true\n        };\n        2\n    }" : Int
            #17 61-62 "x" : Bool
            #19 65-136 "if x {\n            return 1\n        } else {\n            true\n        }" : Bool
            #20 68-69 "x" : Bool
            #23 70-102 "{\n            return 1\n        }" : Bool
            #25 84-92 "return 1" : Bool
            #26 91-92 "1" : Int
            #27 103-136 "else {\n            true\n        }" : Bool
            #28 108-136 "{\n            true\n        }" : Bool
            #30 122-126 "true" : Bool
            #32 146-147 "2" : Int
        "#]],
    );
}

#[test]
fn return_diverges_stmt_after() {
    check(
        indoc! {"
            namespace A {
                function Foo(x : Bool) : Int {
                    let x = {
                        return 1;
                        true
                    };
                    x
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-40 "(x : Bool)" : Bool
            #7 31-39 "x : Bool" : Bool
            #15 47-132 "{\n        let x = {\n            return 1;\n            true\n        };\n        x\n    }" : Int
            #17 61-62 "x" : Unit
            #19 65-115 "{\n            return 1;\n            true\n        }" : Unit
            #20 65-115 "{\n            return 1;\n            true\n        }" : Unit
            #22 79-87 "return 1" : Unit
            #23 86-87 "1" : Int
            #25 101-105 "true" : Bool
            #27 125-126 "x" : Unit
        "#]],
    );
}

#[test]
fn return_mismatch() {
    check(
        indoc! {"
            namespace A {
                function Foo(x : Bool) : Int {
                    return true;
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-40 "(x : Bool)" : Bool
            #7 31-39 "x : Bool" : Bool
            #15 47-75 "{\n        return true;\n    }" : Int
            #17 57-68 "return true" : Unit
            #18 64-68 "true" : Bool
            Error(Type(Error(TyMismatch("Int", "Bool", Span { lo: 64, hi: 68 }))))
        "#]],
    );
}

#[test]
fn return_with_satisfying_specialization_succeeds() {
    check(
        indoc! {"
            namespace test {
                operation E() : Unit {}
                operation A() : Unit is Adj {}
                operation C() : Unit is Ctl {}
                operation AC() : Unit is Adj + Ctl {}

                function returns_A_as_A() : (Unit => Unit is Adj) { A }
                function returns_AC_as_A() : (Unit => Unit is Adj) { AC }

                function returns_AC_as_C() : (Unit => Unit is Ctl) { AC }
                function returns_C_as_C() : (Unit => Unit is Ctl) { C }

                function returns_A_as_E() : (Unit => Unit) { A }
                function returns_AC_as_E() : (Unit => Unit) { AC }
                function returns_C_as_E() : (Unit => Unit) { C }
                function returns_E_as_E() : (Unit => Unit) { E }
            }
        "},
        "",
        &expect![[r#"
            #6 32-34 "()" : Unit
            #10 42-44 "{}" : Unit
            #14 60-62 "()" : Unit
            #19 77-79 "{}" : Unit
            #23 95-97 "()" : Unit
            #28 112-114 "{}" : Unit
            #32 131-133 "()" : Unit
            #39 154-156 "{}" : Unit
            #43 185-187 "()" : Unit
            #53 212-217 "{ A }" : (Unit => Unit is Adj)
            #55 214-215 "A" : (Unit => Unit is Adj)
            #61 246-248 "()" : Unit
            #71 273-279 "{ AC }" : (Unit => Unit is Adj + Ctl)
            #73 275-277 "AC" : (Unit => Unit is Adj + Ctl)
            #79 309-311 "()" : Unit
            #89 336-342 "{ AC }" : (Unit => Unit is Adj + Ctl)
            #91 338-340 "AC" : (Unit => Unit is Adj + Ctl)
            #97 370-372 "()" : Unit
            #107 397-402 "{ C }" : (Unit => Unit is Ctl)
            #109 399-400 "C" : (Unit => Unit is Ctl)
            #115 431-433 "()" : Unit
            #124 451-456 "{ A }" : (Unit => Unit is Adj)
            #126 453-454 "A" : (Unit => Unit is Adj)
            #132 485-487 "()" : Unit
            #141 505-511 "{ AC }" : (Unit => Unit is Adj + Ctl)
            #143 507-509 "AC" : (Unit => Unit is Adj + Ctl)
            #149 539-541 "()" : Unit
            #158 559-564 "{ C }" : (Unit => Unit is Ctl)
            #160 561-562 "C" : (Unit => Unit is Ctl)
            #166 592-594 "()" : Unit
            #175 612-617 "{ E }" : (Unit => Unit)
            #177 614-615 "E" : (Unit => Unit)
        "#]],
    );
}

#[test]
fn return_with_unsatisfying_specialization_fails() {
    check(
        indoc! {"
            namespace test {
                operation E() : Unit {}
                operation A() : Unit is Adj {}
                operation C() : Unit is Ctl {}
                operation AC() : Unit is Adj + Ctl {}

                function returns_E_as_A() : (Unit => Unit is Adj) { E }
                function returns_C_as_A() : (Unit => Unit is Adj) { C }

                function returns_E_as_C() : (Unit => Unit is Ctl) { E }
                function returns_A_as_C() : (Unit => Unit is Ctl) { A }

                function returns_E_as_AC() : (Unit => Unit is Adj + Ctl) { E }
                function returns_A_as_AC() : (Unit => Unit is Adj + Ctl) { A }
                function returns_C_as_AC() : (Unit => Unit is Adj + Ctl) { C }
            }
        "},
        "",
        &expect![[r#"
            #6 32-34 "()" : Unit
            #10 42-44 "{}" : Unit
            #14 60-62 "()" : Unit
            #19 77-79 "{}" : Unit
            #23 95-97 "()" : Unit
            #28 112-114 "{}" : Unit
            #32 131-133 "()" : Unit
            #39 154-156 "{}" : Unit
            #43 185-187 "()" : Unit
            #53 212-217 "{ E }" : (Unit => Unit)
            #55 214-215 "E" : (Unit => Unit)
            #61 245-247 "()" : Unit
            #71 272-277 "{ C }" : (Unit => Unit is Ctl)
            #73 274-275 "C" : (Unit => Unit is Ctl)
            #79 306-308 "()" : Unit
            #89 333-338 "{ E }" : (Unit => Unit)
            #91 335-336 "E" : (Unit => Unit)
            #97 366-368 "()" : Unit
            #107 393-398 "{ A }" : (Unit => Unit is Adj)
            #109 395-396 "A" : (Unit => Unit is Adj)
            #115 428-430 "()" : Unit
            #127 461-466 "{ E }" : (Unit => Unit)
            #129 463-464 "E" : (Unit => Unit)
            #135 495-497 "()" : Unit
            #147 528-533 "{ A }" : (Unit => Unit is Adj)
            #149 530-531 "A" : (Unit => Unit is Adj)
            #155 562-564 "()" : Unit
            #167 595-600 "{ C }" : (Unit => Unit is Ctl)
            #169 597-598 "C" : (Unit => Unit is Ctl)
            Error(Type(Error(FunctorMismatch(Value(Adj), Value(Empty), Span { lo: 214, hi: 215 }))))
            Error(Type(Error(FunctorMismatch(Value(Adj), Value(Ctl), Span { lo: 274, hi: 275 }))))
            Error(Type(Error(FunctorMismatch(Value(Ctl), Value(Empty), Span { lo: 335, hi: 336 }))))
            Error(Type(Error(FunctorMismatch(Value(Ctl), Value(Adj), Span { lo: 395, hi: 396 }))))
            Error(Type(Error(FunctorMismatch(Value(CtlAdj), Value(Empty), Span { lo: 463, hi: 464 }))))
            Error(Type(Error(FunctorMismatch(Value(CtlAdj), Value(Adj), Span { lo: 530, hi: 531 }))))
            Error(Type(Error(FunctorMismatch(Value(CtlAdj), Value(Ctl), Span { lo: 597, hi: 598 }))))
        "#]],
    );
}

#[test]
fn array_unknown_field_error() {
    check(
        indoc! {"
            namespace A {
                function Foo(x : Qubit[]) : Int {
                    x::Size
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-43 "(x : Qubit[])" : Qubit[]
            #7 31-42 "x : Qubit[]" : Qubit[]
            #16 50-73 "{\n        x::Size\n    }" : Int
            #18 60-67 "x::Size" : Int
            #19 60-61 "x" : Qubit[]
            Error(Type(Error(MissingClassHasField("Qubit[]", "Size", Span { lo: 60, hi: 67 }))))
        "#]],
    );
}

#[test]
fn range_fields_are_int() {
    check(
        indoc! {"
            namespace A {
                function Foo(r : Range) : (Int, Int, Int) {
                    (r::Start, r::Step, r::End)
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-41 "(r : Range)" : Range
            #7 31-40 "r : Range" : Range
            #22 60-103 "{\n        (r::Start, r::Step, r::End)\n    }" : (Int, Int, Int)
            #24 70-97 "(r::Start, r::Step, r::End)" : (Int, Int, Int)
            #25 71-79 "r::Start" : Int
            #26 71-72 "r" : Range
            #30 81-88 "r::Step" : Int
            #31 81-82 "r" : Range
            #35 90-96 "r::End" : Int
            #36 90-91 "r" : Range
        "#]],
    );
}

#[test]
fn range_to_field_start() {
    check(
        "",
        "(...2..8)::Start",
        &expect![[r#"
            #1 0-16 "(...2..8)::Start" : ?0
            #2 0-9 "(...2..8)" : RangeTo
            #3 1-8 "...2..8" : RangeTo
            #4 4-5 "2" : Int
            #5 7-8 "8" : Int
            Error(Type(Error(MissingClassHasField("RangeTo", "Start", Span { lo: 0, hi: 16 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 16 }))))
        "#]],
    );
}

#[test]
fn range_to_field_step() {
    check(
        "",
        "(...2..8)::Step",
        &expect![[r#"
            #1 0-15 "(...2..8)::Step" : Int
            #2 0-9 "(...2..8)" : RangeTo
            #3 1-8 "...2..8" : RangeTo
            #4 4-5 "2" : Int
            #5 7-8 "8" : Int
        "#]],
    );
}

#[test]
fn range_to_field_end() {
    check(
        "",
        "(...2..8)::End",
        &expect![[r#"
            #1 0-14 "(...2..8)::End" : Int
            #2 0-9 "(...2..8)" : RangeTo
            #3 1-8 "...2..8" : RangeTo
            #4 4-5 "2" : Int
            #5 7-8 "8" : Int
        "#]],
    );
}

#[test]
fn range_from_field_start() {
    check(
        "",
        "(0..2...)::Start",
        &expect![[r#"
            #1 0-16 "(0..2...)::Start" : Int
            #2 0-9 "(0..2...)" : RangeFrom
            #3 1-8 "0..2..." : RangeFrom
            #4 1-2 "0" : Int
            #5 4-5 "2" : Int
        "#]],
    );
}

#[test]
fn range_from_field_step() {
    check(
        "",
        "(0..2...)::Step",
        &expect![[r#"
            #1 0-15 "(0..2...)::Step" : Int
            #2 0-9 "(0..2...)" : RangeFrom
            #3 1-8 "0..2..." : RangeFrom
            #4 1-2 "0" : Int
            #5 4-5 "2" : Int
        "#]],
    );
}

#[test]
fn range_from_field_end() {
    check(
        "",
        "(0..2...)::End",
        &expect![[r#"
            #1 0-14 "(0..2...)::End" : ?0
            #2 0-9 "(0..2...)" : RangeFrom
            #3 1-8 "0..2..." : RangeFrom
            #4 1-2 "0" : Int
            #5 4-5 "2" : Int
            Error(Type(Error(MissingClassHasField("RangeFrom", "End", Span { lo: 0, hi: 14 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 14 }))))
        "#]],
    );
}

#[test]
fn range_full_field_start() {
    check(
        "",
        "...::Start",
        &expect![[r#"
            #1 0-10 "...::Start" : ?0
            #2 0-3 "..." : RangeFull
            Error(Type(Error(MissingClassHasField("RangeFull", "Start", Span { lo: 0, hi: 10 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 10 }))))
        "#]],
    );
}

#[test]
fn range_full_implicit_step() {
    check(
        "",
        "...::Step",
        &expect![[r#"
            #1 0-9 "...::Step" : Int
            #2 0-3 "..." : RangeFull
        "#]],
    );
}

#[test]
fn range_full_explicit_step() {
    check(
        "",
        "(...2...)::Step",
        &expect![[r#"
            #1 0-15 "(...2...)::Step" : Int
            #2 0-9 "(...2...)" : RangeFull
            #3 1-8 "...2..." : RangeFull
            #4 4-5 "2" : Int
        "#]],
    );
}

#[test]
fn range_full_field_end() {
    check(
        "",
        "...::End",
        &expect![[r#"
            #1 0-8 "...::End" : ?0
            #2 0-3 "..." : RangeFull
            Error(Type(Error(MissingClassHasField("RangeFull", "End", Span { lo: 0, hi: 8 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 8 }))))
        "#]],
    );
}

#[test]
fn interpolate_int() {
    check(
        "",
        r#"$"{4}""#,
        &expect![[r#"
            #1 0-6 "$\"{4}\"" : String
            #2 3-4 "4" : Int
        "#]],
    );
}

#[test]
fn interpolate_string() {
    check(
        "",
        r#"$"{"foo"}""#,
        &expect![[r#"
            #1 0-10 "$\"{\"foo\"}\"" : String
            #2 3-8 "\"foo\"" : String
        "#]],
    );
}

#[test]
fn interpolate_qubit() {
    check(
        "",
        r#"{ use q = Qubit(); $"{q}" }"#,
        &expect![[r#"
            #1 0-27 "{ use q = Qubit(); $\"{q}\" }" : String
            #2 0-27 "{ use q = Qubit(); $\"{q}\" }" : String
            #4 6-7 "q" : Qubit
            #6 10-17 "Qubit()" : Qubit
            #8 19-25 "$\"{q}\"" : String
            #9 22-23 "q" : Qubit
        "#]],
    );
}

#[test]
fn interpolate_function() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {}
            }
        "},
        r#"$"{A.Foo}""#,
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-40 "{}" : Unit
            #9 43-53 "$\"{A.Foo}\"" : String
            #10 46-51 "A.Foo" : (Unit -> Unit)
            Error(Type(Error(MissingClassShow("(Unit -> Unit)", Span { lo: 46, hi: 51 }))))
        "#]],
    );
}

#[test]
fn interpolate_operation() {
    check(
        indoc! {"
            namespace A {
                operation Foo() : () {}
            }
        "},
        r#"$"{A.Foo}""#,
        &expect![[r#"
            #6 31-33 "()" : Unit
            #8 39-41 "{}" : Unit
            #9 44-54 "$\"{A.Foo}\"" : String
            #10 47-52 "A.Foo" : (Unit => Unit)
            Error(Type(Error(MissingClassShow("(Unit => Unit)", Span { lo: 47, hi: 52 }))))
        "#]],
    );
}

#[test]
fn interpolate_int_array() {
    check(
        "",
        r#"$"{[1, 2, 3]}""#,
        &expect![[r#"
            #1 0-14 "$\"{[1, 2, 3]}\"" : String
            #2 3-12 "[1, 2, 3]" : Int[]
            #3 4-5 "1" : Int
            #4 7-8 "2" : Int
            #5 10-11 "3" : Int
        "#]],
    );
}

#[test]
fn interpolate_function_array() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {}
                function Bar() : () {}
            }
        "},
        r#"$"{[A.Foo, A.Bar]}""#,
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-40 "{}" : Unit
            #12 57-59 "()" : Unit
            #14 65-67 "{}" : Unit
            #15 70-89 "$\"{[A.Foo, A.Bar]}\"" : String
            #16 73-87 "[A.Foo, A.Bar]" : (Unit -> Unit)[]
            #17 74-79 "A.Foo" : (Unit -> Unit)
            #21 81-86 "A.Bar" : (Unit -> Unit)
            Error(Type(Error(MissingClassShow("(Unit -> Unit)", Span { lo: 73, hi: 87 }))))
        "#]],
    );
}

#[test]
fn interpolate_int_string_tuple() {
    check(
        "",
        r#"$"{(1, "foo")}""#,
        &expect![[r#"
            #1 0-15 "$\"{(1, \"foo\")}\"" : String
            #2 3-13 "(1, \"foo\")" : (Int, String)
            #3 4-5 "1" : Int
            #4 7-12 "\"foo\"" : String
        "#]],
    );
}

#[test]
fn interpolate_int_function_tuple() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {}
            }
        "},
        r#"$"{(1, A.Foo)}""#,
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-40 "{}" : Unit
            #9 43-58 "$\"{(1, A.Foo)}\"" : String
            #10 46-56 "(1, A.Foo)" : (Int, (Unit -> Unit))
            #11 47-48 "1" : Int
            #12 50-55 "A.Foo" : (Unit -> Unit)
            Error(Type(Error(MissingClassShow("(Unit -> Unit)", Span { lo: 46, hi: 56 }))))
        "#]],
    );
}

#[test]
fn newtype_cons() {
    check(
        indoc! {"
            namespace A {
                newtype NewInt = Int;
                function Foo() : NewInt { NewInt(5) }
            }
        "},
        "",
        &expect![[r#"
            #12 56-58 "()" : Unit
            #16 68-81 "{ NewInt(5) }" : UDT<"NewInt": Item 1>
            #18 70-79 "NewInt(5)" : UDT<"NewInt": Item 1>
            #19 70-76 "NewInt" : (Int -> UDT<"NewInt": Item 1>)
            #22 76-79 "(5)" : Int
            #23 77-78 "5" : Int
        "#]],
    );
}

#[test]
fn newtype_cons_wrong_input() {
    check(
        indoc! {"
            namespace A {
                newtype NewInt = Int;
                function Foo() : NewInt { NewInt(5.0) }
            }
        "},
        "",
        &expect![[r#"
            #12 56-58 "()" : Unit
            #16 68-83 "{ NewInt(5.0) }" : UDT<"NewInt": Item 1>
            #18 70-81 "NewInt(5.0)" : UDT<"NewInt": Item 1>
            #19 70-76 "NewInt" : (Int -> UDT<"NewInt": Item 1>)
            #22 76-81 "(5.0)" : Double
            #23 77-80 "5.0" : Double
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 70, hi: 81 }))))
        "#]],
    );
}

#[test]
fn struct_cons() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5, Second = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-124 "{ new Pair { First = 5, Second = 6 } }" : UDT<"Pair": Item 1>
            #25 88-122 "new Pair { First = 5, Second = 6 }" : UDT<"Pair": Item 1>
            #30 107-108 "5" : Int
            #33 119-120 "6" : Int
        "#]],
    );
}

#[test]
fn struct_cons_wrong_input() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5.0, Second = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-126 "{ new Pair { First = 5.0, Second = 6 } }" : UDT<"Pair": Item 1>
            #25 88-124 "new Pair { First = 5.0, Second = 6 }" : UDT<"Pair": Item 1>
            #30 107-110 "5.0" : Double
            #33 121-122 "6" : Int
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 99, hi: 110 }))))
        "#]],
    );
}

#[test]
fn struct_cons_wrong_field() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5, NotSecond = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-127 "{ new Pair { First = 5, NotSecond = 6 } }" : UDT<"Pair": Item 1>
            #25 88-125 "new Pair { First = 5, NotSecond = 6 }" : UDT<"Pair": Item 1>
            #30 107-108 "5" : Int
            #33 122-123 "6" : Int
            Error(Type(Error(MissingClassHasField("Pair", "NotSecond", Span { lo: 110, hi: 123 }))))
        "#]],
    );
}

#[test]
fn struct_cons_dup_field() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5, First = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-123 "{ new Pair { First = 5, First = 6 } }" : UDT<"Pair": Item 1>
            #25 88-121 "new Pair { First = 5, First = 6 }" : UDT<"Pair": Item 1>
            #30 107-108 "5" : Int
            #33 118-119 "6" : Int
            Error(Type(Error(DuplicateField("Pair", "First", Span { lo: 110, hi: 119 }))))
        "#]],
    );
}

#[test]
fn struct_cons_too_few_fields() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-112 "{ new Pair { First = 5 } }" : UDT<"Pair": Item 1>
            #25 88-110 "new Pair { First = 5 }" : UDT<"Pair": Item 1>
            #30 107-108 "5" : Int
            Error(Type(Error(MissingClassCorrectFieldCount("Pair", Span { lo: 88, hi: 110 }))))
        "#]],
    );
}

#[test]
fn struct_cons_too_many_fields() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair { new Pair { First = 5, Second = 6, Third = 7 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-135 "{ new Pair { First = 5, Second = 6, Third = 7 } }" : UDT<"Pair": Item 1>
            #25 88-133 "new Pair { First = 5, Second = 6, Third = 7 }" : UDT<"Pair": Item 1>
            #30 107-108 "5" : Int
            #33 119-120 "6" : Int
            #36 130-131 "7" : Int
            Error(Type(Error(MissingClassCorrectFieldCount("Pair", Span { lo: 88, hi: 133 }))))
            Error(Type(Error(MissingClassHasField("Pair", "Third", Span { lo: 122, hi: 131 }))))
        "#]],
    );
}

#[test]
fn struct_copy_cons() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair {
                    let pair = new Pair { First = 5, Second = 6 };
                    new Pair { ...pair }
                }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-177 "{\n        let pair = new Pair { First = 5, Second = 6 };\n        new Pair { ...pair }\n    }" : UDT<"Pair": Item 1>
            #25 100-104 "pair" : UDT<"Pair": Item 1>
            #27 107-141 "new Pair { First = 5, Second = 6 }" : UDT<"Pair": Item 1>
            #32 126-127 "5" : Int
            #35 138-139 "6" : Int
            #37 151-171 "new Pair { ...pair }" : UDT<"Pair": Item 1>
            #40 165-169 "pair" : UDT<"Pair": Item 1>
        "#]],
    );
}

#[test]
fn struct_copy_cons_with_fields() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair {
                    let pair = new Pair { First = 5, Second = 6 };
                    new Pair { ...pair, First = 7 }
                }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-188 "{\n        let pair = new Pair { First = 5, Second = 6 };\n        new Pair { ...pair, First = 7 }\n    }" : UDT<"Pair": Item 1>
            #25 100-104 "pair" : UDT<"Pair": Item 1>
            #27 107-141 "new Pair { First = 5, Second = 6 }" : UDT<"Pair": Item 1>
            #32 126-127 "5" : Int
            #35 138-139 "6" : Int
            #37 151-182 "new Pair { ...pair, First = 7 }" : UDT<"Pair": Item 1>
            #40 165-169 "pair" : UDT<"Pair": Item 1>
            #45 179-180 "7" : Int
        "#]],
    );
}

#[test]
fn struct_copy_cons_too_many_fields() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Foo() : Pair {
                    let pair = new Pair { First = 5, Second = 6 };
                    new Pair { ...pair, First = 7, Second = 8, Third = 9 }
                }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-211 "{\n        let pair = new Pair { First = 5, Second = 6 };\n        new Pair { ...pair, First = 7, Second = 8, Third = 9 }\n    }" : UDT<"Pair": Item 1>
            #25 100-104 "pair" : UDT<"Pair": Item 1>
            #27 107-141 "new Pair { First = 5, Second = 6 }" : UDT<"Pair": Item 1>
            #32 126-127 "5" : Int
            #35 138-139 "6" : Int
            #37 151-205 "new Pair { ...pair, First = 7, Second = 8, Third = 9 }" : UDT<"Pair": Item 1>
            #40 165-169 "pair" : UDT<"Pair": Item 1>
            #45 179-180 "7" : Int
            #48 191-192 "8" : Int
            #51 202-203 "9" : Int
            Error(Type(Error(MissingClassCorrectFieldCount("Pair", Span { lo: 151, hi: 205 }))))
            Error(Type(Error(MissingClassHasField("Pair", "Third", Span { lo: 194, hi: 203 }))))
        "#]],
    );
}

#[test]
fn struct_cons_udt_not_struct() {
    check(
        indoc! {"
            namespace A {
                newtype Triple = (Int, Int, Int);
                function Foo() : Triple { new Triple { First = 5, Second = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 68-70 "()" : Unit
            #23 80-120 "{ new Triple { First = 5, Second = 6 } }" : ?
            #25 82-118 "new Triple { First = 5, Second = 6 }" : ?
            #30 103-104 "5" : ?
            #33 115-116 "6" : ?
            Error(Type(Error(MissingClassStruct("Triple", Span { lo: 86, hi: 92 }))))
        "#]],
    );
}

#[test]
fn struct_cons_struct_like_udt() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);
                function Foo() : Pair { new Pair { First = 5, Second = 6 } }
            }
        "},
        "",
        &expect![[r##"
            #19 78-80 "()" : Unit
            #23 88-126 "{ new Pair { First = 5, Second = 6 } }" : UDT<"Pair": Item 1>
            #25 90-124 "new Pair { First = 5, Second = 6 }" : UDT<"Pair": Item 1>
            #30 109-110 "5" : Int
            #33 121-122 "6" : Int
        "##]],
    );
}

#[test]
fn struct_cons_ty_not_struct() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int { new Int { First = 5, Second = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-76 "{ new Int { First = 5, Second = 6 } }" : ?
            #12 41-74 "new Int { First = 5, Second = 6 }" : ?
            #17 59-60 "5" : ?
            #20 71-72 "6" : ?
            Error(Type(Error(MissingClassStruct("Int", Span { lo: 45, hi: 48 }))))
        "#]],
    );
}

#[test]
fn struct_cons_ident_not_struct() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    let q = 3;
                    new q { First = 5, Second = 6 }
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-105 "{\n        let q = 3;\n        new q { First = 5, Second = 6 }\n    }" : ?
            #12 53-54 "q" : Int
            #14 57-58 "3" : Int
            #16 68-99 "new q { First = 5, Second = 6 }" : ?
            #21 84-85 "5" : ?
            #24 96-97 "6" : ?
            Error(Resolve(NotFound("q", Span { lo: 72, hi: 73 })))
        "#]],
    );
}

#[test]
fn struct_cons_call_not_struct() {
    check(
        indoc! {"
            namespace A {
                struct Pair { First : Int, Second : Int }
                function Bar() : Pair { new Pair { First = 1, Second = 2 } }
                function Foo() : Pair { new Bar { First = 5, Second = 6 } }
            }
        "},
        "",
        &expect![[r#"
            #19 76-78 "()" : Unit
            #23 86-124 "{ new Pair { First = 1, Second = 2 } }" : UDT<"Pair": Item 1>
            #25 88-122 "new Pair { First = 1, Second = 2 }" : UDT<"Pair": Item 1>
            #30 107-108 "1" : Int
            #33 119-120 "2" : Int
            #37 141-143 "()" : Unit
            #41 151-188 "{ new Bar { First = 5, Second = 6 } }" : ?
            #43 153-186 "new Bar { First = 5, Second = 6 }" : ?
            #48 171-172 "5" : ?
            #51 183-184 "6" : ?
            Error(Resolve(NotFound("Bar", Span { lo: 157, hi: 160 })))
        "#]],
    );
}

#[test]
fn newtype_does_not_match_base_ty() {
    check(
        indoc! {"
            namespace A {
                newtype NewInt = Int;
                function Foo() : Int { NewInt(5) }
            }
        "},
        "",
        &expect![[r#"
            #12 56-58 "()" : Unit
            #16 65-78 "{ NewInt(5) }" : UDT<"NewInt": Item 1>
            #18 67-76 "NewInt(5)" : UDT<"NewInt": Item 1>
            #19 67-73 "NewInt" : (Int -> UDT<"NewInt": Item 1>)
            #22 73-76 "(5)" : Int
            #23 74-75 "5" : Int
            Error(Type(Error(TyMismatch("Int", "NewInt", Span { lo: 67, hi: 76 }))))
        "#]],
    );
}

#[test]
fn newtype_does_not_match_other_newtype() {
    check(
        indoc! {"
            namespace A {
                newtype NewInt1 = Int;
                newtype NewInt2 = Int;
                function Foo() : NewInt2 { NewInt1(5) }
            }
        "},
        "",
        &expect![[r#"
            #18 84-86 "()" : Unit
            #22 97-111 "{ NewInt1(5) }" : UDT<"NewInt1": Item 1>
            #24 99-109 "NewInt1(5)" : UDT<"NewInt1": Item 1>
            #25 99-106 "NewInt1" : (Int -> UDT<"NewInt1": Item 1>)
            #28 106-109 "(5)" : Int
            #29 107-108 "5" : Int
            Error(Type(Error(TyMismatch("NewInt2", "NewInt1", Span { lo: 99, hi: 109 }))))
        "#]],
    );
}

#[test]
fn newtype_unwrap() {
    check(
        indoc! {"
            namespace A {
                newtype Foo = (Int, Bool);
                function Bar(x : Foo) : () {
                    let y = x!;
                }
            }
        "},
        "",
        &expect![[r#"
            #16 61-70 "(x : Foo)" : UDT<"Foo": Item 1>
            #17 62-69 "x : Foo" : UDT<"Foo": Item 1>
            #23 76-103 "{\n        let y = x!;\n    }" : Unit
            #25 90-91 "y" : (Int, Bool)
            #27 94-96 "x!" : (Int, Bool)
            #28 94-95 "x" : UDT<"Foo": Item 1>
        "#]],
    );
}

#[test]
fn newtype_field() {
    check(
        indoc! {"
            namespace A {
                newtype Foo = Bar : Int;
                function Baz(x : Foo) : () {
                    let y = x::Bar;
                }
            }
        "},
        "",
        &expect![[r#"
            #13 59-68 "(x : Foo)" : UDT<"Foo": Item 1>
            #14 60-67 "x : Foo" : UDT<"Foo": Item 1>
            #20 74-105 "{\n        let y = x::Bar;\n    }" : Unit
            #22 88-89 "y" : Int
            #24 92-98 "x::Bar" : Int
            #25 92-93 "x" : UDT<"Foo": Item 1>
        "#]],
    );
}

#[test]
fn newtype_field_invalid() {
    check(
        indoc! {"
            namespace A {
                newtype Foo = Bar : Int;
                function Baz(x : Foo) : () {
                    let y = x::Nope;
                }
            }
        "},
        "",
        &expect![[r#"
            #13 59-68 "(x : Foo)" : UDT<"Foo": Item 1>
            #14 60-67 "x : Foo" : UDT<"Foo": Item 1>
            #20 74-106 "{\n        let y = x::Nope;\n    }" : Unit
            #22 88-89 "y" : ?1
            #24 92-99 "x::Nope" : ?1
            #25 92-93 "x" : UDT<"Foo": Item 1>
            Error(Type(Error(MissingClassHasField("Foo", "Nope", Span { lo: 92, hi: 99 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 92, hi: 99 }))))
        "#]],
    );
}

#[test]
fn struct_field_path() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C }
                struct C { i : Int }
                function Bar(x : A) : Unit {
                    let y = x.b.c.i;
                }
            }
        "},
        "",
        &expect![[r#"
            #30 103-110 "(x : A)" : UDT<"A": Item 1>
            #31 104-109 "x : A" : UDT<"A": Item 1>
            #39 118-150 "{\n        let y = x.b.c.i;\n    }" : Unit
            #41 132-133 "y" : Int
            #43 136-143 "x.b.c.i" : Int
            #45 136-137 "x" : UDT<"A": Item 1>
            #46 138-139 "b" : UDT<"B": Item 2>
            #47 140-141 "c" : UDT<"C": Item 3>
            #48 142-143 "i" : Int
        "#]],
    );
}

#[test]
fn struct_field_path_invalid() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C}
                struct C { i : Int }
                function Bar(x : A) : Unit {
                    let y = x.b.Nope.i;
                }
            }
        "},
        "",
        &expect![[r#"
            #30 102-109 "(x : A)" : UDT<"A": Item 1>
            #31 103-108 "x : A" : UDT<"A": Item 1>
            #39 117-152 "{\n        let y = x.b.Nope.i;\n    }" : Unit
            #41 131-132 "y" : ?3
            #43 135-145 "x.b.Nope.i" : ?3
            #45 135-136 "x" : UDT<"A": Item 1>
            #46 137-138 "b" : UDT<"B": Item 2>
            #47 139-143 "Nope" : ?2
            #48 144-145 "i" : ?3
            Error(Type(Error(MissingClassHasField("B", "Nope", Span { lo: 135, hi: 143 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 135, hi: 143 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 135, hi: 145 }))))
        "#]],
    );
}

#[test]
fn struct_field_path_with_expr() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C }
                struct C { i : Int }
                function Bar(x : A) : Unit {
                    let y = { x.b }.c.i;
                }
            }
        "},
        "",
        &expect![[r#"
            #30 103-110 "(x : A)" : UDT<"A": Item 1>
            #31 104-109 "x : A" : UDT<"A": Item 1>
            #39 118-154 "{\n        let y = { x.b }.c.i;\n    }" : Unit
            #41 132-133 "y" : Int
            #43 136-147 "{ x.b }.c.i" : Int
            #44 136-145 "{ x.b }.c" : UDT<"C": Item 3>
            #45 136-143 "{ x.b }" : UDT<"B": Item 2>
            #46 136-143 "{ x.b }" : UDT<"B": Item 2>
            #48 138-141 "x.b" : UDT<"B": Item 2>
            #50 138-139 "x" : UDT<"A": Item 1>
            #51 140-141 "b" : UDT<"B": Item 2>
        "#]],
    );
}

#[test]
fn struct_field_path_with_expr_invalid() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C}
                struct C { i : Int }
                function Bar(x : A) : Unit {
                    let y = { x }.b.Nope.i;
                }
            }
        "},
        "",
        &expect![[r#"
            #30 102-109 "(x : A)" : UDT<"A": Item 1>
            #31 103-108 "x : A" : UDT<"A": Item 1>
            #39 117-156 "{\n        let y = { x }.b.Nope.i;\n    }" : Unit
            #41 131-132 "y" : ?3
            #43 135-149 "{ x }.b.Nope.i" : ?3
            #44 135-147 "{ x }.b.Nope" : ?2
            #45 135-142 "{ x }.b" : UDT<"B": Item 2>
            #46 135-140 "{ x }" : UDT<"A": Item 1>
            #47 135-140 "{ x }" : UDT<"A": Item 1>
            #49 137-138 "x" : UDT<"A": Item 1>
            Error(Type(Error(MissingClassHasField("B", "Nope", Span { lo: 135, hi: 147 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 135, hi: 147 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 135, hi: 149 }))))
        "#]],
    );
}

#[test]
fn unknown_name_fits_any_ty() {
    check(
        "",
        "{ let x : Int = foo; let y : Qubit = foo; }",
        &expect![[r#"
            #1 0-43 "{ let x : Int = foo; let y : Qubit = foo; }" : Unit
            #2 0-43 "{ let x : Int = foo; let y : Qubit = foo; }" : Unit
            #4 6-13 "x : Int" : Int
            #9 16-19 "foo" : ?
            #13 25-34 "y : Qubit" : Qubit
            #18 37-40 "foo" : ?
            Error(Resolve(NotFound("foo", Span { lo: 16, hi: 19 })))
            Error(Resolve(NotFound("foo", Span { lo: 37, hi: 40 })))
        "#]],
    );
}

#[test]
fn unknown_name_has_any_class() {
    check(
        "",
        "{ foo(); foo + 1 }",
        &expect![[r#"
            #1 0-18 "{ foo(); foo + 1 }" : ?
            #2 0-18 "{ foo(); foo + 1 }" : ?
            #4 2-7 "foo()" : ?0
            #5 2-5 "foo" : ?
            #8 5-7 "()" : Unit
            #10 9-16 "foo + 1" : ?
            #11 9-12 "foo" : ?
            #14 15-16 "1" : Int
            Error(Resolve(NotFound("foo", Span { lo: 2, hi: 5 })))
            Error(Resolve(NotFound("foo", Span { lo: 9, hi: 12 })))
            Error(Type(Error(AmbiguousTy(Span { lo: 2, hi: 7 }))))
        "#]],
    );
}

#[test]
fn local_function() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    function Bar() : Int { 2 }
                    Bar() + 1
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-99 "{\n        function Bar() : Int { 2 }\n        Bar() + 1\n    }" : Int
            #15 61-63 "()" : Unit
            #19 70-75 "{ 2 }" : Int
            #21 72-73 "2" : Int
            #23 84-93 "Bar() + 1" : Int
            #24 84-89 "Bar()" : Int
            #25 84-87 "Bar" : (Unit -> Int)
            #28 87-89 "()" : Unit
            #29 92-93 "1" : Int
        "#]],
    );
}

#[test]
fn local_function_error() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    function Bar() : Int { 2.0 }
                    Bar()
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-97 "{\n        function Bar() : Int { 2.0 }\n        Bar()\n    }" : Int
            #15 61-63 "()" : Unit
            #19 70-77 "{ 2.0 }" : Double
            #21 72-75 "2.0" : Double
            #23 86-91 "Bar()" : Int
            #24 86-89 "Bar" : (Unit -> Int)
            #27 89-91 "()" : Unit
            Error(Type(Error(TyMismatch("Int", "Double", Span { lo: 72, hi: 75 }))))
        "#]],
    );
}

#[test]
fn local_function_use_before_declare() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    Bar();
                    function Bar() : () {}
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-91 "{\n        Bar();\n        function Bar() : () {}\n    }" : Unit
            #10 48-53 "Bar()" : Unit
            #11 48-51 "Bar" : (Unit -> Unit)
            #14 51-53 "()" : Unit
            #19 75-77 "()" : Unit
            #21 83-85 "{}" : Unit
        "#]],
    );
}

#[test]
fn local_function_last_stmt_is_unit_block() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    Bar();
                    function Bar() : Int { 4 }
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 39-96 "{\n        Bar();\n        function Bar() : Int { 4 }\n    }" : Unit
            #12 49-54 "Bar()" : Int
            #13 49-52 "Bar" : (Unit -> Int)
            #16 52-54 "()" : Unit
            #21 76-78 "()" : Unit
            #25 85-90 "{ 4 }" : Int
            #27 87-88 "4" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 35, hi: 38 }))))
        "#]],
    );
}

#[test]
fn local_type() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    newtype Bar = Int;
                    let x = Bar(5);
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-96 "{\n        newtype Bar = Int;\n        let x = Bar(5);\n    }" : Unit
            #17 79-80 "x" : UDT<"Bar": Item 2>
            #19 83-89 "Bar(5)" : UDT<"Bar": Item 2>
            #20 83-86 "Bar" : (Int -> UDT<"Bar": Item 2>)
            #23 86-89 "(5)" : Int
            #24 87-88 "5" : Int
        "#]],
    );
}

#[test]
fn local_open() {
    check(
        indoc! {"
            namespace A { function Foo() : () { open B; Bar(); } }
            namespace B { function Bar() : () {} }
        "},
        "",
        &expect![[r#"
            #6 26-28 "()" : Unit
            #8 34-52 "{ open B; Bar(); }" : Unit
            #14 44-49 "Bar()" : Unit
            #15 44-47 "Bar" : (Unit -> Unit)
            #18 47-49 "()" : Unit
            #24 81-83 "()" : Unit
            #26 89-91 "{}" : Unit
        "#]],
    );
}

#[test]
fn infinite() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let x = invalid;
                    let xs = [x, [x]];
                }
            }
        "},
        "",
        &expect![[r##"
            #6 30-32 "()" : Unit
            #8 38-97 "{\n        let x = invalid;\n        let xs = [x, [x]];\n    }" : Unit
            #10 52-53 "x" : ?0
            #12 56-63 "invalid" : ?
            #16 77-79 "xs" : ?0[]
            #18 82-90 "[x, [x]]" : ?0[]
            #19 83-84 "x" : ?0
            #22 86-89 "[x]" : ?0[]
            #23 87-88 "x" : ?0
            Error(Resolve(NotFound("invalid", Span { lo: 56, hi: 63 })))
            Error(Type(Error(RecursiveTypeConstraint(Span { lo: 86, hi: 89 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 52, hi: 53 }))))
        "##]],
    );
}

#[test]
fn lambda_inner_return() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {
                    let f = () -> {
                        return 42;
                    };
                    let r = f();
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-126 "{\n        let f = () -> {\n            return 42;\n        };\n        let r = f();\n    }" : Unit
            #12 54-55 "f" : (Unit -> Int)
            #14 58-98 "() -> {\n            return 42;\n        }" : (Unit -> Int)
            #15 58-60 "()" : Unit
            #16 64-98 "{\n            return 42;\n        }" : Int
            #17 64-98 "{\n            return 42;\n        }" : Int
            #19 78-87 "return 42" : Unit
            #20 85-87 "42" : Int
            #22 112-113 "r" : Int
            #24 116-119 "f()" : Int
            #25 116-117 "f" : (Unit -> Int)
            #28 117-119 "()" : Unit
        "#]],
    );
}

#[test]
fn lambda_inner_return_without_call_ambiguous() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {
                    let f = (a, b) -> {
                        return a + b;
                    };
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-112 "{\n        let f = (a, b) -> {\n            return a + b;\n        };\n    }" : Unit
            #12 54-55 "f" : ((?2, ?2) -> ?2)
            #14 58-105 "(a, b) -> {\n            return a + b;\n        }" : ((?2, ?2) -> ?2)
            #15 58-64 "(a, b)" : (?2, ?2)
            #16 59-60 "a" : ?2
            #18 62-63 "b" : ?2
            #20 68-105 "{\n            return a + b;\n        }" : ?2
            #21 68-105 "{\n            return a + b;\n        }" : ?2
            #23 82-94 "return a + b" : Unit
            #24 89-94 "a + b" : ?2
            #25 89-90 "a" : ?2
            #28 93-94 "b" : ?2
            Error(Type(Error(AmbiguousTy(Span { lo: 62, hi: 63 }))))
        "#]],
    );
}

#[test]
fn lambda_implicit_return_without_call_ambiguous() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {
                    let f = (a, b) -> {
                        a + b
                    };
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-104 "{\n        let f = (a, b) -> {\n            a + b\n        };\n    }" : Unit
            #12 54-55 "f" : ((?3, ?3) -> ?3)
            #14 58-97 "(a, b) -> {\n            a + b\n        }" : ((?3, ?3) -> ?3)
            #15 58-64 "(a, b)" : (?3, ?3)
            #16 59-60 "a" : ?3
            #18 62-63 "b" : ?3
            #20 68-97 "{\n            a + b\n        }" : ?3
            #21 68-97 "{\n            a + b\n        }" : ?3
            #23 82-87 "a + b" : ?3
            #24 82-83 "a" : ?3
            #27 86-87 "b" : ?3
            Error(Type(Error(AmbiguousTy(Span { lo: 68, hi: 97 }))))
        "#]],
    );
}

#[test]
fn lambda_implicit_unit_return() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {
                    let f = (a, b) -> {};
                    f(1, 2);
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #10 40-94 "{\n        let f = (a, b) -> {};\n        f(1, 2);\n    }" : Unit
            #12 54-55 "f" : ((Int, Int) -> Unit)
            #14 58-70 "(a, b) -> {}" : ((Int, Int) -> Unit)
            #15 58-64 "(a, b)" : (Int, Int)
            #16 59-60 "a" : Int
            #18 62-63 "b" : Int
            #20 68-70 "{}" : Unit
            #21 68-70 "{}" : Unit
            #23 80-87 "f(1, 2)" : Unit
            #24 80-81 "f" : ((Int, Int) -> Unit)
            #27 81-87 "(1, 2)" : (Int, Int)
            #28 82-83 "1" : Int
            #29 85-86 "2" : Int
        "#]],
    );
}

#[test]
fn lambda_adj() {
    check(
        indoc! {"
            namespace A {
                operation Foo(op : () => () is Adj) : () {}
                operation Bar() : () { Foo(() => ()) }
            }
        "},
        "",
        &expect![[r#"
            #6 31-53 "(op : () => () is Adj)" : (Unit => Unit is Adj)
            #7 32-52 "op : () => () is Adj" : (Unit => Unit is Adj)
            #14 59-61 "{}" : Unit
            #18 79-81 "()" : Unit
            #20 87-104 "{ Foo(() => ()) }" : Unit
            #22 89-102 "Foo(() => ())" : Unit
            #23 89-92 "Foo" : ((Unit => Unit is Adj) => Unit)
            #26 92-102 "(() => ())" : (Unit => Unit is Adj)
            #27 93-101 "() => ()" : (Unit => Unit is Adj)
            #28 93-95 "()" : Unit
            #29 99-101 "()" : Unit
        "#]],
    );
}

#[test]
fn lambda_ctl() {
    check(
        indoc! {"
            namespace A {
                operation Foo(op : () => () is Ctl) : () {}
                operation Bar() : () { Foo(() => ()) }
            }
        "},
        "",
        &expect![[r#"
            #6 31-53 "(op : () => () is Ctl)" : (Unit => Unit is Ctl)
            #7 32-52 "op : () => () is Ctl" : (Unit => Unit is Ctl)
            #14 59-61 "{}" : Unit
            #18 79-81 "()" : Unit
            #20 87-104 "{ Foo(() => ()) }" : Unit
            #22 89-102 "Foo(() => ())" : Unit
            #23 89-92 "Foo" : ((Unit => Unit is Ctl) => Unit)
            #26 92-102 "(() => ())" : (Unit => Unit is Ctl)
            #27 93-101 "() => ()" : (Unit => Unit is Ctl)
            #28 93-95 "()" : Unit
            #29 99-101 "()" : Unit
        "#]],
    );
}

#[test]
fn lambda_adj_ctl() {
    check(
        indoc! {"
            namespace A {
                operation Foo(op : () => () is Adj + Ctl) : () {}
                operation Bar() : () { Foo(() => ()) }
            }
        "},
        "",
        &expect![[r#"
            #6 31-59 "(op : () => () is Adj + Ctl)" : (Unit => Unit is Adj + Ctl)
            #7 32-58 "op : () => () is Adj + Ctl" : (Unit => Unit is Adj + Ctl)
            #16 65-67 "{}" : Unit
            #20 85-87 "()" : Unit
            #22 93-110 "{ Foo(() => ()) }" : Unit
            #24 95-108 "Foo(() => ())" : Unit
            #25 95-98 "Foo" : ((Unit => Unit is Adj + Ctl) => Unit)
            #28 98-108 "(() => ())" : (Unit => Unit is Adj + Ctl)
            #29 99-107 "() => ()" : (Unit => Unit is Adj + Ctl)
            #30 99-101 "()" : Unit
            #31 105-107 "()" : Unit
        "#]],
    );
}

#[test]
fn lambda_functors_let_binding() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let op : Qubit => Unit is Adj = q => ();
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #8 38-94 "{\n        let op : Qubit => Unit is Adj = q => ();\n    }" : Unit
            #10 52-77 "op : Qubit => Unit is Adj" : (Qubit => Unit is Adj)
            #20 80-87 "q => ()" : (Qubit => Unit is Adj)
            #21 80-81 "q" : Qubit
            #23 85-87 "()" : Unit
        "#]],
    );
}

#[test]
fn lambda_adjoint_before_functors_inferred() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Qubit => Unit is Adj {
                    let op = q => ();
                    Adjoint op
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #15 56-108 "{\n        let op = q => ();\n        Adjoint op\n    }" : (Qubit => Unit is Adj)
            #17 70-72 "op" : (Qubit => Unit is Adj)
            #19 75-82 "q => ()" : (Qubit => Unit is Adj)
            #20 75-76 "q" : Qubit
            #22 80-82 "()" : Unit
            #24 92-102 "Adjoint op" : (Qubit => Unit is Adj)
            #25 100-102 "op" : (Qubit => Unit is Adj)
        "#]],
    );
}

#[test]
fn lambda_invalid_adjoint_before_functors_inferred() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Qubit => Unit is Ctl {
                    let op = q => ();
                    Adjoint op
                }
            }
        "},
        "",
        &expect![[r#"
            #6 30-32 "()" : Unit
            #15 56-108 "{\n        let op = q => ();\n        Adjoint op\n    }" : (Qubit => Unit is Ctl)
            #17 70-72 "op" : (Qubit => Unit is Ctl)
            #19 75-82 "q => ()" : (Qubit => Unit is Ctl)
            #20 75-76 "q" : Qubit
            #22 80-82 "()" : Unit
            #24 92-102 "Adjoint op" : (Qubit => Unit is Ctl)
            #25 100-102 "op" : (Qubit => Unit is Ctl)
            Error(Type(Error(MissingFunctor(Value(Adj), Value(Ctl), Span { lo: 92, hi: 102 }))))
        "#]],
    );
}

#[test]
fn lambda_multiple_uses_functors_inferred() {
    check(
        indoc! {"
            namespace A {
                operation TakeAdj(op : Qubit => () is Adj) : () {}
                operation TakeAdjCtl(op : Qubit => () is Adj + Ctl) : () {}
                operation Foo() : () {
                    let op = q => ();
                    TakeAdj(op);
                    TakeAdjCtl(op);
                    let opCtl = Controlled op;
                }
            }
        "},
        "",
        &expect![[r#"
            #6 35-60 "(op : Qubit => () is Adj)" : (Qubit => Unit is Adj)
            #7 36-59 "op : Qubit => () is Adj" : (Qubit => Unit is Adj)
            #16 66-68 "{}" : Unit
            #20 93-124 "(op : Qubit => () is Adj + Ctl)" : (Qubit => Unit is Adj + Ctl)
            #21 94-123 "op : Qubit => () is Adj + Ctl" : (Qubit => Unit is Adj + Ctl)
            #32 130-132 "{}" : Unit
            #36 150-152 "()" : Unit
            #38 158-271 "{\n        let op = q => ();\n        TakeAdj(op);\n        TakeAdjCtl(op);\n        let opCtl = Controlled op;\n    }" : Unit
            #40 172-174 "op" : (Qubit => Unit is Adj + Ctl)
            #42 177-184 "q => ()" : (Qubit => Unit is Adj + Ctl)
            #43 177-178 "q" : Qubit
            #45 182-184 "()" : Unit
            #47 194-205 "TakeAdj(op)" : Unit
            #48 194-201 "TakeAdj" : ((Qubit => Unit is Adj + Ctl) => Unit)
            #51 201-205 "(op)" : (Qubit => Unit is Adj + Ctl)
            #52 202-204 "op" : (Qubit => Unit is Adj + Ctl)
            #56 215-229 "TakeAdjCtl(op)" : Unit
            #57 215-225 "TakeAdjCtl" : ((Qubit => Unit is Adj + Ctl) => Unit)
            #60 225-229 "(op)" : (Qubit => Unit is Adj + Ctl)
            #61 226-228 "op" : (Qubit => Unit is Adj + Ctl)
            #65 243-248 "opCtl" : ((Qubit[], Qubit) => Unit is Adj + Ctl)
            #67 251-264 "Controlled op" : ((Qubit[], Qubit) => Unit is Adj + Ctl)
            #68 262-264 "op" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn partial_app_one_hole() {
    check(
        "",
        "{
            function Foo(x : Int) : Int { x }
            let f = Foo(_);
        }",
        &expect![[r#"
            #1 0-85 "{\n            function Foo(x : Int) : Int { x }\n            let f = Foo(_);\n        }" : Unit
            #2 0-85 "{\n            function Foo(x : Int) : Int { x }\n            let f = Foo(_);\n        }" : Unit
            #7 26-35 "(x : Int)" : Int
            #8 27-34 "x : Int" : Int
            #16 42-47 "{ x }" : Int
            #18 44-45 "x" : Int
            #22 64-65 "f" : (Int -> Int)
            #24 68-74 "Foo(_)" : (Int -> Int)
            #25 68-71 "Foo" : (Int -> Int)
            #28 71-74 "(_)" : Int
            #29 72-73 "_" : Int
        "#]],
    );
}

#[test]
fn partial_app_one_given_one_hole() {
    check(
        "",
        indoc! {"{
            function Foo(x : Int, y : Int) : Int { x + y }
            let f = Foo(2, _);
        }"},
        &expect![[r#"
            #1 0-77 "{\n    function Foo(x : Int, y : Int) : Int { x + y }\n    let f = Foo(2, _);\n}" : Unit
            #2 0-77 "{\n    function Foo(x : Int, y : Int) : Int { x + y }\n    let f = Foo(2, _);\n}" : Unit
            #7 18-36 "(x : Int, y : Int)" : (Int, Int)
            #8 19-26 "x : Int" : Int
            #13 28-35 "y : Int" : Int
            #21 43-52 "{ x + y }" : Int
            #23 45-50 "x + y" : Int
            #24 45-46 "x" : Int
            #27 49-50 "y" : Int
            #31 61-62 "f" : (Int -> Int)
            #33 65-74 "Foo(2, _)" : (Int -> Int)
            #34 65-68 "Foo" : ((Int, Int) -> Int)
            #37 68-74 "(2, _)" : (Int, Int)
            #38 69-70 "2" : Int
            #39 72-73 "_" : Int
        "#]],
    );
}

#[test]
fn partial_app_two_holes() {
    check(
        "",
        indoc! {"{
            function Foo(x : Int, y : Int) : Int { x + y }
            let f = Foo(_, _);
        }"},
        &expect![[r#"
            #1 0-77 "{\n    function Foo(x : Int, y : Int) : Int { x + y }\n    let f = Foo(_, _);\n}" : Unit
            #2 0-77 "{\n    function Foo(x : Int, y : Int) : Int { x + y }\n    let f = Foo(_, _);\n}" : Unit
            #7 18-36 "(x : Int, y : Int)" : (Int, Int)
            #8 19-26 "x : Int" : Int
            #13 28-35 "y : Int" : Int
            #21 43-52 "{ x + y }" : Int
            #23 45-50 "x + y" : Int
            #24 45-46 "x" : Int
            #27 49-50 "y" : Int
            #31 61-62 "f" : ((Int, Int) -> Int)
            #33 65-74 "Foo(_, _)" : ((Int, Int) -> Int)
            #34 65-68 "Foo" : ((Int, Int) -> Int)
            #37 68-74 "(_, _)" : (Int, Int)
            #38 69-70 "_" : Int
            #39 72-73 "_" : Int
        "#]],
    );
}

#[test]
fn partial_app_nested_tuple() {
    check(
        "",
        indoc! {"{
            function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}
            let f = Foo(_, (_, 1.0, _), _);
        }"},
        &expect![[r#"
            #1 0-121 "{\n    function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}\n    let f = Foo(_, (_, 1.0, _), _);\n}" : Unit
            #2 0-121 "{\n    function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}\n    let f = Foo(_, (_, 1.0, _), _);\n}" : Unit
            #7 18-75 "(a : Int, (b : Bool, c : Double, d : String), e : Result)" : (Int, (Bool, Double, String), Result)
            #8 19-26 "a : Int" : Int
            #13 28-62 "(b : Bool, c : Double, d : String)" : (Bool, Double, String)
            #14 29-37 "b : Bool" : Bool
            #19 39-49 "c : Double" : Double
            #24 51-61 "d : String" : String
            #29 64-74 "e : Result" : Result
            #35 81-83 "{}" : Unit
            #37 92-93 "f" : ((Int, (Bool, String), Result) -> Unit)
            #39 96-118 "Foo(_, (_, 1.0, _), _)" : ((Int, (Bool, String), Result) -> Unit)
            #40 96-99 "Foo" : ((Int, (Bool, Double, String), Result) -> Unit)
            #43 99-118 "(_, (_, 1.0, _), _)" : (Int, (Bool, Double, String), Result)
            #44 100-101 "_" : Int
            #45 103-114 "(_, 1.0, _)" : (Bool, Double, String)
            #46 104-105 "_" : Bool
            #47 107-110 "1.0" : Double
            #48 112-113 "_" : String
            #49 116-117 "_" : Result
        "#]],
    );
}

#[test]
fn partial_app_nested_tuple_singleton_unwrap() {
    check(
        "",
        indoc! {"{
            function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}
            let f = Foo(_, (true, 1.0, _), _);
        }"},
        &expect![[r#"
            #1 0-124 "{\n    function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}\n    let f = Foo(_, (true, 1.0, _), _);\n}" : Unit
            #2 0-124 "{\n    function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}\n    let f = Foo(_, (true, 1.0, _), _);\n}" : Unit
            #7 18-75 "(a : Int, (b : Bool, c : Double, d : String), e : Result)" : (Int, (Bool, Double, String), Result)
            #8 19-26 "a : Int" : Int
            #13 28-62 "(b : Bool, c : Double, d : String)" : (Bool, Double, String)
            #14 29-37 "b : Bool" : Bool
            #19 39-49 "c : Double" : Double
            #24 51-61 "d : String" : String
            #29 64-74 "e : Result" : Result
            #35 81-83 "{}" : Unit
            #37 92-93 "f" : ((Int, String, Result) -> Unit)
            #39 96-121 "Foo(_, (true, 1.0, _), _)" : ((Int, String, Result) -> Unit)
            #40 96-99 "Foo" : ((Int, (Bool, Double, String), Result) -> Unit)
            #43 99-121 "(_, (true, 1.0, _), _)" : (Int, (Bool, Double, String), Result)
            #44 100-101 "_" : Int
            #45 103-117 "(true, 1.0, _)" : (Bool, Double, String)
            #46 104-108 "true" : Bool
            #47 110-113 "1.0" : Double
            #48 115-116 "_" : String
            #49 119-120 "_" : Result
        "#]],
    );
}

#[test]
fn partial_app_too_many_args() {
    check(
        "",
        indoc! {"{
            function Foo(x : Int) : Int { x }
            let f = Foo(1, _, _);
        }"},
        &expect![[r#"
            #1 0-67 "{\n    function Foo(x : Int) : Int { x }\n    let f = Foo(1, _, _);\n}" : Unit
            #2 0-67 "{\n    function Foo(x : Int) : Int { x }\n    let f = Foo(1, _, _);\n}" : Unit
            #7 18-27 "(x : Int)" : Int
            #8 19-26 "x : Int" : Int
            #16 34-39 "{ x }" : Int
            #18 36-37 "x" : Int
            #22 48-49 "f" : Int
            #24 52-64 "Foo(1, _, _)" : Int
            #25 52-55 "Foo" : (Int -> Int)
            #28 55-64 "(1, _, _)" : (Int, ?1, ?2)
            #29 56-57 "1" : Int
            #30 59-60 "_" : ?1
            #31 62-63 "_" : ?2
            Error(Type(Error(TyMismatch("Int", "(Int, ?, ?)", Span { lo: 52, hi: 64 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 59, hi: 60 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 62, hi: 63 }))))
        "#]],
    );
}

#[test]
fn typed_hole_error_concrete_type() {
    check(
        "",
        "_ + 3",
        &expect![[r#"
            #1 0-5 "_ + 3" : Int
            #2 0-1 "_" : Int
            #3 4-5 "3" : Int
            Error(Type(Error(TyHole("Int", Span { lo: 0, hi: 1 }))))
        "#]],
    );
}

#[test]
fn typed_hole_error_ambiguous_type() {
    check(
        "",
        "_(3)",
        &expect![[r#"
            #1 0-4 "_(3)" : ?1
            #2 0-1 "_" : ?0
            #3 1-4 "(3)" : Int
            #4 2-3 "3" : Int
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 1 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 0, hi: 4 }))))
            Error(Type(Error(TyHole("?", Span { lo: 0, hi: 1 }))))
        "#]],
    );
}

#[test]
fn functors_in_arg_superset_of_empty() {
    check(
        "",
        "{
            operation Foo(op : Qubit => ()) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-137 "{\n            operation Foo(op : Qubit => ()) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #2 0-137 "{\n            operation Foo(op : Qubit => ()) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #7 27-45 "(op : Qubit => ())" : (Qubit => Unit)
            #8 28-44 "op : Qubit => ()" : (Qubit => Unit)
            #16 51-53 "{}" : Unit
            #21 79-90 "(q : Qubit)" : Qubit
            #22 80-89 "q : Qubit" : Qubit
            #29 103-105 "{}" : Unit
            #31 118-126 "Foo(Bar)" : Unit
            #32 118-121 "Foo" : ((Qubit => Unit is Adj) => Unit)
            #35 121-126 "(Bar)" : (Qubit => Unit is Adj)
            #36 122-125 "Bar" : (Qubit => Unit is Adj)
        "#]],
    );
}

#[test]
fn functors_in_arg_superset_of_adj() {
    check(
        "",
        "{
            operation Foo(op : Qubit => () is Adj) : () {}
            operation Bar(q : Qubit) : () is Adj + Ctl {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-150 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj + Ctl {}\n            Foo(Bar);\n        }" : Unit
            #2 0-150 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj + Ctl {}\n            Foo(Bar);\n        }" : Unit
            #7 27-52 "(op : Qubit => () is Adj)" : (Qubit => Unit is Adj)
            #8 28-51 "op : Qubit => () is Adj" : (Qubit => Unit is Adj)
            #17 58-60 "{}" : Unit
            #22 86-97 "(q : Qubit)" : Qubit
            #23 87-96 "q : Qubit" : Qubit
            #32 116-118 "{}" : Unit
            #34 131-139 "Foo(Bar)" : Unit
            #35 131-134 "Foo" : ((Qubit => Unit is Adj + Ctl) => Unit)
            #38 134-139 "(Bar)" : (Qubit => Unit is Adj + Ctl)
            #39 135-138 "Bar" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn functors_in_arg_subset_of_ctl_adj() {
    check(
        "",
        "{
            operation Foo(op : Qubit => () is Adj + Ctl) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-150 "{\n            operation Foo(op : Qubit => () is Adj + Ctl) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #2 0-150 "{\n            operation Foo(op : Qubit => () is Adj + Ctl) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #7 27-58 "(op : Qubit => () is Adj + Ctl)" : (Qubit => Unit is Adj + Ctl)
            #8 28-57 "op : Qubit => () is Adj + Ctl" : (Qubit => Unit is Adj + Ctl)
            #19 64-66 "{}" : Unit
            #24 92-103 "(q : Qubit)" : Qubit
            #25 93-102 "q : Qubit" : Qubit
            #32 116-118 "{}" : Unit
            #34 131-139 "Foo(Bar)" : Unit
            #35 131-134 "Foo" : ((Qubit => Unit is Adj) => Unit)
            #38 134-139 "(Bar)" : (Qubit => Unit is Adj)
            #39 135-138 "Bar" : (Qubit => Unit is Adj)
            Error(Type(Error(MissingFunctor(Value(CtlAdj), Value(Adj), Span { lo: 131, hi: 139 }))))
        "#]],
    );
}

#[test]
fn functors_in_arg_eq_to_adj() {
    check(
        "",
        "{
            operation Foo(op : Qubit => () is Adj) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-144 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #2 0-144 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            Foo(Bar);\n        }" : Unit
            #7 27-52 "(op : Qubit => () is Adj)" : (Qubit => Unit is Adj)
            #8 28-51 "op : Qubit => () is Adj" : (Qubit => Unit is Adj)
            #17 58-60 "{}" : Unit
            #22 86-97 "(q : Qubit)" : Qubit
            #23 87-96 "q : Qubit" : Qubit
            #30 110-112 "{}" : Unit
            #32 125-133 "Foo(Bar)" : Unit
            #33 125-128 "Foo" : ((Qubit => Unit is Adj) => Unit)
            #36 128-133 "(Bar)" : (Qubit => Unit is Adj)
            #37 129-132 "Bar" : (Qubit => Unit is Adj)
        "#]],
    );
}

#[test]
fn functors_in_arg_nested_arrow_superset_of_adj() {
    check(
        "",
        "{
            operation Foo(op : (Qubit => () is Adj) => ()) : () {}
            operation Bar(op : Qubit => () is Adj + Ctl) : () {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-165 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => () is Adj + Ctl) : () {}\n            Foo(Bar);\n        }" : Unit
            #2 0-165 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => () is Adj + Ctl) : () {}\n            Foo(Bar);\n        }" : Unit
            #7 27-60 "(op : (Qubit => () is Adj) => ())" : ((Qubit => Unit is Adj) => Unit)
            #8 28-59 "op : (Qubit => () is Adj) => ()" : ((Qubit => Unit is Adj) => Unit)
            #20 66-68 "{}" : Unit
            #25 94-125 "(op : Qubit => () is Adj + Ctl)" : (Qubit => Unit is Adj + Ctl)
            #26 95-124 "op : Qubit => () is Adj + Ctl" : (Qubit => Unit is Adj + Ctl)
            #37 131-133 "{}" : Unit
            #39 146-154 "Foo(Bar)" : Unit
            #40 146-149 "Foo" : (((Qubit => Unit is Adj) => Unit) => Unit)
            #43 149-154 "(Bar)" : ((Qubit => Unit is Adj) => Unit)
            #44 150-153 "Bar" : ((Qubit => Unit is Adj) => Unit)
            Error(Type(Error(MissingFunctor(Value(CtlAdj), Value(Adj), Span { lo: 146, hi: 154 }))))
        "#]],
    );
}

#[test]
fn functors_in_arg_nested_arrow_subset_of_adj() {
    check(
        "",
        "{
            operation Foo(op : (Qubit => () is Adj) => ()) : () {}
            operation Bar(op : Qubit => ()) : () {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-152 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => ()) : () {}\n            Foo(Bar);\n        }" : Unit
            #2 0-152 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => ()) : () {}\n            Foo(Bar);\n        }" : Unit
            #7 27-60 "(op : (Qubit => () is Adj) => ())" : ((Qubit => Unit is Adj) => Unit)
            #8 28-59 "op : (Qubit => () is Adj) => ()" : ((Qubit => Unit is Adj) => Unit)
            #20 66-68 "{}" : Unit
            #25 94-112 "(op : Qubit => ())" : (Qubit => Unit)
            #26 95-111 "op : Qubit => ()" : (Qubit => Unit)
            #34 118-120 "{}" : Unit
            #36 133-141 "Foo(Bar)" : Unit
            #37 133-136 "Foo" : (((Qubit => Unit is Adj) => Unit) => Unit)
            #40 136-141 "(Bar)" : ((Qubit => Unit is Adj) => Unit)
            #41 137-140 "Bar" : ((Qubit => Unit is Adj) => Unit)
        "#]],
    );
}

#[test]
fn functors_in_arg_nested_arrow_eq_to_adj() {
    check(
        "",
        "{
            operation Foo(op : (Qubit => () is Adj) => ()) : () {}
            operation Bar(op : Qubit => () is Adj) : () {}
            Foo(Bar);
        }",
        &expect![[r#"
            #1 0-159 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => () is Adj) : () {}\n            Foo(Bar);\n        }" : Unit
            #2 0-159 "{\n            operation Foo(op : (Qubit => () is Adj) => ()) : () {}\n            operation Bar(op : Qubit => () is Adj) : () {}\n            Foo(Bar);\n        }" : Unit
            #7 27-60 "(op : (Qubit => () is Adj) => ())" : ((Qubit => Unit is Adj) => Unit)
            #8 28-59 "op : (Qubit => () is Adj) => ()" : ((Qubit => Unit is Adj) => Unit)
            #20 66-68 "{}" : Unit
            #25 94-119 "(op : Qubit => () is Adj)" : (Qubit => Unit is Adj)
            #26 95-118 "op : Qubit => () is Adj" : (Qubit => Unit is Adj)
            #35 125-127 "{}" : Unit
            #37 140-148 "Foo(Bar)" : Unit
            #38 140-143 "Foo" : (((Qubit => Unit is Adj) => Unit) => Unit)
            #41 143-148 "(Bar)" : ((Qubit => Unit is Adj) => Unit)
            #42 144-147 "Bar" : ((Qubit => Unit is Adj) => Unit)
        "#]],
    );
}

#[test]
fn functors_in_arg_array_superset_of_adj() {
    check(
        "",
        "{
            operation Foo(ops : (Qubit => () is Adj)[]) : () {}
            operation Bar(q : Qubit) : () is Adj + Ctl {}
            Foo([Bar]);
        }",
        &expect![[r#"
            #1 0-157 "{\n            operation Foo(ops : (Qubit => () is Adj)[]) : () {}\n            operation Bar(q : Qubit) : () is Adj + Ctl {}\n            Foo([Bar]);\n        }" : Unit
            #2 0-157 "{\n            operation Foo(ops : (Qubit => () is Adj)[]) : () {}\n            operation Bar(q : Qubit) : () is Adj + Ctl {}\n            Foo([Bar]);\n        }" : Unit
            #7 27-57 "(ops : (Qubit => () is Adj)[])" : (Qubit => Unit is Adj)[]
            #8 28-56 "ops : (Qubit => () is Adj)[]" : (Qubit => Unit is Adj)[]
            #19 63-65 "{}" : Unit
            #24 91-102 "(q : Qubit)" : Qubit
            #25 92-101 "q : Qubit" : Qubit
            #34 121-123 "{}" : Unit
            #36 136-146 "Foo([Bar])" : Unit
            #37 136-139 "Foo" : ((Qubit => Unit is Adj + Ctl)[] => Unit)
            #40 139-146 "([Bar])" : (Qubit => Unit is Adj + Ctl)[]
            #41 140-145 "[Bar]" : (Qubit => Unit is Adj + Ctl)[]
            #42 141-144 "Bar" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn functors_in_arg_array_subset_of_adj() {
    check(
        "",
        "{
            operation Foo(ops : (Qubit => () is Adj)[]) : () {}
            operation Bar(q : Qubit) : () {}
            Foo([Bar]);
        }",
        &expect![[r#"
            #1 0-144 "{\n            operation Foo(ops : (Qubit => () is Adj)[]) : () {}\n            operation Bar(q : Qubit) : () {}\n            Foo([Bar]);\n        }" : Unit
            #2 0-144 "{\n            operation Foo(ops : (Qubit => () is Adj)[]) : () {}\n            operation Bar(q : Qubit) : () {}\n            Foo([Bar]);\n        }" : Unit
            #7 27-57 "(ops : (Qubit => () is Adj)[])" : (Qubit => Unit is Adj)[]
            #8 28-56 "ops : (Qubit => () is Adj)[]" : (Qubit => Unit is Adj)[]
            #19 63-65 "{}" : Unit
            #24 91-102 "(q : Qubit)" : Qubit
            #25 92-101 "q : Qubit" : Qubit
            #31 108-110 "{}" : Unit
            #33 123-133 "Foo([Bar])" : Unit
            #34 123-126 "Foo" : ((Qubit => Unit)[] => Unit)
            #37 126-133 "([Bar])" : (Qubit => Unit)[]
            #38 127-132 "[Bar]" : (Qubit => Unit)[]
            #39 128-131 "Bar" : (Qubit => Unit)
            Error(Type(Error(MissingFunctor(Value(Adj), Value(Empty), Span { lo: 123, hi: 133 }))))
        "#]],
    );
}

#[test]
fn functors_in_array_all_same() {
    check(
        "",
        "{
            operation Foo(q : Qubit) : () is Adj {}
            let ops = [Foo, Foo, Foo];
        }",
        &expect![[r#"
            #1 0-102 "{\n            operation Foo(q : Qubit) : () is Adj {}\n            let ops = [Foo, Foo, Foo];\n        }" : Unit
            #2 0-102 "{\n            operation Foo(q : Qubit) : () is Adj {}\n            let ops = [Foo, Foo, Foo];\n        }" : Unit
            #7 27-38 "(q : Qubit)" : Qubit
            #8 28-37 "q : Qubit" : Qubit
            #15 51-53 "{}" : Unit
            #17 70-73 "ops" : (Qubit => Unit is Adj)[]
            #19 76-91 "[Foo, Foo, Foo]" : (Qubit => Unit is Adj)[]
            #20 77-80 "Foo" : (Qubit => Unit is Adj)
            #23 82-85 "Foo" : (Qubit => Unit is Adj)
            #26 87-90 "Foo" : (Qubit => Unit is Adj)
        "#]],
    );
}

#[test]
fn functors_in_array_mixed() {
    check(
        "",
        "{
            operation Foo(q : Qubit) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            operation Baz(q : Qubit) : () is Adj + Ctl {}
            let ops = [Foo, Bar, Baz];
        }",
        &expect![[r#"
            #1 0-205 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [Foo, Bar, Baz];\n        }" : Unit
            #2 0-205 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [Foo, Bar, Baz];\n        }" : Unit
            #7 27-38 "(q : Qubit)" : Qubit
            #8 28-37 "q : Qubit" : Qubit
            #14 44-46 "{}" : Unit
            #19 72-83 "(q : Qubit)" : Qubit
            #20 73-82 "q : Qubit" : Qubit
            #27 96-98 "{}" : Unit
            #32 124-135 "(q : Qubit)" : Qubit
            #33 125-134 "q : Qubit" : Qubit
            #42 154-156 "{}" : Unit
            #44 173-176 "ops" : (Qubit => Unit)[]
            #46 179-194 "[Foo, Bar, Baz]" : (Qubit => Unit)[]
            #47 180-183 "Foo" : (Qubit => Unit)
            #50 185-188 "Bar" : (Qubit => Unit is Adj)
            #53 190-193 "Baz" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn functors_in_array_mixed_lambda_all_empty() {
    check(
        "",
        "{
            operation Foo(q : Qubit) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            operation Baz(q : Qubit) : () is Adj + Ctl {}
            let ops = [Foo, q => Bar(q), q => Baz(q)];
        }",
        &expect![[r#"
            #1 0-221 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [Foo, q => Bar(q), q => Baz(q)];\n        }" : Unit
            #2 0-221 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [Foo, q => Bar(q), q => Baz(q)];\n        }" : Unit
            #7 27-38 "(q : Qubit)" : Qubit
            #8 28-37 "q : Qubit" : Qubit
            #14 44-46 "{}" : Unit
            #19 72-83 "(q : Qubit)" : Qubit
            #20 73-82 "q : Qubit" : Qubit
            #27 96-98 "{}" : Unit
            #32 124-135 "(q : Qubit)" : Qubit
            #33 125-134 "q : Qubit" : Qubit
            #42 154-156 "{}" : Unit
            #44 173-176 "ops" : (Qubit => Unit)[]
            #46 179-210 "[Foo, q => Bar(q), q => Baz(q)]" : (Qubit => Unit)[]
            #47 180-183 "Foo" : (Qubit => Unit)
            #50 185-196 "q => Bar(q)" : (Qubit => Unit)
            #51 185-186 "q" : Qubit
            #53 190-196 "Bar(q)" : Unit
            #54 190-193 "Bar" : (Qubit => Unit is Adj)
            #57 193-196 "(q)" : Qubit
            #58 194-195 "q" : Qubit
            #61 198-209 "q => Baz(q)" : (Qubit => Unit)
            #62 198-199 "q" : Qubit
            #64 203-209 "Baz(q)" : Unit
            #65 203-206 "Baz" : (Qubit => Unit is Adj + Ctl)
            #68 206-209 "(q)" : Qubit
            #69 207-208 "q" : Qubit
        "#]],
    );
}

#[test]
fn functors_in_array_mixed_lambda_all_ctl_adj() {
    check(
        "",
        "{
            operation Foo(q : Qubit) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            operation Baz(q : Qubit) : () is Adj + Ctl {}
            let ops = [q => Foo(q), q => Bar(q), Baz];
        }",
        &expect![[r#"
            #1 0-221 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [q => Foo(q), q => Bar(q), Baz];\n        }" : Unit
            #2 0-221 "{\n            operation Foo(q : Qubit) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let ops = [q => Foo(q), q => Bar(q), Baz];\n        }" : Unit
            #7 27-38 "(q : Qubit)" : Qubit
            #8 28-37 "q : Qubit" : Qubit
            #14 44-46 "{}" : Unit
            #19 72-83 "(q : Qubit)" : Qubit
            #20 73-82 "q : Qubit" : Qubit
            #27 96-98 "{}" : Unit
            #32 124-135 "(q : Qubit)" : Qubit
            #33 125-134 "q : Qubit" : Qubit
            #42 154-156 "{}" : Unit
            #44 173-176 "ops" : (Qubit => Unit is Adj + Ctl)[]
            #46 179-210 "[q => Foo(q), q => Bar(q), Baz]" : (Qubit => Unit is Adj + Ctl)[]
            #47 180-191 "q => Foo(q)" : (Qubit => Unit is Adj + Ctl)
            #48 180-181 "q" : Qubit
            #50 185-191 "Foo(q)" : Unit
            #51 185-188 "Foo" : (Qubit => Unit)
            #54 188-191 "(q)" : Qubit
            #55 189-190 "q" : Qubit
            #58 193-204 "q => Bar(q)" : (Qubit => Unit is Adj + Ctl)
            #59 193-194 "q" : Qubit
            #61 198-204 "Bar(q)" : Unit
            #62 198-201 "Bar" : (Qubit => Unit is Adj)
            #65 201-204 "(q)" : Qubit
            #66 202-203 "q" : Qubit
            #69 206-209 "Baz" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn functors_in_arg_bound_to_let_becomes_monotype() {
    check(
        "",
        "{
            operation Foo(op : Qubit => () is Adj) : () {}
            operation Bar(q : Qubit) : () is Adj {}
            operation Baz(q : Qubit) : () is Adj + Ctl {}
            let foo = Foo;
            foo(Bar);
            foo(Baz);
        }",
        &expect![[r#"
            #1 0-251 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let foo = Foo;\n            foo(Bar);\n            foo(Baz);\n        }" : Unit
            #2 0-251 "{\n            operation Foo(op : Qubit => () is Adj) : () {}\n            operation Bar(q : Qubit) : () is Adj {}\n            operation Baz(q : Qubit) : () is Adj + Ctl {}\n            let foo = Foo;\n            foo(Bar);\n            foo(Baz);\n        }" : Unit
            #7 27-52 "(op : Qubit => () is Adj)" : (Qubit => Unit is Adj)
            #8 28-51 "op : Qubit => () is Adj" : (Qubit => Unit is Adj)
            #17 58-60 "{}" : Unit
            #22 86-97 "(q : Qubit)" : Qubit
            #23 87-96 "q : Qubit" : Qubit
            #30 110-112 "{}" : Unit
            #35 138-149 "(q : Qubit)" : Qubit
            #36 139-148 "q : Qubit" : Qubit
            #45 168-170 "{}" : Unit
            #47 187-190 "foo" : ((Qubit => Unit is Adj) => Unit)
            #49 193-196 "Foo" : ((Qubit => Unit is Adj) => Unit)
            #53 210-218 "foo(Bar)" : Unit
            #54 210-213 "foo" : ((Qubit => Unit is Adj) => Unit)
            #57 213-218 "(Bar)" : (Qubit => Unit is Adj)
            #58 214-217 "Bar" : (Qubit => Unit is Adj)
            #62 232-240 "foo(Baz)" : Unit
            #63 232-235 "foo" : ((Qubit => Unit is Adj) => Unit)
            #66 235-240 "(Baz)" : (Qubit => Unit is Adj + Ctl)
            #67 236-239 "Baz" : (Qubit => Unit is Adj + Ctl)
        "#]],
    );
}

#[test]
fn duplicate_callable_decls_inferred_and_ignored() {
    check(
        indoc! {"
            namespace Test {
                function Foo() : Bool { true }
                function Foo() : Unit {}
                function Bar() : Unit {
                    let val = Foo();
                }
            }
        "},
        "",
        &expect![[r#"
            #6 33-35 "()" : Unit
            #10 43-51 "{ true }" : Bool
            #12 45-49 "true" : Bool
            #16 68-70 "()" : Unit
            #20 78-80 "{}" : Unit
            #24 97-99 "()" : Unit
            #28 107-139 "{\n        let val = Foo();\n    }" : Unit
            #30 121-124 "val" : Bool
            #32 127-132 "Foo()" : Bool
            #33 127-130 "Foo" : (Unit -> Bool)
            #36 130-132 "()" : Unit
            Error(Resolve(Duplicate("Foo", "Test", Span { lo: 65, hi: 68 })))
        "#]],
    );
}

#[test]
fn duplicate_type_decls_inferred_and_ignored() {
    check(
        indoc! {"
            namespace Test {
                newtype Foo = Bool;
                newtype Foo = Unit;
                function Bar() : Unit {
                    let val = Foo(true);
                }
            }
        "},
        "",
        &expect![[r#"
            #18 81-83 "()" : Unit
            #22 91-127 "{\n        let val = Foo(true);\n    }" : Unit
            #24 105-108 "val" : UDT<"Foo": Item 1>
            #26 111-120 "Foo(true)" : UDT<"Foo": Item 1>
            #27 111-114 "Foo" : (Bool -> UDT<"Foo": Item 1>)
            #30 114-120 "(true)" : Bool
            #31 115-119 "true" : Bool
            Error(Resolve(Duplicate("Foo", "Test", Span { lo: 53, hi: 56 })))
        "#]],
    );
}

#[test]
fn instantiate_duplicate_ty_param_names() {
    check(
        "namespace Test { function Foo<'T, 'T>() : () { let f = Foo; } }",
        "",
        &expect![[r#"
            #8 37-39 "()" : Unit
            #10 45-61 "{ let f = Foo; }" : Unit
            #12 51-52 "f" : (Unit -> Unit)
            #14 55-58 "Foo" : (Unit -> Unit)
            Error(Type(Error(AmbiguousTy(Span { lo: 55, hi: 58 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 55, hi: 58 }))))
        "#]],
    );
}
#[test]
fn ambiguous_generic() {
    check(
        "namespace Test {
            function Foo<'T>(x: 'T) : 'T { x }
            function Bar() : () { let x = Foo([]); }
        }",
        "",
        &expect![[r#"
            #7 45-52 "(x: 'T)" : Param<"'T": 0>
            #8 46-51 "x: 'T" : Param<"'T": 0>
            #14 58-63 "{ x }" : Param<"'T": 0>
            #16 60-61 "x" : Param<"'T": 0>
            #22 88-90 "()" : Unit
            #24 96-116 "{ let x = Foo([]); }" : Unit
            #26 102-103 "x" : ?2[]
            #28 106-113 "Foo([])" : ?2[]
            #29 106-109 "Foo" : (?2[] -> ?2[])
            #32 109-113 "([])" : ?2[]
            #33 110-112 "[]" : ?2[]
            Error(Type(Error(AmbiguousTy(Span { lo: 110, hi: 112 }))))
        "#]],
    );
}
#[test]
fn invalid_ident() {
    check(
        r#"namespace NS {
    function Foo() : () {
        let x : 'invalid = 0;
    }
}
        "#,
        "",
        &expect![[r#"
            #6 31-33 "()" : Unit
            #8 39-76 "{\n        let x : 'invalid = 0;\n    }" : Unit
            #10 53-65 "x : 'invalid" : ?
            #14 68-69 "0" : Int
            Error(Resolve(NotFound("'invalid", Span { lo: 57, hi: 65 })))
        "#]],
    );
}
#[test]
fn undeclared_generic_param() {
    check(
        r#"namespace c{operation y(g: 'U): Unit {} }"#,
        "",
        &expect![[r##"
            #6 23-30 "(g: 'U)" : ?
            #7 24-29 "g: 'U" : ?
            #14 37-39 "{}" : Unit
            Error(Resolve(NotFound("'U", Span { lo: 27, hi: 29 })))
            Error(Type(Error(MissingTy { span: Span { lo: 27, hi: 29 } })))
        "##]],
    );
}

#[test]
fn use_bound_item_in_another_bound_item() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {
                    function C() : Unit {
                        D();
                    }
                    function D() : Unit {}
                }
            }
        "},
        "",
        &expect![[r#"
            #6 28-30 "()" : Unit
            #10 38-133 "{\n        function C() : Unit {\n            D();\n        }\n        function D() : Unit {}\n    }" : Unit
            #15 58-60 "()" : Unit
            #19 68-96 "{\n            D();\n        }" : Unit
            #21 82-85 "D()" : Unit
            #22 82-83 "D" : (Unit -> Unit)
            #25 83-85 "()" : Unit
            #30 115-117 "()" : Unit
            #34 125-127 "{}" : Unit
        "#]],
    );
}

#[test]
fn inferred_generic_tuple_arguments_for_passed_callable() {
    check(
        indoc! {"
            namespace Test {
                function Apply<'T>(f : 'T -> Unit, arg : 'T) : Unit {
                    f(arg);
                }
                function Check(x : Int, y : Int) : Unit {}
                function Test() : Unit {
                    let x = (1, 2);
                    Apply(Check, x);
                    Apply(Check, (1, 2));
                }
            }
        "},
        "",
        &expect![[r#"
            #7 39-65 "(f : 'T -> Unit, arg : 'T)" : ((Param<"'T": 0> -> Unit), Param<"'T": 0>)
            #8 40-54 "f : 'T -> Unit" : (Param<"'T": 0> -> Unit)
            #16 56-64 "arg : 'T" : Param<"'T": 0>
            #23 73-96 "{\n        f(arg);\n    }" : Unit
            #25 83-89 "f(arg)" : Unit
            #26 83-84 "f" : (Param<"'T": 0> -> Unit)
            #29 84-89 "(arg)" : Param<"'T": 0>
            #30 85-88 "arg" : Param<"'T": 0>
            #36 115-133 "(x : Int, y : Int)" : (Int, Int)
            #37 116-123 "x : Int" : Int
            #42 125-132 "y : Int" : Int
            #50 141-143 "{}" : Unit
            #54 161-163 "()" : Unit
            #58 171-257 "{\n        let x = (1, 2);\n        Apply(Check, x);\n        Apply(Check, (1, 2));\n    }" : Unit
            #60 185-186 "x" : (Int, Int)
            #62 189-195 "(1, 2)" : (Int, Int)
            #63 190-191 "1" : Int
            #64 193-194 "2" : Int
            #66 205-220 "Apply(Check, x)" : Unit
            #67 205-210 "Apply" : ((((Int, Int) -> Unit), (Int, Int)) -> Unit)
            #70 210-220 "(Check, x)" : (((Int, Int) -> Unit), (Int, Int))
            #71 211-216 "Check" : ((Int, Int) -> Unit)
            #74 218-219 "x" : (Int, Int)
            #78 230-250 "Apply(Check, (1, 2))" : Unit
            #79 230-235 "Apply" : ((((Int, Int) -> Unit), (Int, Int)) -> Unit)
            #82 235-250 "(Check, (1, 2))" : (((Int, Int) -> Unit), (Int, Int))
            #83 236-241 "Check" : ((Int, Int) -> Unit)
            #86 243-249 "(1, 2)" : (Int, Int)
            #87 244-245 "1" : Int
            #88 247-248 "2" : Int
        "#]],
    );
}

#[test]
fn inference_infinite_recursion_should_fail() {
    // This creates an infinite recursion in the type inference algorithm, because it tries
    // to prove that `'U1[]` is equal to `'U1`. This should hit the recursion limit configured
    // in the solver.
    check(
        indoc! {"
            namespace Test{
                function A<'T1, 'U1> (x : ('T1 -> 'U1)) : 'U1[] {
                }
                function B<'T2, 'U2> (y : (('T2, 'U2) -> 'T2)) : 'T2 {
                }
                function Invalid() : Unit{
                    A and B
                }
            }
        "},
        "",
        &expect![[r##"
            #8 41-59 "(x : ('T1 -> 'U1))" : (Param<"'T1": 0> -> Param<"'U1": 1>)
            #9 42-58 "x : ('T1 -> 'U1)" : (Param<"'T1": 0> -> Param<"'U1": 1>)
            #20 68-75 "{\n    }" : Unit
            #26 101-126 "(y : (('T2, 'U2) -> 'T2))" : ((Param<"'T2": 0>, Param<"'U2": 1>) -> Param<"'T2": 0>)
            #27 102-125 "y : (('T2, 'U2) -> 'T2)" : ((Param<"'T2": 0>, Param<"'U2": 1>) -> Param<"'T2": 0>)
            #40 133-140 "{\n    }" : Unit
            #44 161-163 "()" : Unit
            #48 170-193 "{\n        A and B\n    }" : (((?2, ?3) -> ?2) -> ?2[])
            #50 180-187 "A and B" : (((?2, ?3) -> ?2) -> ?2[])
            #51 180-181 "A" : (((?2, ?3) -> ?2) -> ?2[])
            #54 186-187 "B" : (((?2, ?3) -> ?2) -> ?2)
            Error(Type(Error(TyMismatch("Unit", "'U1[]", Span { lo: 62, hi: 67 }))))
            Error(Type(Error(TyMismatch("Unit", "'T2", Span { lo: 129, hi: 132 }))))
            Error(Type(Error(RecursiveTypeConstraint(Span { lo: 186, hi: 187 }))))
            Error(Type(Error(TyMismatch("Bool", "(((?, ?) -> ?) -> ?[])", Span { lo: 180, hi: 181 }))))
            Error(Type(Error(TyMismatch("Unit", "(((?, ?) -> ?) -> ?[])", Span { lo: 180, hi: 187 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 186, hi: 187 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 186, hi: 187 }))))
        "##]],
    );
}

#[test]
fn use_range_field_names_in_udt() {
    check(
        indoc! {"
            namespace A {
                newtype B = (
                    Start : Int,
                    Step : Int,
                    End : Int
                );
                function C() : Unit {
                    let udt = B(1, 2, 3);
                    let start = udt::Start;
                    let step = udt::Step;
                    let end = udt::End;
                }
            }
        "},
        "",
        &expect![[r#"
            #24 112-114 "()" : Unit
            #28 122-249 "{\n        let udt = B(1, 2, 3);\n        let start = udt::Start;\n        let step = udt::Step;\n        let end = udt::End;\n    }" : Unit
            #30 136-139 "udt" : UDT<"B": Item 1>
            #32 142-152 "B(1, 2, 3)" : UDT<"B": Item 1>
            #33 142-143 "B" : ((Int, Int, Int) -> UDT<"B": Item 1>)
            #36 143-152 "(1, 2, 3)" : (Int, Int, Int)
            #37 144-145 "1" : Int
            #38 147-148 "2" : Int
            #39 150-151 "3" : Int
            #41 166-171 "start" : Int
            #43 174-184 "udt::Start" : Int
            #44 174-177 "udt" : UDT<"B": Item 1>
            #49 198-202 "step" : Int
            #51 205-214 "udt::Step" : Int
            #52 205-208 "udt" : UDT<"B": Item 1>
            #57 228-231 "end" : Int
            #59 234-242 "udt::End" : Int
            #60 234-237 "udt" : UDT<"B": Item 1>
        "#]],
    );
}

#[test]
fn lambda_on_array_where_item_used_in_call_should_be_inferred() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {
                    let f = qs => C(qs[0]);
                }
                operation C(q : Qubit) : Unit is Ctl + Adj { }
            }
        "},
        "",
        &expect![[r#"
            #6 28-30 "()" : Unit
            #10 38-77 "{\n        let f = qs => C(qs[0]);\n    }" : Unit
            #12 52-53 "f" : (Qubit[] => Unit)
            #14 56-70 "qs => C(qs[0])" : (Qubit[] => Unit)
            #15 56-58 "qs" : Qubit[]
            #17 62-70 "C(qs[0])" : Unit
            #18 62-63 "C" : (Qubit => Unit is Adj + Ctl)
            #21 63-70 "(qs[0])" : Qubit
            #22 64-69 "qs[0]" : Qubit
            #23 64-66 "qs" : Qubit[]
            #26 67-68 "0" : Int
            #30 93-104 "(q : Qubit)" : Qubit
            #31 94-103 "q : Qubit" : Qubit
            #42 125-128 "{ }" : Unit
        "#]],
    );
}

#[test]
fn within_apply_returns_type_from_apply_block() {
    check(
        "",
        "{ let x = within { } apply { 4 }; let y = x + 1; }",
        &expect![[r##"
            #1 0-50 "{ let x = within { } apply { 4 }; let y = x + 1; }" : Unit
            #2 0-50 "{ let x = within { } apply { 4 }; let y = x + 1; }" : Unit
            #4 6-7 "x" : Int
            #6 10-32 "within { } apply { 4 }" : Int
            #7 17-20 "{ }" : Unit
            #8 27-32 "{ 4 }" : Int
            #10 29-30 "4" : Int
            #12 38-39 "y" : Int
            #14 42-47 "x + 1" : Int
            #15 42-43 "x" : Int
            #18 46-47 "1" : Int
        "##]],
    );
}

#[test]
fn within_block_should_be_unit_error() {
    check(
        "",
        "within { 4 } apply { 0 }",
        &expect![[r##"
            #1 0-24 "within { 4 } apply { 0 }" : Int
            #2 7-12 "{ 4 }" : Int
            #4 9-10 "4" : Int
            #5 19-24 "{ 0 }" : Int
            #7 21-22 "0" : Int
            Error(Type(Error(TyMismatch("Unit", "Int", Span { lo: 7, hi: 12 }))))
        "##]],
    );
}

#[test]
fn path_field_access() {
    check(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    let b = new B { C = 5 };
                    b.C;
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-118 "{\n        let b = new B { C = 5 };\n        b.C;\n    }" : Unit
            #20 79-80 "b" : UDT<"B": Item 1>
            #22 83-98 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 95-96 "5" : Int
            #29 108-111 "b.C" : Int
            #31 108-109 "b" : UDT<"B": Item 1>
            #32 110-111 "C" : Int
        "##]],
    );
}

#[test]
fn field_access_chained() {
    check(
        indoc! {"
            namespace A {
                struct B { C : Int }
                struct D { E : B }
                function Foo() : Unit {
                    let d = new D { E = new B { C = 5 } };
                    d.E.C;
                }
            }
        "},
        "",
        &expect![[r##"
            #22 78-80 "()" : Unit
            #26 88-157 "{\n        let d = new D { E = new B { C = 5 } };\n        d.E.C;\n    }" : Unit
            #28 102-103 "d" : UDT<"D": Item 2>
            #30 106-135 "new D { E = new B { C = 5 } }" : UDT<"D": Item 2>
            #35 118-133 "new B { C = 5 }" : UDT<"B": Item 1>
            #40 130-131 "5" : Int
            #42 145-150 "d.E.C" : Int
            #44 145-146 "d" : UDT<"D": Item 2>
            #45 147-148 "E" : UDT<"B": Item 1>
            #46 149-150 "C" : Int
        "##]],
    );
}

#[test]
fn expr_field_access() {
    check(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    (new B { C = 5 }).C;
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-101 "{\n        (new B { C = 5 }).C;\n    }" : Unit
            #20 75-94 "(new B { C = 5 }).C" : Int
            #21 75-92 "(new B { C = 5 })" : UDT<"B": Item 1>
            #22 76-91 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 88-89 "5" : Int
        "##]],
    );
}

#[test]
fn expr_incomplete_field_access() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    (new B { C = 5 }).;
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-100 "{\n        (new B { C = 5 }).;\n    }" : Unit
            #20 75-93 "(new B { C = 5 })." : ?
            #21 75-92 "(new B { C = 5 })" : UDT<"B": Item 1>
            #22 76-91 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 88-89 "5" : Int
        "##]],
    );
}

#[test]
fn expr_incomplete_field_access_no_semi() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    (new B { C = 5 }).
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-99 "{\n        (new B { C = 5 }).\n    }" : ?
            #20 75-98 "(new B { C = 5 }).\n    " : ?
            #21 75-92 "(new B { C = 5 })" : UDT<"B": Item 1>
            #22 76-91 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 88-89 "5" : Int
        "##]],
    );
}

#[test]
fn path_incomplete_field_access() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    let b = new B { C = 5 };
                    b.;
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-117 "{\n        let b = new B { C = 5 };\n        b.;\n    }" : Unit
            #20 79-80 "b" : UDT<"B": Item 1>
            #22 83-98 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 95-96 "5" : Int
            #29 108-110 "b." : ?
            #30 108-109 "b" : UDT<"B": Item 1>
        "##]],
    );
}

#[test]
fn incomplete_field_access_chained() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                struct D { E : B }
                function Foo() : Unit {
                    let d = new D { E = new B { C = 5 } };
                    d.E.;
                }
            }
        "},
        "",
        &expect![[r##"
            #22 78-80 "()" : Unit
            #26 88-156 "{\n        let d = new D { E = new B { C = 5 } };\n        d.E.;\n    }" : Unit
            #28 102-103 "d" : UDT<"D": Item 2>
            #30 106-135 "new D { E = new B { C = 5 } }" : UDT<"D": Item 2>
            #35 118-133 "new B { C = 5 }" : UDT<"B": Item 1>
            #40 130-131 "5" : Int
            #42 145-149 "d.E." : ?
            #43 145-146 "d" : UDT<"D": Item 2>
            #44 147-148 "E" : UDT<"B": Item 1>
        "##]],
    );
}

#[test]
fn expr_field_access_chained() {
    check(
        indoc! {"
            namespace A {
                struct B { C : Int }
                struct D { E : B }
                function Foo() : Unit {
                    (new D { E = new B { C = 5 } }).E.C;
                }
            }
        "},
        "",
        &expect![[r##"
            #22 78-80 "()" : Unit
            #26 88-140 "{\n        (new D { E = new B { C = 5 } }).E.C;\n    }" : Unit
            #28 98-133 "(new D { E = new B { C = 5 } }).E.C" : Int
            #29 98-131 "(new D { E = new B { C = 5 } }).E" : UDT<"B": Item 1>
            #30 98-129 "(new D { E = new B { C = 5 } })" : UDT<"D": Item 2>
            #31 99-128 "new D { E = new B { C = 5 } }" : UDT<"D": Item 2>
            #36 111-126 "new B { C = 5 }" : UDT<"B": Item 1>
            #41 123-124 "5" : Int
        "##]],
    );
}

#[test]
fn incomplete_expr_field_access_chained() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                struct D { E : B }
                function Foo() : Unit {
                    (new D { E = new B { C = 5 } }).E.;
                }
            }
        "},
        "",
        &expect![[r##"
            #22 78-80 "()" : Unit
            #26 88-139 "{\n        (new D { E = new B { C = 5 } }).E.;\n    }" : Unit
            #28 98-132 "(new D { E = new B { C = 5 } }).E." : ?
            #29 98-131 "(new D { E = new B { C = 5 } }).E" : UDT<"B": Item 1>
            #30 98-129 "(new D { E = new B { C = 5 } })" : UDT<"D": Item 2>
            #31 99-128 "new D { E = new B { C = 5 } }" : UDT<"D": Item 2>
            #36 111-126 "new B { C = 5 }" : UDT<"B": Item 1>
            #41 123-124 "5" : Int
        "##]],
    );
}

#[test]
fn field_access_not_ident() {
    check_allow_parse_errors(
        indoc! {"
            namespace A {
                struct B { C : Int }
                function Foo() : Unit {
                    let b = new B { C = 5 };
                    b.
                    123;
                }
            }
        "},
        "",
        &expect![[r##"
            #14 55-57 "()" : Unit
            #18 65-129 "{\n        let b = new B { C = 5 };\n        b.\n        123;\n    }" : Unit
            #20 79-80 "b" : UDT<"B": Item 1>
            #22 83-98 "new B { C = 5 }" : UDT<"B": Item 1>
            #27 95-96 "5" : Int
            #29 108-119 "b.\n        " : ?
            #30 108-109 "b" : UDT<"B": Item 1>
            #32 119-122 "123" : Int
        "##]],
    );
}

#[test]
fn type_exceeding_size_limit_is_not_propaged_and_generates_error() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {
                    let tooBig : ((((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))) -> (((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))))[] = [];
                    let x = tooBig[0];
                }
            }
        "},
        "",
        &expect![[r##"
            #6 30-32 "()" : Unit
            #10 40-610 "{\n        let tooBig : ((((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))) -> (((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))))[] = [];\n        let x = tooBig[0];\n    }" : Unit
            #12 54-571 "tooBig : ((((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))) -> (((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ())))) -> ((((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))) -> (((() -> ()) -> (() -> ())) -> ((() -> ()) -> (() -> ()))))))[]" : ((((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))) -> (((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))))[]
            #205 574-576 "[]" : ?0[]
            #207 590-591 "x" : ?2
            #209 594-603 "tooBig[0]" : ?2
            #210 594-600 "tooBig" : ((((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))) -> (((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))))[]
            #213 601-602 "0" : Int
            Error(Type(Error(TySizeLimitExceeded("((((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))) -> (((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))))", Span { lo: 574, hi: 576 }))))
            Error(Type(Error(TySizeLimitExceeded("((((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))) -> (((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))))", Span { lo: 594, hi: 600 }))))
            Error(Type(Error(TySizeLimitExceeded("((((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))) -> (((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit)))) -> ((((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))) -> (((Unit -> Unit) -> (Unit -> Unit)) -> ((Unit -> Unit) -> (Unit -> Unit))))))", Span { lo: 594, hi: 603 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 574, hi: 576 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 594, hi: 603 }))))
            Error(Type(Error(AmbiguousTy(Span { lo: 594, hi: 600 }))))
        "##]],
    );
}

#[test]
fn complex_literal_imaginary_only() {
    check(
        "",
        "4.0i",
        &expect![[r##"
        #1 0-4 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
    "##]],
    );
}

#[test]
fn complex_literal_imaginary_only_negation() {
    check(
        "",
        "-4.0i",
        &expect![[r##"
            #1 0-5 "-4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 1-5 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_real_and_imaginary() {
    check(
        "",
        "3.0 + 4.0i",
        &expect![[r##"
            #1 0-10 "3.0 + 4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-3 "3.0" : Double
            #3 6-10 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_imaginary_and_real() {
    check(
        "",
        "4.0i + 3.0",
        &expect![[r##"
            #1 0-10 "4.0i + 3.0" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-4 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 7-10 "3.0" : Double
        "##]],
    );
}

#[test]
fn complex_literal_with_subtraction() {
    check(
        "",
        "3.0 - 4.0i",
        &expect![[r##"
            #1 0-10 "3.0 - 4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-3 "3.0" : Double
            #3 6-10 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_with_subtraction_imaginary_first() {
    check(
        "",
        "4.0i - 3.0",
        &expect![[r##"
            #1 0-10 "4.0i - 3.0" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-4 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 7-10 "3.0" : Double
        "##]],
    );
}

#[test]
fn complex_literal_with_negation() {
    check(
        "",
        "-3.0 + 4.0i",
        &expect![[r##"
            #1 0-11 "-3.0 + 4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-4 "-3.0" : Double
            #3 1-4 "3.0" : Double
            #4 7-11 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_with_two_real_parts_error() {
    check(
        "",
        "2.0 + 3.0 + 4.0i",
        &expect![[r##"
            #1 0-16 "2.0 + 3.0 + 4.0i" : Double
            #2 0-9 "2.0 + 3.0" : Double
            #3 0-3 "2.0" : Double
            #4 6-9 "3.0" : Double
            #5 12-16 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            Error(Type(Error(TyMismatch("Double", "Complex(Test)", Span { lo: 12, hi: 16 }))))
        "##]],
    );
}

#[test]
fn add_double_and_complex_literal_with_parens_error() {
    check(
        "",
        "2.0 + (3.0 + 4.0i)",
        &expect![[r##"
            #1 0-18 "2.0 + (3.0 + 4.0i)" : Double
            #2 0-3 "2.0" : Double
            #3 6-18 "(3.0 + 4.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 7-17 "3.0 + 4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #5 7-10 "3.0" : Double
            #6 13-17 "4.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            Error(Type(Error(TyMismatch("Double", "Complex(Test)", Span { lo: 6, hi: 18 }))))
        "##]],
    );
}

#[test]
fn add_complex_literals() {
    check(
        "",
        "(2.0 + 3.0i) + (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) + (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn subtract_complex_literals() {
    check(
        "",
        "(2.0 + 3.0i) - (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) - (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn multiply_complex_literals() {
    check(
        "",
        "(2.0 + 3.0i) * (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) * (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn divide_complex_literals() {
    check(
        "",
        "(2.0 + 3.0i) / (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) / (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn exponential_with_complex_literal() {
    check(
        "",
        "(2.0 + 3.0i) ^ (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) ^ (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_supports_eq() {
    check(
        "",
        "(2.0 + 3.0i) == (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-28 "(2.0 + 3.0i) == (4.0 + 5.0i)" : Bool
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 16-28 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 17-27 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 17-20 "4.0" : Double
            #9 23-27 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
        "##]],
    );
}

#[test]
fn complex_literal_does_not_support_ord() {
    check(
        "",
        "(2.0 + 3.0i) < (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) < (4.0 + 5.0i)" : Bool
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            Error(Type(Error(MissingClassOrd("Complex(Test)", Span { lo: 0, hi: 12 }))))
        "##]],
    );
}

#[test]
fn complex_literal_does_not_support_mod() {
    check(
        "",
        "(2.0 + 3.0i) % (4.0 + 5.0i)",
        &expect![[r##"
            #1 0-27 "(2.0 + 3.0i) % (4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #2 0-12 "(2.0 + 3.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #3 1-11 "2.0 + 3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #4 1-4 "2.0" : Double
            #5 7-11 "3.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #6 15-27 "(4.0 + 5.0i)" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #7 16-26 "4.0 + 5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            #8 16-19 "4.0" : Double
            #9 22-26 "5.0i" : UDT<"Complex(Test)": Item 3 (Package 0)>
            Error(Type(Error(MissingClassMod("Complex(Test)", Span { lo: 0, hi: 12 }))))
        "##]],
    );
}
