// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compile::{self, PackageStore, SourceMap, compile};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};

fn check_hir(input: &str, expect: &Expect) {
    let sources = SourceMap::new([("test".into(), input.into())], None);
    let unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    expect.assert_eq(&unit.package.to_string());
}

fn check_errors(input: &str, expect: &Expect) {
    let sources = SourceMap::new([("test".into(), input.into())], None);
    let unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );

    let lower_errors: Vec<_> = unit
        .errors
        .into_iter()
        .filter_map(try_into_lower_error)
        .collect();

    expect.assert_debug_eq(&lower_errors);
}

fn try_into_lower_error(error: compile::Error) -> Option<super::Error> {
    if let compile::ErrorKind::Lower(error) = error.0 {
        Some(error)
    } else {
        None
    }
}

#[test]
fn test_entrypoint_attr_allowed() {
    check_errors(
        indoc! {"
            namespace input {
                @EntryPoint()
                operation Foo() : Unit {
                    body ... {}
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_entrypoint_attr_wrong_args() {
    check_errors(
        indoc! {r#"
            namespace input {
                @EntryPoint("Bar")
                operation Foo() : Unit {
                    body ... {}
                }
            }
        "#},
        &expect![[r#"
            [
                InvalidAttrArgs(
                    "empty or profile name",
                    Span {
                        lo: 33,
                        hi: 40,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_target_profile_base_attr_allowed() {
    check_errors(
        indoc! {"
            namespace input {
                @Config(Base)
                operation Foo() : Unit {
                    body ... {}
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_target_profile_attr_wrong_args() {
    check_errors(
        indoc! {"
            namespace input {
                @Config(Bar)
                operation Foo() : Unit {
                    body ... {}
                }
            }
        "},
        &expect![[r#"
            [
                InvalidAttrArgs(
                    "runtime capability",
                    Span {
                        lo: 29,
                        hi: 34,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_unknown_attr() {
    check_errors(
        indoc! {"
            namespace input {
                @Bar()
                operation Foo() : Unit {
                    body ... {}
                }
            }
        "},
        &expect![[r#"
            [
                UnknownAttr(
                    "Bar",
                    Span {
                        lo: 23,
                        hi: 26,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn lift_local_function() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(x : Int) : Int {
                    function Bar(y : Int) : Int { y + 1 }
                    Bar(x + 2)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-120] (Public):
                    Namespace (Ident 23 [10-11] "A"): Item 1
                Item 1 [18-118] (Internal):
                    Parent: 0
                    Callable 0 [18-118] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [31-38] [Type Int]: Bind: Ident 3 [31-32] "x"
                        output: Int
                        functors: empty set
                        body: SpecDecl 4 [18-118]: Impl:
                            Block 5 [46-118] [Type Int]:
                                Stmt 6 [56-93]: Item: 2
                                Stmt 17 [102-112]: Expr: Expr 18 [102-112] [Type Int]: Call:
                                    Expr 19 [102-105] [Type (Int -> Int)]: Var: Item 2
                                    Expr 20 [106-111] [Type Int]: BinOp (Add):
                                        Expr 21 [106-107] [Type Int]: Var: Local 3
                                        Expr 22 [110-111] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [56-93] (Internal):
                    Parent: 1
                    Callable 7 [56-93] (function):
                        name: Ident 8 [65-68] "Bar"
                        input: Pat 9 [69-76] [Type Int]: Bind: Ident 10 [69-70] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 11 [56-93]: Impl:
                            Block 12 [84-93] [Type Int]:
                                Stmt 13 [86-91]: Expr: Expr 14 [86-91] [Type Int]: BinOp (Add):
                                    Expr 15 [86-87] [Type Int]: Var: Local 10
                                    Expr 16 [90-91] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_local_operation() {
    check_hir(
        indoc! {"
            namespace A {
                operation Foo() : Result {
                    operation Bar(q : Qubit) : Result { Zero }
                    use q = Qubit();
                    Bar(q)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-143] (Public):
                    Namespace (Ident 22 [10-11] "A"): Item 1
                Item 1 [18-141] (Internal):
                    Parent: 0
                    Callable 0 [18-141] (operation):
                        name: Ident 1 [28-31] "Foo"
                        input: Pat 2 [31-33] [Type Unit]: Unit
                        output: Result
                        functors: empty set
                        body: SpecDecl 3 [18-141]: Impl:
                            Block 4 [43-141] [Type Result]:
                                Stmt 5 [53-95]: Item: 2
                                Stmt 14 [104-120]: Qubit (Fresh)
                                    Pat 15 [108-109] [Type Qubit]: Bind: Ident 16 [108-109] "q"
                                    QubitInit 17 [112-119] [Type Qubit]: Single
                                Stmt 18 [129-135]: Expr: Expr 19 [129-135] [Type Result]: Call:
                                    Expr 20 [129-132] [Type (Qubit => Result)]: Var: Item 2
                                    Expr 21 [133-134] [Type Qubit]: Var: Local 16
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [53-95] (Internal):
                    Parent: 1
                    Callable 6 [53-95] (operation):
                        name: Ident 7 [63-66] "Bar"
                        input: Pat 8 [67-76] [Type Qubit]: Bind: Ident 9 [67-68] "q"
                        output: Result
                        functors: empty set
                        body: SpecDecl 10 [53-95]: Impl:
                            Block 11 [87-95] [Type Result]:
                                Stmt 12 [89-93]: Expr: Expr 13 [89-93] [Type Result]: Lit: Result(Zero)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_local_newtype() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    newtype Bar = Int;
                    let x = Bar(5);
                    x!
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-110] (Public):
                    Namespace (Ident 16 [10-11] "A"): Item 1
                Item 1 [18-108] (Internal):
                    Parent: 0
                    Callable 0 [18-108] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-32] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [18-108]: Impl:
                            Block 4 [39-108] [Type Int]:
                                Stmt 5 [49-67]: Item: 2
                                Stmt 7 [76-91]: Local (Immutable):
                                    Pat 8 [80-81] [Type UDT<"Bar": Item 2>]: Bind: Ident 9 [80-81] "x"
                                    Expr 10 [84-90] [Type UDT<"Bar": Item 2>]: Call:
                                        Expr 11 [84-87] [Type (Int -> UDT<"Bar": Item 2>)]: Var: Item 2
                                        Expr 12 [88-89] [Type Int]: Lit: Int(5)
                                Stmt 13 [100-102]: Expr: Expr 14 [100-102] [Type Int]: UnOp (Unwrap):
                                    Expr 15 [100-101] [Type UDT<"Bar": Item 2>]: Var: Local 9
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [49-67] (Internal):
                    Parent: 1
                    Type (Ident 6 [57-60] "Bar"): UDT [49-67]:
                        TyDef [63-66]: Field:
                            type: Int"#]],
    );
}

#[test]
fn lift_newtype() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = Int;
                operation Bar() : Unit {
                    let x = Foo(1);
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-97] (Public):
                    Namespace (Ident 12 [10-11] "A"): Item 1, Item 2
                Item 1 [18-36] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-36]:
                        TyDef [32-35]: Field:
                            type: Int
                Item 2 [41-95] (Internal):
                    Parent: 0
                    Callable 1 [41-95] (operation):
                        name: Ident 2 [51-54] "Bar"
                        input: Pat 3 [54-56] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [41-95]: Impl:
                            Block 5 [64-95] [Type Unit]:
                                Stmt 6 [74-89]: Local (Immutable):
                                    Pat 7 [78-79] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [78-79] "x"
                                    Expr 9 [82-88] [Type UDT<"Foo": Item 1>]: Call:
                                        Expr 10 [82-85] [Type (Int -> UDT<"Foo": Item 1>)]: Var: Item 1
                                        Expr 11 [86-87] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_newtype_tuple() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = (Int, Double);
                operation Bar() : Unit {
                    let x = Foo(1, 2.3);
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-112] (Public):
                    Namespace (Ident 14 [10-11] "A"): Item 1, Item 2
                Item 1 [18-46] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-46]:
                        TyDef [32-45]: Field:
                            type: (Int, Double)
                Item 2 [51-110] (Internal):
                    Parent: 0
                    Callable 1 [51-110] (operation):
                        name: Ident 2 [61-64] "Bar"
                        input: Pat 3 [64-66] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [51-110]: Impl:
                            Block 5 [74-110] [Type Unit]:
                                Stmt 6 [84-104]: Local (Immutable):
                                    Pat 7 [88-89] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [88-89] "x"
                                    Expr 9 [92-103] [Type UDT<"Foo": Item 1>]: Call:
                                        Expr 10 [92-95] [Type ((Int, Double) -> UDT<"Foo": Item 1>)]: Var: Item 1
                                        Expr 11 [95-103] [Type (Int, Double)]: Tuple:
                                            Expr 12 [96-97] [Type Int]: Lit: Int(1)
                                            Expr 13 [99-102] [Type Double]: Lit: Double(2.3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_newtype_tuple_fields() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = (a: Int, b: Double);
                operation Bar() : Unit {
                    let x = Foo(1, 2.3);
                    let y = x::b;
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-140] (Public):
                    Namespace (Ident 19 [10-11] "A"): Item 1, Item 2
                Item 1 [18-52] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-52]:
                        TyDef [32-51]: Tuple:
                            TyDef [33-39]: Field:
                                name: a [33-34]
                                type: Int
                            TyDef [41-50]: Field:
                                name: b [41-42]
                                type: Double
                Item 2 [57-138] (Internal):
                    Parent: 0
                    Callable 1 [57-138] (operation):
                        name: Ident 2 [67-70] "Bar"
                        input: Pat 3 [70-72] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [57-138]: Impl:
                            Block 5 [80-138] [Type Unit]:
                                Stmt 6 [90-110]: Local (Immutable):
                                    Pat 7 [94-95] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [94-95] "x"
                                    Expr 9 [98-109] [Type UDT<"Foo": Item 1>]: Call:
                                        Expr 10 [98-101] [Type ((Int, Double) -> UDT<"Foo": Item 1>)]: Var: Item 1
                                        Expr 11 [101-109] [Type (Int, Double)]: Tuple:
                                            Expr 12 [102-103] [Type Int]: Lit: Int(1)
                                            Expr 13 [105-108] [Type Double]: Lit: Double(2.3)
                                Stmt 14 [119-132]: Local (Immutable):
                                    Pat 15 [123-124] [Type Double]: Bind: Ident 16 [123-124] "y"
                                    Expr 17 [127-131] [Type Double]: Field:
                                        Expr 18 [127-128] [Type UDT<"Foo": Item 1>]: Var: Local 8
                                        Path(FieldPath { indices: [1] })
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_newtype_nested_tuple() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = (Int, (Double, Bool));
                operation Bar() : Unit {
                    let x = Foo(1, (2.3, true));
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-128] (Public):
                    Namespace (Ident 16 [10-11] "A"): Item 1, Item 2
                Item 1 [18-54] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-54]:
                        TyDef [32-53]: Field:
                            type: (Int, (Double, Bool))
                Item 2 [59-126] (Internal):
                    Parent: 0
                    Callable 1 [59-126] (operation):
                        name: Ident 2 [69-72] "Bar"
                        input: Pat 3 [72-74] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [59-126]: Impl:
                            Block 5 [82-126] [Type Unit]:
                                Stmt 6 [92-120]: Local (Immutable):
                                    Pat 7 [96-97] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [96-97] "x"
                                    Expr 9 [100-119] [Type UDT<"Foo": Item 1>]: Call:
                                        Expr 10 [100-103] [Type ((Int, (Double, Bool)) -> UDT<"Foo": Item 1>)]: Var: Item 1
                                        Expr 11 [103-119] [Type (Int, (Double, Bool))]: Tuple:
                                            Expr 12 [104-105] [Type Int]: Lit: Int(1)
                                            Expr 13 [107-118] [Type (Double, Bool)]: Tuple:
                                                Expr 14 [108-111] [Type Double]: Lit: Double(2.3)
                                                Expr 15 [113-117] [Type Bool]: Lit: Bool(true)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_newtype_nested_tuple_fields() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = (a: Int, (b: Double, c: Bool));
                operation Bar() : Unit {
                    let x = Foo(1, (2.3, true));
                    let y = x::c;
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-159] (Public):
                    Namespace (Ident 21 [10-11] "A"): Item 1, Item 2
                Item 1 [18-63] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-63]:
                        TyDef [32-62]: Tuple:
                            TyDef [33-39]: Field:
                                name: a [33-34]
                                type: Int
                            TyDef [41-61]: Tuple:
                                TyDef [42-51]: Field:
                                    name: b [42-43]
                                    type: Double
                                TyDef [53-60]: Field:
                                    name: c [53-54]
                                    type: Bool
                Item 2 [68-157] (Internal):
                    Parent: 0
                    Callable 1 [68-157] (operation):
                        name: Ident 2 [78-81] "Bar"
                        input: Pat 3 [81-83] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [68-157]: Impl:
                            Block 5 [91-157] [Type Unit]:
                                Stmt 6 [101-129]: Local (Immutable):
                                    Pat 7 [105-106] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [105-106] "x"
                                    Expr 9 [109-128] [Type UDT<"Foo": Item 1>]: Call:
                                        Expr 10 [109-112] [Type ((Int, (Double, Bool)) -> UDT<"Foo": Item 1>)]: Var: Item 1
                                        Expr 11 [112-128] [Type (Int, (Double, Bool))]: Tuple:
                                            Expr 12 [113-114] [Type Int]: Lit: Int(1)
                                            Expr 13 [116-127] [Type (Double, Bool)]: Tuple:
                                                Expr 14 [117-120] [Type Double]: Lit: Double(2.3)
                                                Expr 15 [122-126] [Type Bool]: Lit: Bool(true)
                                Stmt 16 [138-151]: Local (Immutable):
                                    Pat 17 [142-143] [Type Bool]: Bind: Ident 18 [142-143] "y"
                                    Expr 19 [146-150] [Type Bool]: Field:
                                        Expr 20 [146-147] [Type UDT<"Foo": Item 1>]: Var: Local 8
                                        Path(FieldPath { indices: [1, 1] })
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lift_newtype_from_newtype() {
    check_hir(
        indoc! {"
            namespace A {
                newtype Foo = (a: Int, (b: Double, c: Bool));
                newtype Bar = (x: Int, y: Foo);
                operation Baz() : Unit {
                    let x = Bar(1, Foo(2, (3.4, false)));
                    let y = x::y::c;
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-207] (Public):
                    Namespace (Ident 27 [10-11] "A"): Item 1, Item 2, Item 3
                Item 1 [18-63] (Internal):
                    Parent: 0
                    Type (Ident 0 [26-29] "Foo"): UDT [18-63]:
                        TyDef [32-62]: Tuple:
                            TyDef [33-39]: Field:
                                name: a [33-34]
                                type: Int
                            TyDef [41-61]: Tuple:
                                TyDef [42-51]: Field:
                                    name: b [42-43]
                                    type: Double
                                TyDef [53-60]: Field:
                                    name: c [53-54]
                                    type: Bool
                Item 2 [68-99] (Internal):
                    Parent: 0
                    Type (Ident 1 [76-79] "Bar"): UDT [68-99]:
                        TyDef [82-98]: Tuple:
                            TyDef [83-89]: Field:
                                name: x [83-84]
                                type: Int
                            TyDef [91-97]: Field:
                                name: y [91-92]
                                type: UDT<"Foo": Item 1>
                Item 3 [104-205] (Internal):
                    Parent: 0
                    Callable 2 [104-205] (operation):
                        name: Ident 3 [114-117] "Baz"
                        input: Pat 4 [117-119] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 5 [104-205]: Impl:
                            Block 6 [127-205] [Type Unit]:
                                Stmt 7 [137-174]: Local (Immutable):
                                    Pat 8 [141-142] [Type UDT<"Bar": Item 2>]: Bind: Ident 9 [141-142] "x"
                                    Expr 10 [145-173] [Type UDT<"Bar": Item 2>]: Call:
                                        Expr 11 [145-148] [Type ((Int, UDT<"Foo": Item 1>) -> UDT<"Bar": Item 2>)]: Var: Item 2
                                        Expr 12 [148-173] [Type (Int, UDT<"Foo": Item 1>)]: Tuple:
                                            Expr 13 [149-150] [Type Int]: Lit: Int(1)
                                            Expr 14 [152-172] [Type UDT<"Foo": Item 1>]: Call:
                                                Expr 15 [152-155] [Type ((Int, (Double, Bool)) -> UDT<"Foo": Item 1>)]: Var: Item 1
                                                Expr 16 [155-172] [Type (Int, (Double, Bool))]: Tuple:
                                                    Expr 17 [156-157] [Type Int]: Lit: Int(2)
                                                    Expr 18 [159-171] [Type (Double, Bool)]: Tuple:
                                                        Expr 19 [160-163] [Type Double]: Lit: Double(3.4)
                                                        Expr 20 [165-170] [Type Bool]: Lit: Bool(false)
                                Stmt 21 [183-199]: Local (Immutable):
                                    Pat 22 [187-188] [Type Bool]: Bind: Ident 23 [187-188] "y"
                                    Expr 24 [191-198] [Type Bool]: Field:
                                        Expr 25 [191-195] [Type UDT<"Foo": Item 1>]: Field:
                                            Expr 26 [191-192] [Type UDT<"Bar": Item 2>]: Var: Local 9
                                            Path(FieldPath { indices: [1] })
                                        Path(FieldPath { indices: [1, 1] })
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lower_struct_decl() {
    check_hir(
        indoc! {"
            namespace A {
                struct Foo {
                    x: Int,
                    y: Double,
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-73] (Public):
                    Namespace (Ident 1 [10-11] "A"): Item 1
                Item 1 [18-71] (Internal):
                    Parent: 0
                    Type (Ident 0 [25-28] "Foo"): UDT [18-71]:
                        TyDef [18-71]: Tuple:
                            TyDef [39-45]: Field:
                                name: x [39-40]
                                type: Int
                            TyDef [55-64]: Field:
                                name: y [55-56]
                                type: Double"#]],
    );
}

#[test]
fn lower_struct_constructor() {
    check_hir(
        indoc! {"
            namespace A {
                struct Foo {
                    x: Int,
                    y: Double,
                }
                operation Bar() : Unit {
                    let z = new Foo { x = 1, y = 2.3 };
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-152] (Public):
                    Namespace (Ident 14 [10-11] "A"): Item 1, Item 2
                Item 1 [18-71] (Internal):
                    Parent: 0
                    Type (Ident 0 [25-28] "Foo"): UDT [18-71]:
                        TyDef [18-71]: Tuple:
                            TyDef [39-45]: Field:
                                name: x [39-40]
                                type: Int
                            TyDef [55-64]: Field:
                                name: y [55-56]
                                type: Double
                Item 2 [76-150] (Internal):
                    Parent: 0
                    Callable 1 [76-150] (operation):
                        name: Ident 2 [86-89] "Bar"
                        input: Pat 3 [89-91] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [76-150]: Impl:
                            Block 5 [99-150] [Type Unit]:
                                Stmt 6 [109-144]: Local (Immutable):
                                    Pat 7 [113-114] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [113-114] "z"
                                    Expr 9 [117-143] [Type UDT<"Foo": Item 1>]: Struct (Item 1):
                                        FieldsAssign 10 [127-132]: (Path([0])) Expr 11 [131-132] [Type Int]: Lit: Int(1)
                                        FieldsAssign 12 [134-141]: (Path([1])) Expr 13 [138-141] [Type Double]: Lit: Double(2.3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lower_struct_copy_constructor() {
    check_hir(
        indoc! {"
            namespace A {
                struct Foo {
                    x: Int,
                    y: Double,
                }
                operation Bar() : Foo {
                    let z = new Foo { x = 1, y = 2.3 };
                    new Foo { ...z, x = 4 };
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-184] (Public):
                    Namespace (Ident 19 [10-11] "A"): Item 1, Item 2
                Item 1 [18-71] (Internal):
                    Parent: 0
                    Type (Ident 0 [25-28] "Foo"): UDT [18-71]:
                        TyDef [18-71]: Tuple:
                            TyDef [39-45]: Field:
                                name: x [39-40]
                                type: Int
                            TyDef [55-64]: Field:
                                name: y [55-56]
                                type: Double
                Item 2 [76-182] (Internal):
                    Parent: 0
                    Callable 1 [76-182] (operation):
                        name: Ident 2 [86-89] "Bar"
                        input: Pat 3 [89-91] [Type Unit]: Unit
                        output: UDT<"Foo": Item 1>
                        functors: empty set
                        body: SpecDecl 4 [76-182]: Impl:
                            Block 5 [98-182] [Type Unit]:
                                Stmt 6 [108-143]: Local (Immutable):
                                    Pat 7 [112-113] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [112-113] "z"
                                    Expr 9 [116-142] [Type UDT<"Foo": Item 1>]: Struct (Item 1):
                                        FieldsAssign 10 [126-131]: (Path([0])) Expr 11 [130-131] [Type Int]: Lit: Int(1)
                                        FieldsAssign 12 [133-140]: (Path([1])) Expr 13 [137-140] [Type Double]: Lit: Double(2.3)
                                Stmt 14 [152-176]: Semi: Expr 15 [152-175] [Type UDT<"Foo": Item 1>]: Struct (Item 1):
                                    Copy: Expr 16 [165-166] [Type UDT<"Foo": Item 1>]: Var: Local 8
                                    FieldsAssign 17 [168-173]: (Path([0])) Expr 18 [172-173] [Type Int]: Lit: Int(4)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lower_struct_copy_constructor_with_alternative_fields() {
    check_hir(
        indoc! {r#"
            namespace A {
                struct Foo {
                    x: Int,
                    y: Double,
                    z: String
                }
                operation Bar() : Foo {
                    let z = new Foo { x = 1, y = 2.3, z = "four" };
                    new Foo { ...z, z = "five", y = 6.7 };
                }
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-228] (Public):
                    Namespace (Ident 23 [10-11] "A"): Item 1, Item 2
                Item 1 [18-89] (Internal):
                    Parent: 0
                    Type (Ident 0 [25-28] "Foo"): UDT [18-89]:
                        TyDef [18-89]: Tuple:
                            TyDef [39-45]: Field:
                                name: x [39-40]
                                type: Int
                            TyDef [55-64]: Field:
                                name: y [55-56]
                                type: Double
                            TyDef [74-83]: Field:
                                name: z [74-75]
                                type: String
                Item 2 [94-226] (Internal):
                    Parent: 0
                    Callable 1 [94-226] (operation):
                        name: Ident 2 [104-107] "Bar"
                        input: Pat 3 [107-109] [Type Unit]: Unit
                        output: UDT<"Foo": Item 1>
                        functors: empty set
                        body: SpecDecl 4 [94-226]: Impl:
                            Block 5 [116-226] [Type Unit]:
                                Stmt 6 [126-173]: Local (Immutable):
                                    Pat 7 [130-131] [Type UDT<"Foo": Item 1>]: Bind: Ident 8 [130-131] "z"
                                    Expr 9 [134-172] [Type UDT<"Foo": Item 1>]: Struct (Item 1):
                                        FieldsAssign 10 [144-149]: (Path([0])) Expr 11 [148-149] [Type Int]: Lit: Int(1)
                                        FieldsAssign 12 [151-158]: (Path([1])) Expr 13 [155-158] [Type Double]: Lit: Double(2.3)
                                        FieldsAssign 14 [160-170]: (Path([2])) Expr 15 [164-170] [Type String]: String:
                                            Lit: "four"
                                Stmt 16 [182-220]: Semi: Expr 17 [182-219] [Type UDT<"Foo": Item 1>]: Struct (Item 1):
                                    Copy: Expr 18 [195-196] [Type UDT<"Foo": Item 1>]: Var: Local 8
                                    FieldsAssign 19 [198-208]: (Path([2])) Expr 20 [202-208] [Type String]: String:
                                        Lit: "five"
                                    FieldsAssign 21 [210-217]: (Path([1])) Expr 22 [214-217] [Type Double]: Lit: Double(6.7)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lower_fields_path() {
    check_hir(
        indoc! {r#"
            namespace Foo {
                struct A { b : B }
                struct B { c : C }
                struct C { i : Int }
                operation Bar(a : A) : Unit {
                    let x = a.b.c.i;
                }
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-153] (Public):
                    Namespace (Ident 16 [10-13] "Foo"): Item 1, Item 2, Item 3, Item 4
                Item 1 [20-38] (Internal):
                    Parent: 0
                    Type (Ident 0 [27-28] "A"): UDT [20-38]:
                        TyDef [20-38]: Tuple:
                            TyDef [31-36]: Field:
                                name: b [31-32]
                                type: UDT<"B": Item 2>
                Item 2 [43-61] (Internal):
                    Parent: 0
                    Type (Ident 1 [50-51] "B"): UDT [43-61]:
                        TyDef [43-61]: Tuple:
                            TyDef [54-59]: Field:
                                name: c [54-55]
                                type: UDT<"C": Item 3>
                Item 3 [66-86] (Internal):
                    Parent: 0
                    Type (Ident 2 [73-74] "C"): UDT [66-86]:
                        TyDef [66-86]: Tuple:
                            TyDef [77-84]: Field:
                                name: i [77-78]
                                type: Int
                Item 4 [91-151] (Internal):
                    Parent: 0
                    Callable 3 [91-151] (operation):
                        name: Ident 4 [101-104] "Bar"
                        input: Pat 5 [105-110] [Type UDT<"A": Item 1>]: Bind: Ident 6 [105-106] "a"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 7 [91-151]: Impl:
                            Block 8 [119-151] [Type Unit]:
                                Stmt 9 [129-145]: Local (Immutable):
                                    Pat 10 [133-134] [Type Int]: Bind: Ident 11 [133-134] "x"
                                    Expr 12 [137-144] [Type Int]: Field:
                                        Expr 15 [137-142] [Type UDT<"C": Item 3>]: Field:
                                            Expr 14 [137-140] [Type UDT<"B": Item 2>]: Field:
                                                Expr 13 [137-138] [Type UDT<"A": Item 1>]: Var: Local 6
                                                Path(FieldPath { indices: [0] })
                                            Path(FieldPath { indices: [0] })
                                        Path(FieldPath { indices: [0] })
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lower_fields_path_with_expr() {
    check_hir(
        indoc! {r#"
            namespace Foo {
                struct A { b : B }
                struct B { c : C }
                struct C { i : Int }
                operation Bar(a : A) : Unit {
                    let x = { a.b }.c.i;
                }
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-157] (Public):
                    Namespace (Ident 19 [10-13] "Foo"): Item 1, Item 2, Item 3, Item 4
                Item 1 [20-38] (Internal):
                    Parent: 0
                    Type (Ident 0 [27-28] "A"): UDT [20-38]:
                        TyDef [20-38]: Tuple:
                            TyDef [31-36]: Field:
                                name: b [31-32]
                                type: UDT<"B": Item 2>
                Item 2 [43-61] (Internal):
                    Parent: 0
                    Type (Ident 1 [50-51] "B"): UDT [43-61]:
                        TyDef [43-61]: Tuple:
                            TyDef [54-59]: Field:
                                name: c [54-55]
                                type: UDT<"C": Item 3>
                Item 3 [66-86] (Internal):
                    Parent: 0
                    Type (Ident 2 [73-74] "C"): UDT [66-86]:
                        TyDef [66-86]: Tuple:
                            TyDef [77-84]: Field:
                                name: i [77-78]
                                type: Int
                Item 4 [91-155] (Internal):
                    Parent: 0
                    Callable 3 [91-155] (operation):
                        name: Ident 4 [101-104] "Bar"
                        input: Pat 5 [105-110] [Type UDT<"A": Item 1>]: Bind: Ident 6 [105-106] "a"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 7 [91-155]: Impl:
                            Block 8 [119-155] [Type Unit]:
                                Stmt 9 [129-149]: Local (Immutable):
                                    Pat 10 [133-134] [Type Int]: Bind: Ident 11 [133-134] "x"
                                    Expr 12 [137-148] [Type Int]: Field:
                                        Expr 13 [137-146] [Type UDT<"C": Item 3>]: Field:
                                            Expr 14 [137-144] [Type UDT<"B": Item 2>]: Expr Block: Block 15 [137-144] [Type UDT<"B": Item 2>]:
                                                Stmt 16 [139-142]: Expr: Expr 17 [139-142] [Type UDT<"B": Item 2>]: Field:
                                                    Expr 18 [139-140] [Type UDT<"A": Item 1>]: Var: Local 6
                                                    Path(FieldPath { indices: [0] })
                                            Path(FieldPath { indices: [0] })
                                        Path(FieldPath { indices: [0] })
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_empty_closure() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    let f = x -> x + 1;
                    f(1)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-89] (Public):
                    Namespace (Ident 24 [10-11] "A"): Item 1
                Item 1 [18-87] (Internal):
                    Parent: 0
                    Callable 0 [18-87] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-32] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [18-87]: Impl:
                            Block 4 [39-87] [Type Int]:
                                Stmt 5 [49-68]: Local (Immutable):
                                    Pat 6 [53-54] [Type (Int -> Int)]: Bind: Ident 7 [53-54] "f"
                                    Expr 8 [57-67] [Type (Int -> Int)]: Closure([], 2)
                                Stmt 20 [77-81]: Expr: Expr 21 [77-81] [Type Int]: Call:
                                    Expr 22 [77-78] [Type (Int -> Int)]: Var: Local 7
                                    Expr 23 [79-80] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [57-67] (Internal):
                    Parent: 1
                    Callable 15 [57-67] (function):
                        name: Ident 16 [57-67] "<lambda>"
                        input: Pat 14 [57-67] [Type (Int,)]: Tuple:
                            Pat 9 [57-58] [Type Int]: Bind: Ident 10 [57-58] "x"
                        output: Int
                        functors: empty set
                        body: SpecDecl 17 [62-67]: Impl:
                            Block 18 [62-67] [Type Int]:
                                Stmt 19 [62-67]: Expr: Expr 11 [62-67] [Type Int]: BinOp (Add):
                                    Expr 12 [62-63] [Type Int]: Var: Local 10
                                    Expr 13 [66-67] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_empty_closure_passed() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(f : Int -> Int) : Int { f(2) }
                function Bar() : Int { Foo(x -> x + 1) }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-108] (Public):
                    Namespace (Ident 30 [10-11] "A"): Item 1, Item 2
                Item 1 [18-61] (Internal):
                    Parent: 0
                    Callable 0 [18-61] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [31-45] [Type (Int -> Int)]: Bind: Ident 3 [31-32] "f"
                        output: Int
                        functors: empty set
                        body: SpecDecl 4 [18-61]: Impl:
                            Block 5 [53-61] [Type Int]:
                                Stmt 6 [55-59]: Expr: Expr 7 [55-59] [Type Int]: Call:
                                    Expr 8 [55-56] [Type (Int -> Int)]: Var: Local 3
                                    Expr 9 [57-58] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [66-106] (Internal):
                    Parent: 0
                    Callable 10 [66-106] (function):
                        name: Ident 11 [75-78] "Bar"
                        input: Pat 12 [78-80] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 13 [66-106]: Impl:
                            Block 14 [87-106] [Type Int]:
                                Stmt 15 [89-104]: Expr: Expr 16 [89-104] [Type Int]: Call:
                                    Expr 17 [89-92] [Type ((Int -> Int) -> Int)]: Var: Item 1
                                    Expr 18 [93-103] [Type (Int -> Int)]: Closure([], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [93-103] (Internal):
                    Parent: 2
                    Callable 25 [93-103] (function):
                        name: Ident 26 [93-103] "<lambda>"
                        input: Pat 24 [93-103] [Type (Int,)]: Tuple:
                            Pat 19 [93-94] [Type Int]: Bind: Ident 20 [93-94] "x"
                        output: Int
                        functors: empty set
                        body: SpecDecl 27 [98-103]: Impl:
                            Block 28 [98-103] [Type Int]:
                                Stmt 29 [98-103]: Expr: Expr 21 [98-103] [Type Int]: BinOp (Add):
                                    Expr 22 [98-99] [Type Int]: Var: Local 20
                                    Expr 23 [102-103] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_closure() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    let x = 5;
                    let f = y -> x + y;
                    f(2)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-108] (Public):
                    Namespace (Ident 30 [10-11] "A"): Item 1
                Item 1 [18-106] (Internal):
                    Parent: 0
                    Callable 0 [18-106] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-32] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [18-106]: Impl:
                            Block 4 [39-106] [Type Int]:
                                Stmt 5 [49-59]: Local (Immutable):
                                    Pat 6 [53-54] [Type Int]: Bind: Ident 7 [53-54] "x"
                                    Expr 8 [57-58] [Type Int]: Lit: Int(5)
                                Stmt 9 [68-87]: Local (Immutable):
                                    Pat 10 [72-73] [Type (Int -> Int)]: Bind: Ident 11 [72-73] "f"
                                    Expr 12 [76-86] [Type (Int -> Int)]: Closure([7], 2)
                                Stmt 26 [96-100]: Expr: Expr 27 [96-100] [Type Int]: Call:
                                    Expr 28 [96-97] [Type (Int -> Int)]: Var: Local 11
                                    Expr 29 [98-99] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [76-86] (Internal):
                    Parent: 1
                    Callable 21 [76-86] (function):
                        name: Ident 22 [76-86] "<lambda>"
                        input: Pat 19 [76-86] [Type (Int, Int)]: Tuple:
                            Pat 20 [53-54] [Type Int]: Bind: Ident 18 [53-54] "x"
                            Pat 13 [76-77] [Type Int]: Bind: Ident 14 [76-77] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 23 [81-86]: Impl:
                            Block 24 [81-86] [Type Int]:
                                Stmt 25 [81-86]: Expr: Expr 15 [81-86] [Type Int]: BinOp (Add):
                                    Expr 16 [81-82] [Type Int]: Var: Local 18
                                    Expr 17 [85-86] [Type Int]: Var: Local 14
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_closure_repeated_var() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    let x = 5;
                    let f = y -> x + x + y;
                    f(2)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-112] (Public):
                    Namespace (Ident 32 [10-11] "A"): Item 1
                Item 1 [18-110] (Internal):
                    Parent: 0
                    Callable 0 [18-110] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-32] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [18-110]: Impl:
                            Block 4 [39-110] [Type Int]:
                                Stmt 5 [49-59]: Local (Immutable):
                                    Pat 6 [53-54] [Type Int]: Bind: Ident 7 [53-54] "x"
                                    Expr 8 [57-58] [Type Int]: Lit: Int(5)
                                Stmt 9 [68-91]: Local (Immutable):
                                    Pat 10 [72-73] [Type (Int -> Int)]: Bind: Ident 11 [72-73] "f"
                                    Expr 12 [76-90] [Type (Int -> Int)]: Closure([7], 2)
                                Stmt 28 [100-104]: Expr: Expr 29 [100-104] [Type Int]: Call:
                                    Expr 30 [100-101] [Type (Int -> Int)]: Var: Local 11
                                    Expr 31 [102-103] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [76-90] (Internal):
                    Parent: 1
                    Callable 23 [76-90] (function):
                        name: Ident 24 [76-90] "<lambda>"
                        input: Pat 21 [76-90] [Type (Int, Int)]: Tuple:
                            Pat 22 [53-54] [Type Int]: Bind: Ident 20 [53-54] "x"
                            Pat 13 [76-77] [Type Int]: Bind: Ident 14 [76-77] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 25 [81-90]: Impl:
                            Block 26 [81-90] [Type Int]:
                                Stmt 27 [81-90]: Expr: Expr 15 [81-90] [Type Int]: BinOp (Add):
                                    Expr 16 [81-86] [Type Int]: BinOp (Add):
                                        Expr 17 [81-82] [Type Int]: Var: Local 20
                                        Expr 18 [85-86] [Type Int]: Var: Local 20
                                    Expr 19 [89-90] [Type Int]: Var: Local 14
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_closure_passed() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(f : Int -> Int) : Int { f(2) }
                function Bar() : Int {
                    let x = 5;
                    Foo(y -> x + y)
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-139] (Public):
                    Namespace (Ident 36 [10-11] "A"): Item 1, Item 2
                Item 1 [18-61] (Internal):
                    Parent: 0
                    Callable 0 [18-61] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [31-45] [Type (Int -> Int)]: Bind: Ident 3 [31-32] "f"
                        output: Int
                        functors: empty set
                        body: SpecDecl 4 [18-61]: Impl:
                            Block 5 [53-61] [Type Int]:
                                Stmt 6 [55-59]: Expr: Expr 7 [55-59] [Type Int]: Call:
                                    Expr 8 [55-56] [Type (Int -> Int)]: Var: Local 3
                                    Expr 9 [57-58] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [66-137] (Internal):
                    Parent: 0
                    Callable 10 [66-137] (function):
                        name: Ident 11 [75-78] "Bar"
                        input: Pat 12 [78-80] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 13 [66-137]: Impl:
                            Block 14 [87-137] [Type Int]:
                                Stmt 15 [97-107]: Local (Immutable):
                                    Pat 16 [101-102] [Type Int]: Bind: Ident 17 [101-102] "x"
                                    Expr 18 [105-106] [Type Int]: Lit: Int(5)
                                Stmt 19 [116-131]: Expr: Expr 20 [116-131] [Type Int]: Call:
                                    Expr 21 [116-119] [Type ((Int -> Int) -> Int)]: Var: Item 1
                                    Expr 22 [120-130] [Type (Int -> Int)]: Closure([17], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [120-130] (Internal):
                    Parent: 2
                    Callable 31 [120-130] (function):
                        name: Ident 32 [120-130] "<lambda>"
                        input: Pat 29 [120-130] [Type (Int, Int)]: Tuple:
                            Pat 30 [101-102] [Type Int]: Bind: Ident 28 [101-102] "x"
                            Pat 23 [120-121] [Type Int]: Bind: Ident 24 [120-121] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 33 [125-130]: Impl:
                            Block 34 [125-130] [Type Int]:
                                Stmt 35 [125-130]: Expr: Expr 25 [125-130] [Type Int]: BinOp (Add):
                                    Expr 26 [125-126] [Type Int]: Var: Local 28
                                    Expr 27 [129-130] [Type Int]: Var: Local 24
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_function_nested_closure() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(f : Int -> Int -> Int) : Int { f(2)(3) }
                function Bar() : Int {
                    let a = 5;
                    Foo(b -> {
                        let c = 1;
                        d -> a + b + c + d
                    })
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-209] (Public):
                    Namespace (Ident 64 [10-11] "A"): Item 1, Item 2
                Item 1 [18-71] (Internal):
                    Parent: 0
                    Callable 0 [18-71] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [31-52] [Type (Int -> (Int -> Int))]: Bind: Ident 3 [31-32] "f"
                        output: Int
                        functors: empty set
                        body: SpecDecl 4 [18-71]: Impl:
                            Block 5 [60-71] [Type Int]:
                                Stmt 6 [62-69]: Expr: Expr 7 [62-69] [Type Int]: Call:
                                    Expr 8 [62-66] [Type (Int -> Int)]: Call:
                                        Expr 9 [62-63] [Type (Int -> (Int -> Int))]: Var: Local 3
                                        Expr 10 [64-65] [Type Int]: Lit: Int(2)
                                    Expr 11 [67-68] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [76-207] (Internal):
                    Parent: 0
                    Callable 12 [76-207] (function):
                        name: Ident 13 [85-88] "Bar"
                        input: Pat 14 [88-90] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 15 [76-207]: Impl:
                            Block 16 [97-207] [Type Int]:
                                Stmt 17 [107-117]: Local (Immutable):
                                    Pat 18 [111-112] [Type Int]: Bind: Ident 19 [111-112] "a"
                                    Expr 20 [115-116] [Type Int]: Lit: Int(5)
                                Stmt 21 [126-201]: Expr: Expr 22 [126-201] [Type Int]: Call:
                                    Expr 23 [126-129] [Type ((Int -> (Int -> Int)) -> Int)]: Var: Item 1
                                    Expr 24 [130-200] [Type (Int -> (Int -> Int))]: Closure([19], 4)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [172-190] (Internal):
                    Parent: 2
                    Callable 51 [172-190] (function):
                        name: Ident 52 [172-190] "<lambda>"
                        input: Pat 47 [172-190] [Type (Int, Int, Int, Int)]: Tuple:
                            Pat 48 [111-112] [Type Int]: Bind: Ident 44 [111-112] "a"
                            Pat 49 [130-131] [Type Int]: Bind: Ident 45 [130-131] "b"
                            Pat 50 [153-154] [Type Int]: Bind: Ident 46 [153-154] "c"
                            Pat 35 [172-173] [Type Int]: Bind: Ident 36 [172-173] "d"
                        output: Int
                        functors: empty set
                        body: SpecDecl 53 [177-190]: Impl:
                            Block 54 [177-190] [Type Int]:
                                Stmt 55 [177-190]: Expr: Expr 37 [177-190] [Type Int]: BinOp (Add):
                                    Expr 38 [177-186] [Type Int]: BinOp (Add):
                                        Expr 39 [177-182] [Type Int]: BinOp (Add):
                                            Expr 40 [177-178] [Type Int]: Var: Local 44
                                            Expr 41 [181-182] [Type Int]: Var: Local 45
                                        Expr 42 [185-186] [Type Int]: Var: Local 46
                                    Expr 43 [189-190] [Type Int]: Var: Local 36
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 4 [130-200] (Internal):
                    Parent: 2
                    Callable 59 [130-200] (function):
                        name: Ident 60 [130-200] "<lambda>"
                        input: Pat 57 [130-200] [Type (Int, Int)]: Tuple:
                            Pat 58 [111-112] [Type Int]: Bind: Ident 56 [111-112] "a"
                            Pat 25 [130-131] [Type Int]: Bind: Ident 26 [130-131] "b"
                        output: (Int -> Int)
                        functors: empty set
                        body: SpecDecl 61 [135-200]: Impl:
                            Block 62 [135-200] [Type (Int -> Int)]:
                                Stmt 63 [135-200]: Expr: Expr 27 [135-200] [Type (Int -> Int)]: Expr Block: Block 28 [135-200] [Type (Int -> Int)]:
                                    Stmt 29 [149-159]: Local (Immutable):
                                        Pat 30 [153-154] [Type Int]: Bind: Ident 31 [153-154] "c"
                                        Expr 32 [157-158] [Type Int]: Lit: Int(1)
                                    Stmt 33 [172-190]: Expr: Expr 34 [172-190] [Type (Int -> Int)]: Closure([56, 26, 31], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_operation_empty_closure() {
    check_hir(
        indoc! {"
            namespace A {
                operation Foo(op : Qubit => ()) : () {
                    use q = Qubit();
                    op(q)
                }
                operation Bar() : Result { Foo(q => ()) }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-149] (Public):
                    Namespace (Ident 32 [10-11] "A"): Item 1, Item 2
                Item 1 [18-101] (Internal):
                    Parent: 0
                    Callable 0 [18-101] (operation):
                        name: Ident 1 [28-31] "Foo"
                        generics:
                            0: functor (empty set)
                        input: Pat 2 [32-48] [Type (Qubit => Unit is Param<0>)]: Bind: Ident 3 [32-34] "op"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [18-101]: Impl:
                            Block 5 [55-101] [Type Unit]:
                                Stmt 6 [65-81]: Qubit (Fresh)
                                    Pat 7 [69-70] [Type Qubit]: Bind: Ident 8 [69-70] "q"
                                    QubitInit 9 [73-80] [Type Qubit]: Single
                                Stmt 10 [90-95]: Expr: Expr 11 [90-95] [Type Unit]: Call:
                                    Expr 12 [90-92] [Type (Qubit => Unit is Param<0>)]: Var: Local 3
                                    Expr 13 [93-94] [Type Qubit]: Var: Local 8
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [106-147] (Internal):
                    Parent: 0
                    Callable 14 [106-147] (operation):
                        name: Ident 15 [116-119] "Bar"
                        input: Pat 16 [119-121] [Type Unit]: Unit
                        output: Result
                        functors: empty set
                        body: SpecDecl 17 [106-147]: Impl:
                            Block 18 [131-147] [Type Unit]:
                                Stmt 19 [133-145]: Expr: Expr 20 [133-145] [Type Unit]: Call:
                                    Expr 21 [133-136] [Type ((Qubit => Unit) => Unit)]: Var:
                                        res: Item 1
                                        generics:
                                            empty set
                                    Expr 22 [137-144] [Type (Qubit => Unit)]: Closure([], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [137-144] (Internal):
                    Parent: 2
                    Callable 27 [137-144] (operation):
                        name: Ident 28 [137-144] "<lambda>"
                        input: Pat 26 [137-144] [Type (Qubit,)]: Tuple:
                            Pat 23 [137-138] [Type Qubit]: Bind: Ident 24 [137-138] "q"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 29 [142-144]: Impl:
                            Block 30 [142-144] [Type Unit]:
                                Stmt 31 [142-144]: Expr: Expr 25 [142-144] [Type Unit]: Unit
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_operation_closure() {
    check_hir(
        indoc! {"
            namespace A {
                operation MResetZ(q : Qubit) : Result { body intrinsic; }
                operation Foo(op : () => Result) : Result { op() }
                operation Bar() : Result {
                    use q = Qubit();
                    Foo(() => MResetZ(q))
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-224] (Public):
                    Namespace (Ident 40 [10-11] "A"): Item 1, Item 2, Item 3
                Item 1 [18-75] (Internal):
                    Parent: 0
                    Callable 0 [18-75] (operation):
                        name: Ident 1 [28-35] "MResetZ"
                        input: Pat 2 [36-45] [Type Qubit]: Bind: Ident 3 [36-37] "q"
                        output: Result
                        functors: empty set
                        body: SpecDecl 4 [58-73]: Gen: Intrinsic
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [80-130] (Internal):
                    Parent: 0
                    Callable 5 [80-130] (operation):
                        name: Ident 6 [90-93] "Foo"
                        generics:
                            0: functor (empty set)
                        input: Pat 7 [94-111] [Type (Unit => Result is Param<0>)]: Bind: Ident 8 [94-96] "op"
                        output: Result
                        functors: empty set
                        body: SpecDecl 9 [80-130]: Impl:
                            Block 10 [122-130] [Type Result]:
                                Stmt 11 [124-128]: Expr: Expr 12 [124-128] [Type Result]: Call:
                                    Expr 13 [124-126] [Type (Unit => Result is Param<0>)]: Var: Local 8
                                    Expr 14 [126-128] [Type Unit]: Unit
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [135-222] (Internal):
                    Parent: 0
                    Callable 15 [135-222] (operation):
                        name: Ident 16 [145-148] "Bar"
                        input: Pat 17 [148-150] [Type Unit]: Unit
                        output: Result
                        functors: empty set
                        body: SpecDecl 18 [135-222]: Impl:
                            Block 19 [160-222] [Type Result]:
                                Stmt 20 [170-186]: Qubit (Fresh)
                                    Pat 21 [174-175] [Type Qubit]: Bind: Ident 22 [174-175] "q"
                                    QubitInit 23 [178-185] [Type Qubit]: Single
                                Stmt 24 [195-216]: Expr: Expr 25 [195-216] [Type Result]: Call:
                                    Expr 26 [195-198] [Type ((Unit => Result) => Result)]: Var:
                                        res: Item 2
                                        generics:
                                            empty set
                                    Expr 27 [199-215] [Type (Unit => Result)]: Closure([22], 4)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 4 [199-215] (Internal):
                    Parent: 3
                    Callable 35 [199-215] (operation):
                        name: Ident 36 [199-215] "<lambda>"
                        input: Pat 33 [199-215] [Type (Qubit, Unit)]: Tuple:
                            Pat 34 [174-175] [Type Qubit]: Bind: Ident 32 [174-175] "q"
                            Pat 28 [199-201] [Type Unit]: Unit
                        output: Result
                        functors: empty set
                        body: SpecDecl 37 [205-215]: Impl:
                            Block 38 [205-215] [Type Result]:
                                Stmt 39 [205-215]: Expr: Expr 29 [205-215] [Type Result]: Call:
                                    Expr 30 [205-212] [Type (Qubit => Result)]: Var: Item 1
                                    Expr 31 [213-214] [Type Qubit]: Var: Local 32
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_adj() {
    check_hir(
        indoc! {r#"
            namespace A {
                operation X(q : Qubit) : () is Adj {}
                operation Foo(op : Qubit => () is Adj) : () {}
                operation Bar() : () { Foo(q => X(q)); }
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-153] (Public):
                    Namespace (Ident 32 [10-11] "A"): Item 1, Item 2, Item 3
                Item 1 [18-55] (Internal):
                    Parent: 0
                    Callable 0 [18-55] (operation):
                        name: Ident 1 [28-29] "X"
                        input: Pat 2 [30-39] [Type Qubit]: Bind: Ident 3 [30-31] "q"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 4 [18-55]: Impl:
                            Block 5 [53-55]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [60-106] (Internal):
                    Parent: 0
                    Callable 6 [60-106] (operation):
                        name: Ident 7 [70-73] "Foo"
                        generics:
                            0: functor (Adj)
                        input: Pat 8 [74-97] [Type (Qubit => Unit is Param<0>)]: Bind: Ident 9 [74-76] "op"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 10 [60-106]: Impl:
                            Block 11 [104-106]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [111-151] (Internal):
                    Parent: 0
                    Callable 12 [111-151] (operation):
                        name: Ident 13 [121-124] "Bar"
                        input: Pat 14 [124-126] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 15 [111-151]: Impl:
                            Block 16 [132-151] [Type Unit]:
                                Stmt 17 [134-149]: Semi: Expr 18 [134-148] [Type Unit]: Call:
                                    Expr 19 [134-137] [Type ((Qubit => Unit is Adj) => Unit)]: Var:
                                        res: Item 2
                                        generics:
                                            Adj
                                    Expr 20 [138-147] [Type (Qubit => Unit is Adj)]: Closure([], 4)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 4 [138-147] (Internal):
                    Parent: 3
                    Callable 27 [138-147] (operation):
                        name: Ident 28 [138-147] "<lambda>"
                        input: Pat 26 [138-147] [Type (Qubit,)]: Tuple:
                            Pat 21 [138-139] [Type Qubit]: Bind: Ident 22 [138-139] "q"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 29 [143-147]: Impl:
                            Block 30 [143-147] [Type Unit]:
                                Stmt 31 [143-147]: Expr: Expr 23 [143-147] [Type Unit]: Call:
                                    Expr 24 [143-144] [Type (Qubit => Unit is Adj)]: Var: Item 1
                                    Expr 25 [145-146] [Type Qubit]: Var: Local 22
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_one_hole() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(x : Int, y : Int) : Int { x + y }
                function Bar() : () { let f = Foo(_, 2); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-113] (Public):
                    Namespace (Ident 45 [10-11] "A"): Item 1, Item 2
                Item 1 [18-64] (Internal):
                    Parent: 0
                    Callable 0 [18-64] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-48] [Type (Int, Int)]: Tuple:
                            Pat 3 [31-38] [Type Int]: Bind: Ident 4 [31-32] "x"
                            Pat 5 [40-47] [Type Int]: Bind: Ident 6 [40-41] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 7 [18-64]: Impl:
                            Block 8 [55-64] [Type Int]:
                                Stmt 9 [57-62]: Expr: Expr 10 [57-62] [Type Int]: BinOp (Add):
                                    Expr 11 [57-58] [Type Int]: Var: Local 4
                                    Expr 12 [61-62] [Type Int]: Var: Local 6
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [69-111] (Internal):
                    Parent: 0
                    Callable 13 [69-111] (function):
                        name: Ident 14 [78-81] "Bar"
                        input: Pat 15 [81-83] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 16 [69-111]: Impl:
                            Block 17 [89-111] [Type Unit]:
                                Stmt 18 [91-109]: Local (Immutable):
                                    Pat 19 [95-96] [Type (Int -> Int)]: Bind: Ident 20 [95-96] "f"
                                    Expr 21 [99-108] [Type (Int -> Int)]: Expr Block: Block 42 [99-108] [Type (Int -> Int)]:
                                        Stmt 30 [106-107]: Local (Immutable):
                                            Pat 29 [106-107] [Type Int]: Bind: Ident 27 [106-107] "arg"
                                            Expr 26 [106-107] [Type Int]: Lit: Int(2)
                                        Stmt 43 [99-108]: Expr: Expr 44 [99-108] [Type (Int -> Int)]: Closure([27], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [99-108] (Internal):
                    Parent: 2
                    Callable 37 [99-108] (function):
                        name: Ident 38 [99-108] "<lambda>"
                        input: Pat 35 [99-108] [Type (Int, Int)]: Tuple:
                            Pat 36 [106-107] [Type Int]: Bind: Ident 34 [106-107] "arg"
                            Pat 24 [103-104] [Type Int]: Bind: Ident 23 [103-104] "hole"
                        output: Int
                        functors: empty set
                        body: SpecDecl 39 [99-108]: Impl:
                            Block 40 [99-108] [Type Int]:
                                Stmt 41 [99-108]: Expr: Expr 33 [99-108] [Type Int]: Call:
                                    Expr 22 [99-102] [Type ((Int, Int) -> Int)]: Var: Item 1
                                    Expr 32 [102-108] [Type (Int, Int)]: Tuple:
                                        Expr 25 [103-104] [Type Int]: Var: Local 23
                                        Expr 28 [106-107] [Type Int]: Var: Local 34
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_two_holes() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(x : Int, y : Int) : Int { x + y }
                function Bar() : () { let f = Foo(_, _); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-113] (Public):
                    Namespace (Ident 41 [10-11] "A"): Item 1, Item 2
                Item 1 [18-64] (Internal):
                    Parent: 0
                    Callable 0 [18-64] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-48] [Type (Int, Int)]: Tuple:
                            Pat 3 [31-38] [Type Int]: Bind: Ident 4 [31-32] "x"
                            Pat 5 [40-47] [Type Int]: Bind: Ident 6 [40-41] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 7 [18-64]: Impl:
                            Block 8 [55-64] [Type Int]:
                                Stmt 9 [57-62]: Expr: Expr 10 [57-62] [Type Int]: BinOp (Add):
                                    Expr 11 [57-58] [Type Int]: Var: Local 4
                                    Expr 12 [61-62] [Type Int]: Var: Local 6
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [69-111] (Internal):
                    Parent: 0
                    Callable 13 [69-111] (function):
                        name: Ident 14 [78-81] "Bar"
                        input: Pat 15 [81-83] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 16 [69-111]: Impl:
                            Block 17 [89-111] [Type Unit]:
                                Stmt 18 [91-109]: Local (Immutable):
                                    Pat 19 [95-96] [Type ((Int, Int) -> Int)]: Bind: Ident 20 [95-96] "f"
                                    Expr 21 [99-108] [Type ((Int, Int) -> Int)]: Expr Block: Block 38 [99-108] [Type ((Int, Int) -> Int)]:
                                        Stmt 39 [99-108]: Expr: Expr 40 [99-108] [Type ((Int, Int) -> Int)]: Closure([], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [99-108] (Internal):
                    Parent: 2
                    Callable 33 [99-108] (function):
                        name: Ident 34 [99-108] "<lambda>"
                        input: Pat 32 [99-108] [Type ((Int, Int),)]: Tuple:
                            Pat 30 [102-108] [Type (Int, Int)]: Tuple:
                                Pat 24 [103-104] [Type Int]: Bind: Ident 23 [103-104] "hole"
                                Pat 27 [106-107] [Type Int]: Bind: Ident 26 [106-107] "hole"
                        output: Int
                        functors: empty set
                        body: SpecDecl 35 [99-108]: Impl:
                            Block 36 [99-108] [Type Int]:
                                Stmt 37 [99-108]: Expr: Expr 31 [99-108] [Type Int]: Call:
                                    Expr 22 [99-102] [Type ((Int, Int) -> Int)]: Var: Item 1
                                    Expr 29 [102-108] [Type (Int, Int)]: Tuple:
                                        Expr 25 [103-104] [Type Int]: Var: Local 23
                                        Expr 28 [106-107] [Type Int]: Var: Local 26
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_nested_tuple() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}
                function Bar() : () { let f = Foo(_, (_, 1.0, _), _); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-157] (Public):
                    Namespace (Ident 60 [10-11] "A"): Item 1, Item 2
                Item 1 [18-95] (Internal):
                    Parent: 0
                    Callable 0 [18-95] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-87] [Type (Int, (Bool, Double, String), Result)]: Tuple:
                            Pat 3 [31-38] [Type Int]: Bind: Ident 4 [31-32] "a"
                            Pat 5 [40-74] [Type (Bool, Double, String)]: Tuple:
                                Pat 6 [41-49] [Type Bool]: Bind: Ident 7 [41-42] "b"
                                Pat 8 [51-61] [Type Double]: Bind: Ident 9 [51-52] "c"
                                Pat 10 [63-73] [Type String]: Bind: Ident 11 [63-64] "d"
                            Pat 12 [76-86] [Type Result]: Bind: Ident 13 [76-77] "e"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 14 [18-95]: Impl:
                            Block 15 [93-95]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [100-155] (Internal):
                    Parent: 0
                    Callable 16 [100-155] (function):
                        name: Ident 17 [109-112] "Bar"
                        input: Pat 18 [112-114] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 19 [100-155]: Impl:
                            Block 20 [120-155] [Type Unit]:
                                Stmt 21 [122-153]: Local (Immutable):
                                    Pat 22 [126-127] [Type ((Int, (Bool, String), Result) -> Unit)]: Bind: Ident 23 [126-127] "f"
                                    Expr 24 [130-152] [Type ((Int, (Bool, String), Result) -> Unit)]: Expr Block: Block 57 [130-152] [Type ((Int, (Bool, String), Result) -> Unit)]:
                                        Stmt 36 [141-144]: Local (Immutable):
                                            Pat 35 [141-144] [Type Double]: Bind: Ident 33 [141-144] "arg"
                                            Expr 32 [141-144] [Type Double]: Lit: Double(1)
                                        Stmt 58 [130-152]: Expr: Expr 59 [130-152] [Type ((Int, (Bool, String), Result) -> Unit)]: Closure([33], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [130-152] (Internal):
                    Parent: 2
                    Callable 52 [130-152] (function):
                        name: Ident 53 [130-152] "<lambda>"
                        input: Pat 50 [130-152] [Type (Double, (Int, (Bool, String), Result))]: Tuple:
                            Pat 51 [141-144] [Type Double]: Bind: Ident 49 [141-144] "arg"
                            Pat 47 [133-152] [Type (Int, (Bool, String), Result)]: Tuple:
                                Pat 27 [134-135] [Type Int]: Bind: Ident 26 [134-135] "hole"
                                Pat 42 [137-148] [Type (Bool, String)]: Tuple:
                                    Pat 30 [138-139] [Type Bool]: Bind: Ident 29 [138-139] "hole"
                                    Pat 39 [146-147] [Type String]: Bind: Ident 38 [146-147] "hole"
                                Pat 44 [150-151] [Type Result]: Bind: Ident 43 [150-151] "hole"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 54 [130-152]: Impl:
                            Block 55 [130-152] [Type Unit]:
                                Stmt 56 [130-152]: Expr: Expr 48 [130-152] [Type Unit]: Call:
                                    Expr 25 [130-133] [Type ((Int, (Bool, Double, String), Result) -> Unit)]: Var: Item 1
                                    Expr 46 [133-152] [Type (Int, (Bool, Double, String), Result)]: Tuple:
                                        Expr 28 [134-135] [Type Int]: Var: Local 26
                                        Expr 41 [137-148] [Type (Bool, Double, String)]: Tuple:
                                            Expr 31 [138-139] [Type Bool]: Var: Local 29
                                            Expr 34 [141-144] [Type Double]: Var: Local 49
                                            Expr 40 [146-147] [Type String]: Var: Local 38
                                        Expr 45 [150-151] [Type Result]: Var: Local 43
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_nested_tuple_singleton_unwrap() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(a : Int, (b : Bool, c : Double, d : String), e : Result) : () {}
                function Bar() : () { let f = Foo(_, (true, 1.0, _), _); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-160] (Public):
                    Namespace (Ident 64 [10-11] "A"): Item 1, Item 2
                Item 1 [18-95] (Internal):
                    Parent: 0
                    Callable 0 [18-95] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-87] [Type (Int, (Bool, Double, String), Result)]: Tuple:
                            Pat 3 [31-38] [Type Int]: Bind: Ident 4 [31-32] "a"
                            Pat 5 [40-74] [Type (Bool, Double, String)]: Tuple:
                                Pat 6 [41-49] [Type Bool]: Bind: Ident 7 [41-42] "b"
                                Pat 8 [51-61] [Type Double]: Bind: Ident 9 [51-52] "c"
                                Pat 10 [63-73] [Type String]: Bind: Ident 11 [63-64] "d"
                            Pat 12 [76-86] [Type Result]: Bind: Ident 13 [76-77] "e"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 14 [18-95]: Impl:
                            Block 15 [93-95]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [100-158] (Internal):
                    Parent: 0
                    Callable 16 [100-158] (function):
                        name: Ident 17 [109-112] "Bar"
                        input: Pat 18 [112-114] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 19 [100-158]: Impl:
                            Block 20 [120-158] [Type Unit]:
                                Stmt 21 [122-156]: Local (Immutable):
                                    Pat 22 [126-127] [Type ((Int, String, Result) -> Unit)]: Bind: Ident 23 [126-127] "f"
                                    Expr 24 [130-155] [Type ((Int, String, Result) -> Unit)]: Expr Block: Block 61 [130-155] [Type ((Int, String, Result) -> Unit)]:
                                        Stmt 33 [138-142]: Local (Immutable):
                                            Pat 32 [138-142] [Type Bool]: Bind: Ident 30 [138-142] "arg"
                                            Expr 29 [138-142] [Type Bool]: Lit: Bool(true)
                                        Stmt 39 [144-147]: Local (Immutable):
                                            Pat 38 [144-147] [Type Double]: Bind: Ident 36 [144-147] "arg"
                                            Expr 35 [144-147] [Type Double]: Lit: Double(1)
                                        Stmt 62 [130-155]: Expr: Expr 63 [130-155] [Type ((Int, String, Result) -> Unit)]: Closure([30, 36], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [130-155] (Internal):
                    Parent: 2
                    Callable 56 [130-155] (function):
                        name: Ident 57 [130-155] "<lambda>"
                        input: Pat 53 [130-155] [Type (Bool, Double, (Int, String, Result))]: Tuple:
                            Pat 54 [138-142] [Type Bool]: Bind: Ident 51 [138-142] "arg"
                            Pat 55 [144-147] [Type Double]: Bind: Ident 52 [144-147] "arg"
                            Pat 49 [133-155] [Type (Int, String, Result)]: Tuple:
                                Pat 27 [134-135] [Type Int]: Bind: Ident 26 [134-135] "hole"
                                Pat 42 [149-150] [Type String]: Bind: Ident 41 [149-150] "hole"
                                Pat 46 [153-154] [Type Result]: Bind: Ident 45 [153-154] "hole"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 58 [130-155]: Impl:
                            Block 59 [130-155] [Type Unit]:
                                Stmt 60 [130-155]: Expr: Expr 50 [130-155] [Type Unit]: Call:
                                    Expr 25 [130-133] [Type ((Int, (Bool, Double, String), Result) -> Unit)]: Var: Item 1
                                    Expr 48 [133-155] [Type (Int, (Bool, Double, String), Result)]: Tuple:
                                        Expr 28 [134-135] [Type Int]: Var: Local 26
                                        Expr 44 [137-151] [Type (Bool, Double, String)]: Tuple:
                                            Expr 31 [138-142] [Type Bool]: Var: Local 51
                                            Expr 37 [144-147] [Type Double]: Var: Local 52
                                            Expr 43 [149-150] [Type String]: Var: Local 41
                                        Expr 47 [153-154] [Type Result]: Var: Local 45
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn body_missing_should_fail() {
    check_errors(
        indoc! {"
        namespace test {
            operation A(q : Qubit) : Unit is Adj {
                adjoint ... {}
            }
        }
        "},
        &expect![[r#"
            [
                MissingBody(
                    Span {
                        lo: 21,
                        hi: 88,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn duplicate_specialization() {
    check_errors(
        indoc! {"
        namespace test {
            operation Foo() : Unit {
                body ... {}
                body ... {}
            }
        }
        "},
        &expect![[r#"
            [
                DuplicateSpec(
                    Span {
                        lo: 54,
                        hi: 65,
                    },
                ),
                DuplicateSpec(
                    Span {
                        lo: 74,
                        hi: 85,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn duplicate_specialization_with_gen() {
    check_errors(
        indoc! {"
        namespace test {
            operation Foo() : Unit {
                body ... {}
                body auto;
                body intrinsic;
            }
        }
        "},
        &expect![[r#"
            [
                DuplicateSpec(
                    Span {
                        lo: 54,
                        hi: 65,
                    },
                ),
                DuplicateSpec(
                    Span {
                        lo: 74,
                        hi: 84,
                    },
                ),
                DuplicateSpec(
                    Span {
                        lo: 93,
                        hi: 108,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn partial_app_unknown_callable() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo() : () { let f = Unknown(true, _, _); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-72] (Public):
                    Namespace (Ident 14 [10-11] "A"): Item 1
                Item 1 [18-70] (Internal):
                    Parent: 0
                    Callable 0 [18-70] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-32] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [18-70]: Impl:
                            Block 4 [38-70] [Type Unit]:
                                Stmt 5 [40-68]: Local (Immutable):
                                    Pat 6 [44-45] [Type ?3]: Bind: Ident 7 [44-45] "f"
                                    Expr 8 [48-67] [Type ?3]: Call:
                                        Expr 9 [48-55] [Type ?]: Var: Err
                                        Expr 10 [55-67] [Type (Bool, ?1, ?2)]: Tuple:
                                            Expr 11 [56-60] [Type Bool]: Lit: Bool(true)
                                            Expr 12 [62-63] [Type ?1]: Hole
                                            Expr 13 [65-66] [Type ?2]: Hole
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_too_many_args() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(x : Int) : Int { x }
                function Bar() : () { let f = Foo(1, _, _); }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-103] (Public):
                    Namespace (Ident 22 [10-11] "A"): Item 1, Item 2
                Item 1 [18-51] (Internal):
                    Parent: 0
                    Callable 0 [18-51] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [31-38] [Type Int]: Bind: Ident 3 [31-32] "x"
                        output: Int
                        functors: empty set
                        body: SpecDecl 4 [18-51]: Impl:
                            Block 5 [46-51] [Type Int]:
                                Stmt 6 [48-49]: Expr: Expr 7 [48-49] [Type Int]: Var: Local 3
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [56-101] (Internal):
                    Parent: 0
                    Callable 8 [56-101] (function):
                        name: Ident 9 [65-68] "Bar"
                        input: Pat 10 [68-70] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 11 [56-101]: Impl:
                            Block 12 [76-101] [Type Unit]:
                                Stmt 13 [78-99]: Local (Immutable):
                                    Pat 14 [82-83] [Type Int]: Bind: Ident 15 [82-83] "f"
                                    Expr 16 [86-98] [Type Int]: Call:
                                        Expr 17 [86-89] [Type (Int -> Int)]: Var: Item 1
                                        Expr 18 [89-98] [Type (Int, ?1, ?2)]: Tuple:
                                            Expr 19 [90-91] [Type Int]: Lit: Int(1)
                                            Expr 20 [93-94] [Type ?1]: Hole
                                            Expr 21 [96-97] [Type ?2]: Hole
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_bound_to_non_arrow_ty() {
    check_hir(
        indoc! {"
            namespace A {
                function Foo(x : Int, y : Int) : Int { x + y }
                function Bar() : () {
                    let f : Int = Foo(1, _);
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-131] (Public):
                    Namespace (Ident 45 [10-11] "A"): Item 1, Item 2
                Item 1 [18-64] (Internal):
                    Parent: 0
                    Callable 0 [18-64] (function):
                        name: Ident 1 [27-30] "Foo"
                        input: Pat 2 [30-48] [Type (Int, Int)]: Tuple:
                            Pat 3 [31-38] [Type Int]: Bind: Ident 4 [31-32] "x"
                            Pat 5 [40-47] [Type Int]: Bind: Ident 6 [40-41] "y"
                        output: Int
                        functors: empty set
                        body: SpecDecl 7 [18-64]: Impl:
                            Block 8 [55-64] [Type Int]:
                                Stmt 9 [57-62]: Expr: Expr 10 [57-62] [Type Int]: BinOp (Add):
                                    Expr 11 [57-58] [Type Int]: Var: Local 4
                                    Expr 12 [61-62] [Type Int]: Var: Local 6
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [69-129] (Internal):
                    Parent: 0
                    Callable 13 [69-129] (function):
                        name: Ident 14 [78-81] "Bar"
                        input: Pat 15 [81-83] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 16 [69-129]: Impl:
                            Block 17 [89-129] [Type Unit]:
                                Stmt 18 [99-123]: Local (Immutable):
                                    Pat 19 [103-110] [Type Int]: Bind: Ident 20 [103-104] "f"
                                    Expr 21 [113-122] [Type (Int -> Int)]: Expr Block: Block 42 [113-122] [Type (Int -> Int)]:
                                        Stmt 27 [117-118]: Local (Immutable):
                                            Pat 26 [117-118] [Type Int]: Bind: Ident 24 [117-118] "arg"
                                            Expr 23 [117-118] [Type Int]: Lit: Int(1)
                                        Stmt 43 [113-122]: Expr: Expr 44 [113-122] [Type (Int -> Int)]: Closure([24], 3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [113-122] (Internal):
                    Parent: 2
                    Callable 37 [113-122] (function):
                        name: Ident 38 [113-122] "<lambda>"
                        input: Pat 35 [113-122] [Type (Int, Int)]: Tuple:
                            Pat 36 [117-118] [Type Int]: Bind: Ident 34 [117-118] "arg"
                            Pat 30 [120-121] [Type Int]: Bind: Ident 29 [120-121] "hole"
                        output: Int
                        functors: empty set
                        body: SpecDecl 39 [113-122]: Impl:
                            Block 40 [113-122] [Type Int]:
                                Stmt 41 [113-122]: Expr: Expr 33 [113-122] [Type Int]: Call:
                                    Expr 22 [113-116] [Type ((Int, Int) -> Int)]: Var: Item 1
                                    Expr 32 [116-122] [Type (Int, Int)]: Tuple:
                                        Expr 25 [117-118] [Type Int]: Var: Local 34
                                        Expr 31 [120-121] [Type Int]: Var: Local 29
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn partial_app_hole_as_callee() {
    check_hir(
        indoc! {"
            namespace A {
                @EntryPoint()
                operation Main() : Result[] {
                    let f = _(_);
                    let res = f(4);
                    return [res];
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-141] (Public):
                    Namespace (Ident 21 [10-11] "A"): Item 1
                Item 1 [18-139] (Internal):
                    Parent: 0
                    EntryPoint
                    Callable 0 [36-139] (operation):
                        name: Ident 1 [46-50] "Main"
                        input: Pat 2 [50-52] [Type Unit]: Unit
                        output: Result[]
                        functors: empty set
                        body: SpecDecl 3 [36-139]: Impl:
                            Block 4 [64-139] [Type Result[]]:
                                Stmt 5 [74-87]: Local (Immutable):
                                    Pat 6 [78-79] [Type ?3]: Bind: Ident 7 [78-79] "f"
                                    Expr 8 [82-86] [Type ?3]: Call:
                                        Expr 9 [82-83] [Type ?1]: Hole
                                        Expr 10 [84-85] [Type ?2]: Hole
                                Stmt 11 [96-111]: Local (Immutable):
                                    Pat 12 [100-103] [Type Result]: Bind: Ident 13 [100-103] "res"
                                    Expr 14 [106-110] [Type Result]: Call:
                                        Expr 15 [106-107] [Type ?3]: Var: Local 7
                                        Expr 16 [108-109] [Type Int]: Lit: Int(4)
                                Stmt 17 [120-133]: Semi: Expr 18 [120-132] [Type Unit]: Return: Expr 19 [127-132] [Type Result[]]: Array:
                                    Expr 20 [128-131] [Type Result]: Var: Local 13
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn invalid_elided() {
    check_errors(
        indoc! {r#"
            namespace input {
                operation Foo() : Unit {
                    let ... = 3;
                }
            }
        "#},
        &expect![[r#"
            [
                InvalidElidedPat(
                    Span {
                        lo: 59,
                        hi: 62,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn invalid_spec_pat() {
    check_errors(
        indoc! {r#"
            namespace input {
                operation Foo() : Unit is Ctl {
                    body bar {}
                    controlled (foo, bar) {}
                }
            }
        "#},
        &expect![[r#"
            [
                InvalidSpecPat(
                    Span {
                        lo: 67,
                        hi: 70,
                    },
                ),
                InvalidSpecPat(
                    Span {
                        lo: 93,
                        hi: 103,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_measurement_attr_on_function_issues_error() {
    check_errors(
        indoc! {r#"
            namespace Test {
                @Measurement()
                function Foo(q: Qubit) : Result {
                    body intrinsic;
                }
            }
        "#},
        &expect![[r#"
            [
                InvalidAttrOnFunction(
                    "Measurement",
                    Span {
                        lo: 49,
                        hi: 52,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_reset_attr_on_function_issues_error() {
    check_errors(
        indoc! {r#"
            namespace Test {
                @Reset()
                function Foo(q: Qubit) : Unit {
                    body intrinsic;
                }
            }
        "#},
        &expect![[r#"
            [
                InvalidAttrOnFunction(
                    "Reset",
                    Span {
                        lo: 43,
                        hi: 46,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn item_docs() {
    check_hir(
        "/// This is a namespace.
        namespace A {
            /// This is a newtype.
            newtype Foo = ();

            /// This is a function.
            function Bar() : () {}

            /// This is an operation.
            operation Baz() : () {}
        }",
        &expect![[r#"
            Package:
                Item 0 [0-268] (Public):
                    Doc:
                        This is a namespace.
                    Namespace (Ident 11 [43-44] "A"): Item 1, Item 2, Item 3
                Item 1 [59-111] (Internal):
                    Parent: 0
                    Doc:
                        This is a newtype.
                    Type (Ident 0 [102-105] "Foo"): UDT [59-111]:
                        TyDef [108-110]: Field:
                            type: Unit
                Item 2 [125-183] (Internal):
                    Parent: 0
                    Doc:
                        This is a function.
                    Callable 1 [161-183] (function):
                        name: Ident 2 [170-173] "Bar"
                        input: Pat 3 [173-175] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [161-183]: Impl:
                            Block 5 [181-183]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [197-258] (Internal):
                    Parent: 0
                    Doc:
                        This is an operation.
                    Callable 6 [235-258] (operation):
                        name: Ident 7 [245-248] "Baz"
                        input: Pat 8 [248-250] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 9 [235-258]: Impl:
                            Block 10 [256-258]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn nested_params() {
    check_hir(
        "namespace Test { function Foo<'T>(f: 'T => ()) : () { } }",
        &expect![[r#"
            Package:
                Item 0 [0-57] (Public):
                    Namespace (Ident 6 [10-14] "Test"): Item 1
                Item 1 [17-55] (Internal):
                    Parent: 0
                    Callable 0 [17-55] (function):
                        name: Ident 1 [26-29] "Foo"
                        generics:
                            0: type 'T
                            1: functor (empty set)
                        input: Pat 2 [34-45] [Type (Param<"'T": 0> => Unit is Param<1>)]: Bind: Ident 3 [34-35] "f"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [17-55]: Impl:
                            Block 5 [52-55]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_with_invalid_free_variable() {
    check_hir(
        "namespace Test{ operation T(): Unit {body fakelocal{() => fakelocal;}}}",
        &expect![[r#"
            Package:
                Item 0 [0-71] (Public):
                    Namespace (Ident 17 [10-14] "Test"): Item 1
                Item 1 [16-70] (Internal):
                    Parent: 0
                    Callable 0 [16-70] (operation):
                        name: Ident 1 [26-27] "T"
                        input: Pat 2 [27-29] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [37-69]: Impl:
                            Block 4 [51-69] [Type Unit]:
                                Stmt 5 [52-68]: Semi: Expr 6 [52-67] [Type (Unit => Unit)]: Closure([9], 2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [52-67] (Internal):
                    Parent: 1
                    Callable 12 [52-67] (operation):
                        name: Ident 13 [52-67] "<lambda>"
                        input: Pat 11 [52-67] [Type (Unit,)]: Tuple:
                            Pat 7 [52-54] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 14 [58-67]: Impl:
                            Block 15 [58-67] [Type Unit]:
                                Stmt 16 [58-67]: Expr: Expr 8 [58-67] [Type Unit]: Var: Local 10
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_with_explicit_return_should_have_non_unit_output_type() {
    check_hir(
        "function Foo(): Unit { let f = i -> return i + 1;}",
        &expect![[r#"
            Package:
                Item 0 [0-50] (Public):
                    Namespace (Ident 21 [0-50] "test"): Item 1
                Item 1 [0-50] (Internal):
                    Parent: 0
                    Callable 0 [0-50] (function):
                        name: Ident 1 [9-12] "Foo"
                        input: Pat 2 [12-14] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-50]: Impl:
                            Block 4 [21-50] [Type Unit]:
                                Stmt 5 [23-49]: Local (Immutable):
                                    Pat 6 [27-28] [Type (Int -> Int)]: Bind: Ident 7 [27-28] "f"
                                    Expr 8 [31-48] [Type (Int -> Int)]: Closure([], 2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [31-48] (Internal):
                    Parent: 1
                    Callable 16 [31-48] (function):
                        name: Ident 17 [31-48] "<lambda>"
                        input: Pat 15 [31-48] [Type (Int,)]: Tuple:
                            Pat 9 [31-32] [Type Int]: Bind: Ident 10 [31-32] "i"
                        output: Int
                        functors: empty set
                        body: SpecDecl 18 [36-48]: Impl:
                            Block 19 [36-48] [Type Int]:
                                Stmt 20 [36-48]: Expr: Expr 11 [36-48] [Type Int]: Return: Expr 12 [43-48] [Type Int]: BinOp (Add):
                                    Expr 13 [43-44] [Type Int]: Var: Local 10
                                    Expr 14 [47-48] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn lambda_with_implicit_return_should_have_non_unit_output_type() {
    check_hir(
        "function Foo(): Unit { let f = i -> i + 1}",
        &expect![[r#"
            Package:
                Item 0 [0-42] (Public):
                    Namespace (Ident 20 [0-42] "test"): Item 1
                Item 1 [0-42] (Internal):
                    Parent: 0
                    Callable 0 [0-42] (function):
                        name: Ident 1 [9-12] "Foo"
                        input: Pat 2 [12-14] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-42]: Impl:
                            Block 4 [21-42] [Type Unit]:
                                Stmt 5 [23-41]: Local (Immutable):
                                    Pat 6 [27-28] [Type (Int -> Int)]: Bind: Ident 7 [27-28] "f"
                                    Expr 8 [31-41] [Type (Int -> Int)]: Closure([], 2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [31-41] (Internal):
                    Parent: 1
                    Callable 15 [31-41] (function):
                        name: Ident 16 [31-41] "<lambda>"
                        input: Pat 14 [31-41] [Type (Int,)]: Tuple:
                            Pat 9 [31-32] [Type Int]: Bind: Ident 10 [31-32] "i"
                        output: Int
                        functors: empty set
                        body: SpecDecl 17 [36-41]: Impl:
                            Block 18 [36-41] [Type Int]:
                                Stmt 19 [36-41]: Expr: Expr 11 [36-41] [Type Int]: BinOp (Add):
                                    Expr 12 [36-37] [Type Int]: Var: Local 10
                                    Expr 13 [40-41] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn duplicate_commas_in_tydef() {
    check_hir(
        indoc! {r#"
            namespace test {
                newtype Foo = (Int,,);
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-45] (Public):
                    Namespace (Ident 1 [10-14] "test"): Item 1
                Item 1 [21-43] (Internal):
                    Parent: 0
                    Type (Ident 0 [29-32] "Foo"): UDT [21-43]:
                        TyDef [35-42]: Tuple:
                            TyDef [36-39]: Field:
                                type: Int
                            TyDef [40-40]: Field:
                                type: ?"#]],
    );
}

#[test]
fn duplicate_commas_in_generics() {
    check_hir(
        indoc! {r#"
            namespace test {
                function Foo<'T,,>(x : 'T) : Unit {}
            }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-59] (Public):
                    Namespace (Ident 6 [10-14] "test"): Item 1
                Item 1 [21-57] (Internal):
                    Parent: 0
                    Callable 0 [21-57] (function):
                        name: Ident 1 [30-33] "Foo"
                        generics:
                            0: type 'T
                            1: type 
                        input: Pat 2 [40-46] [Type Param<"'T": 0>]: Bind: Ident 3 [40-41] "x"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [21-57]: Impl:
                            Block 5 [55-57]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn duplicate_commas_in_pat() {
    check_hir(
        indoc! {r#"
            namespace test {
                operation Foo() : Unit {
                    let (x,,) = (1, 2);
                }
            }"#},
        &expect![[r#"
            Package:
                Item 0 [0-81] (Public):
                    Namespace (Ident 13 [10-14] "test"): Item 1
                Item 1 [21-79] (Internal):
                    Parent: 0
                    Callable 0 [21-79] (operation):
                        name: Ident 1 [31-34] "Foo"
                        input: Pat 2 [34-36] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-79]: Impl:
                            Block 4 [44-79] [Type Unit]:
                                Stmt 5 [54-73]: Local (Immutable):
                                    Pat 6 [58-63] [Type (Int, ?)]: Tuple:
                                        Pat 7 [59-60] [Type Int]: Bind: Ident 8 [59-60] "x"
                                        Pat 9 [61-61] [Type ?]: Err
                                    Expr 10 [66-72] [Type (Int, Int)]: Tuple:
                                        Expr 11 [67-68] [Type Int]: Lit: Int(1)
                                        Expr 12 [70-71] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn duplicate_commas_in_tuple() {
    check_hir(
        indoc! {r#"
            namespace test {
                operation Foo() : Unit {
                    let x = (1,,3);
                }
            }"#},
        &expect![[r#"
            Package:
                Item 0 [0-77] (Public):
                    Namespace (Ident 12 [10-14] "test"): Item 1
                Item 1 [21-75] (Internal):
                    Parent: 0
                    Callable 0 [21-75] (operation):
                        name: Ident 1 [31-34] "Foo"
                        input: Pat 2 [34-36] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-75]: Impl:
                            Block 4 [44-75] [Type Unit]:
                                Stmt 5 [54-69]: Local (Immutable):
                                    Pat 6 [58-59] [Type (Int, ?, Int)]: Bind: Ident 7 [58-59] "x"
                                    Expr 8 [62-68] [Type (Int, ?, Int)]: Tuple:
                                        Expr 9 [63-64] [Type Int]: Lit: Int(1)
                                        Expr 10 [65-65] [Type ?]: Err
                                        Expr 11 [66-67] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn duplicate_commas_in_arg_tuple() {
    check_hir(
        indoc! {r#"
            namespace test {
                operation Foo(a : Int, b : Int, c : Int) : Int {
                    Foo(a,,c)
                }
            }"#},
        &expect![[r#"
            Package:
                Item 0 [0-95] (Public):
                    Namespace (Ident 18 [10-14] "test"): Item 1
                Item 1 [21-93] (Internal):
                    Parent: 0
                    Callable 0 [21-93] (operation):
                        name: Ident 1 [31-34] "Foo"
                        input: Pat 2 [34-61] [Type (Int, Int, Int)]: Tuple:
                            Pat 3 [35-42] [Type Int]: Bind: Ident 4 [35-36] "a"
                            Pat 5 [44-51] [Type Int]: Bind: Ident 6 [44-45] "b"
                            Pat 7 [53-60] [Type Int]: Bind: Ident 8 [53-54] "c"
                        output: Int
                        functors: empty set
                        body: SpecDecl 9 [21-93]: Impl:
                            Block 10 [68-93] [Type Int]:
                                Stmt 11 [78-87]: Expr: Expr 12 [78-87] [Type Int]: Call:
                                    Expr 13 [78-81] [Type ((Int, Int, Int) => Int)]: Var: Item 1
                                    Expr 14 [81-87] [Type (Int, ?, Int)]: Tuple:
                                        Expr 15 [82-83] [Type Int]: Var: Local 4
                                        Expr 16 [84-84] [Type ?]: Err
                                        Expr 17 [85-86] [Type Int]: Var: Local 8
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn ignore_item_in_attribute() {
    check_hir(
        "namespace Test {
            @Attr{function Bar() : Unit { Bar }}
            function Foo() : Unit {}
        }",
        &expect![[r#"
            Package:
                Item 0 [0-112] (Public):
                    Namespace (Ident 5 [10-14] "Test"): Item 1
                Item 1 [29-102] (Internal):
                    Parent: 0
                    Callable 0 [78-102] (function):
                        name: Ident 1 [87-90] "Foo"
                        input: Pat 2 [90-92] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [78-102]: Impl:
                            Block 4 [100-102]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn self_export_makes_callable_public() {
    // Export item should vanish during lowering, and the original
    // callable should be marked public
    check_hir(
        indoc! {"
            namespace Test {
                function Length() : Int { 42 }
                export Length;
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-72] (Public):
                    Namespace (Ident 8 [10-14] "Test"): Item 1
                Item 1 [21-51] (Public):
                    Parent: 0
                    Callable 0 [21-51] (function):
                        name: Ident 1 [30-36] "Length"
                        input: Pat 2 [36-38] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [21-51]: Impl:
                            Block 4 [45-51] [Type Int]:
                                Stmt 5 [47-49]: Expr: Expr 6 [47-49] [Type Int]: Lit: Int(42)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn self_export_makes_ty_public() {
    // Export item should vanish during lowering, and the original
    // UDT should be marked public
    check_hir(
        indoc! {"
            namespace Foo {
                struct Bar { x: Int }
                export Bar;
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-59] (Public):
                    Namespace (Ident 2 [10-13] "Foo"): Item 1
                Item 1 [20-41] (Public):
                    Parent: 0
                    Type (Ident 0 [27-30] "Bar"): UDT [20-41]:
                        TyDef [20-41]: Tuple:
                            TyDef [33-39]: Field:
                                name: x [33-34]
                                type: Int"#]],
    );
}

#[test]
fn export_in_different_namespace_creates_export_item() {
    // An export item should be created, and the original callable should
    // remain internal.
    check_hir(
        indoc! {"
            namespace Foo {
                function Bar() : Int { 1 }
            }
            namespace Test {
                export Foo.Bar;
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-48] (Public):
                    Namespace (Ident 7 [10-13] "Foo"): Item 1
                Item 1 [20-46] (Internal):
                    Parent: 0
                    Callable 0 [20-46] (function):
                        name: Ident 1 [29-32] "Bar"
                        input: Pat 2 [32-34] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [20-46]: Impl:
                            Block 4 [41-46] [Type Int]:
                                Stmt 5 [43-44]: Expr: Expr 6 [43-44] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [49-87] (Public):
                    Namespace (Ident 9 [59-63] "Test"): Item 3
                Item 3 [70-85] (Public):
                    Parent: 2
                    Export (Ident 8 [81-84] "Bar"): Item 1"#]],
    );
}

#[test]
fn wildcard_import_export_in_different_namespace_creates_export_item() {
    // An export item should be created, and the original callable should
    // remain internal.
    check_hir(
        indoc! {"
            namespace Foo {
                function Bar() : Unit {}
            }
            namespace Test {
                import Foo.*;
                export Bar;
                function Test() : Unit {
                    Bar();
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-46] (Public):
                    Namespace (Ident 5 [10-13] "Foo"): Item 1
                Item 1 [20-44] (Internal):
                    Parent: 0
                    Callable 0 [20-44] (function):
                        name: Ident 1 [29-32] "Bar"
                        input: Pat 2 [32-34] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [20-44]: Impl:
                            Block 4 [42-44]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [47-149] (Public):
                    Namespace (Ident 16 [57-61] "Test"): Item 3, Item 4
                Item 3 [86-97] (Public):
                    Parent: 2
                    Export (Ident 6 [93-96] "Bar"): Item 1
                Item 4 [102-147] (Internal):
                    Parent: 2
                    Callable 7 [102-147] (function):
                        name: Ident 8 [111-115] "Test"
                        input: Pat 9 [115-117] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 10 [102-147]: Impl:
                            Block 11 [125-147] [Type Unit]:
                                Stmt 12 [135-141]: Semi: Expr 13 [135-140] [Type Unit]: Call:
                                    Expr 14 [135-138] [Type (Unit -> Unit)]: Var: Item 1
                                    Expr 15 [138-140] [Type Unit]: Unit
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn aliased_export_creates_export_item() {
    // An export item should be created, and the original callable should
    // remain internal.
    check_hir(
        indoc! {"
            namespace Foo {
                function Bar() : Int { 1 }
            }
            namespace Test {
                export Foo.Bar as Baz;
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-48] (Public):
                    Namespace (Ident 7 [10-13] "Foo"): Item 1
                Item 1 [20-46] (Internal):
                    Parent: 0
                    Callable 0 [20-46] (function):
                        name: Ident 1 [29-32] "Bar"
                        input: Pat 2 [32-34] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [20-46]: Impl:
                            Block 4 [41-46] [Type Int]:
                                Stmt 5 [43-44]: Expr: Expr 6 [43-44] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [49-94] (Public):
                    Namespace (Ident 9 [59-63] "Test"): Item 3
                Item 3 [70-92] (Public):
                    Parent: 2
                    Export (Ident 8 [88-91] "Baz"): Item 1"#]],
    );
}

#[test]
fn export_parent_namespace_is_err() {
    // A namespace that's only ever declared as a parent (e.g.
    // the `Parent` in `Parent.Foo`) cannot be exported since we don't
    // assign it an Item ID.
    let input = indoc! {"
            namespace Parent.Foo {
                function Bar() : Unit {}
                export Bar;
            }

            namespace Baz {
                export Parent as ParentAlias;
            }

            namespace Main {
                export Baz.ParentAlias;
                operation Main() : Unit {}
            }
        "};
    check_errors(
        input,
        &expect![[r#"
            [
                ParentNamespaceExport {
                    span: Span {
                        lo: 98,
                        hi: 104,
                    },
                },
                ParentNamespaceExport {
                    span: Span {
                        lo: 152,
                        hi: 167,
                    },
                },
            ]
        "#]],
    );
    check_hir(
        input,
        &expect![[r#"
            Package:
                Item 0 [0-69] (Public):
                    Namespace ([Ident 6 [10-16] "Parent", Ident 7 [17-20] "Foo"]): Item 1
                Item 1 [27-51] (Public):
                    Parent: 0
                    Callable 0 [27-51] (function):
                        name: Ident 1 [36-39] "Bar"
                        input: Pat 2 [39-41] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [27-51]: Impl:
                            Block 4 [49-51]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 3 [71-122] (Public):
                    Namespace (Ident 9 [81-84] "Baz"): Item 4
                Item 4 [91-120] (Public):
                    Parent: 3
                    Export (Ident 8 [108-119] "ParentAlias"): Err
                Item 5 [124-201] (Public):
                    Namespace (Ident 16 [134-138] "Main"): Item 6, Item 7
                Item 6 [145-168] (Public):
                    Parent: 5
                    Export (Ident 10 [156-167] "ParentAlias"): Err
                Item 7 [173-199] (Internal):
                    Parent: 5
                    Callable 11 [173-199] (operation):
                        name: Ident 12 [183-187] "Main"
                        input: Pat 13 [187-189] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 14 [173-199]: Impl:
                            Block 15 [197-199]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn literal_complex_lowers_as_struct_decl() {
    check_hir(
        indoc! {"
            function Imaginary4() : Complex {
                4.0i
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-44] (Public):
                    Namespace (Ident 11 [0-44] "test"): Item 1
                Item 1 [0-44] (Internal):
                    Parent: 0
                    Callable 0 [0-44] (function):
                        name: Ident 1 [9-19] "Imaginary4"
                        input: Pat 2 [19-21] [Type Unit]: Unit
                        output: UDT<"Complex": Item 3 (Package 0)>
                        functors: empty set
                        body: SpecDecl 3 [0-44]: Impl:
                            Block 4 [32-44] [Type UDT<"Complex": Item 3 (Package 0)>]:
                                Stmt 5 [38-42]: Expr: Expr 6 [38-42] [Type UDT<"Complex": Item 3 (Package 0)>]: Struct (Item 3 (Package 0)):
                                    FieldsAssign 7 [0-0]: (Path([0])) Expr 8 [0-0] [Type Double]: Lit: Double(0)
                                    FieldsAssign 9 [0-0]: (Path([1])) Expr 10 [0-0] [Type Double]: Lit: Double(4)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}
