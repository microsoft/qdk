// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
use qsc_frontend::compile::{self, PackageStore, SourceMap, compile};
use qsc_hir::{mut_visit::MutVisitor, validate::Validator, visit::Visitor};

use crate::loop_unification::LoopUni;

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
    LoopUni {
        core: store.core(),
        assigner: &mut unit.assigner,
    }
    .visit_package(&mut unit.package);
    Validator::default().visit_package(&unit.package);
    expect.assert_eq(&unit.package.to_string());
}

#[test]
fn convert_for_array() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                for i in arr {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-133] (Public):
                    Namespace (Ident 16 [10-14] "test"): Item 1
                Item 1 [21-131] (Internal):
                    Parent: 0
                    Callable 0 [21-131] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [36-47] [Type Int[]]: Bind: Ident 3 [36-39] "arr"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [21-131]: Impl:
                            Block 5 [56-131] [Type Unit]:
                                Stmt 6 [66-125]: Expr: Expr 43 [66-125] [Type Unit]: Expr Block: Block 44 [66-125] [Type Unit]:
                                    Stmt 18 [0-0]: Local (Immutable):
                                        Pat 19 [75-78] [Type Int[]]: Bind: Ident 17 [75-78] "@array_id_17"
                                        Expr 10 [75-78] [Type Int[]]: Var: Local 3
                                    Stmt 24 [0-0]: Local (Immutable):
                                        Pat 25 [75-78] [Type Int]: Bind: Ident 21 [75-78] "@len_id_21"
                                        Expr 22 [75-78] [Type Int]: Call:
                                            Expr 20 [75-78] [Type (Int[] -> Int)]: Var:
                                                res: Item 1 (Package 0)
                                                generics:
                                                    Int
                                            Expr 23 [75-78] [Type Int[]]: Var: Local 17
                                    Stmt 28 [75-78]: Local (Mutable):
                                        Pat 29 [75-78] [Type Int]: Bind: Ident 26 [75-78] "@index_id_26"
                                        Expr 27 [75-78] [Type Int]: Lit: Int(0)
                                    Stmt 41 [0-0]: Expr: Expr 42 [66-125] [Type Unit]: While:
                                        Expr 38 [75-78] [Type Bool]: BinOp (Lt):
                                            Expr 39 [75-78] [Type Int]: Var: Local 26
                                            Expr 40 [75-78] [Type Int]: Var: Local 21
                                        Block 11 [79-125] [Type Unit]:
                                            Stmt 30 [70-71]: Local (Immutable):
                                                Pat 8 [70-71] [Type Int]: Bind: Ident 9 [70-71] "i"
                                                Expr 31 [75-78] [Type Int]: Index:
                                                    Expr 32 [75-78] [Type Int[]]: Var: Local 17
                                                    Expr 33 [75-78] [Type Int]: Var: Local 26
                                            Stmt 12 [93-115]: Local (Immutable):
                                                Pat 13 [97-98] [Type String]: Bind: Ident 14 [97-98] "x"
                                                Expr 15 [101-114] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 35 [75-78]: Semi: Expr 36 [75-78] [Type Unit]: AssignOp (Add):
                                                Expr 37 [75-78] [Type Int]: Var: Local 26
                                                Expr 34 [75-78] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_for_array_deconstruct() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : (Int, Double)[]) : Unit {
                for (i, d) in arr {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-148] (Public):
                    Namespace (Ident 19 [10-14] "test"): Item 1
                Item 1 [21-146] (Internal):
                    Parent: 0
                    Callable 0 [21-146] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [36-57] [Type (Int, Double)[]]: Bind: Ident 3 [36-39] "arr"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [21-146]: Impl:
                            Block 5 [66-146] [Type Unit]:
                                Stmt 6 [76-140]: Expr: Expr 46 [76-140] [Type Unit]: Expr Block: Block 47 [76-140] [Type Unit]:
                                    Stmt 21 [0-0]: Local (Immutable):
                                        Pat 22 [90-93] [Type (Int, Double)[]]: Bind: Ident 20 [90-93] "@array_id_20"
                                        Expr 13 [90-93] [Type (Int, Double)[]]: Var: Local 3
                                    Stmt 27 [0-0]: Local (Immutable):
                                        Pat 28 [90-93] [Type Int]: Bind: Ident 24 [90-93] "@len_id_24"
                                        Expr 25 [90-93] [Type Int]: Call:
                                            Expr 23 [90-93] [Type ((Int, Double)[] -> Int)]: Var:
                                                res: Item 1 (Package 0)
                                                generics:
                                                    (Int, Double)
                                            Expr 26 [90-93] [Type (Int, Double)[]]: Var: Local 20
                                    Stmt 31 [90-93]: Local (Mutable):
                                        Pat 32 [90-93] [Type Int]: Bind: Ident 29 [90-93] "@index_id_29"
                                        Expr 30 [90-93] [Type Int]: Lit: Int(0)
                                    Stmt 44 [0-0]: Expr: Expr 45 [76-140] [Type Unit]: While:
                                        Expr 41 [90-93] [Type Bool]: BinOp (Lt):
                                            Expr 42 [90-93] [Type Int]: Var: Local 29
                                            Expr 43 [90-93] [Type Int]: Var: Local 24
                                        Block 14 [94-140] [Type Unit]:
                                            Stmt 33 [80-86]: Local (Immutable):
                                                Pat 8 [80-86] [Type (Int, Double)]: Tuple:
                                                    Pat 9 [81-82] [Type Int]: Bind: Ident 10 [81-82] "i"
                                                    Pat 11 [84-85] [Type Double]: Bind: Ident 12 [84-85] "d"
                                                Expr 34 [90-93] [Type (Int, Double)]: Index:
                                                    Expr 35 [90-93] [Type (Int, Double)[]]: Var: Local 20
                                                    Expr 36 [90-93] [Type Int]: Var: Local 29
                                            Stmt 15 [108-130]: Local (Immutable):
                                                Pat 16 [112-113] [Type String]: Bind: Ident 17 [112-113] "x"
                                                Expr 18 [116-129] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 38 [90-93]: Semi: Expr 39 [90-93] [Type Unit]: AssignOp (Add):
                                                Expr 40 [90-93] [Type Int]: Var: Local 29
                                                Expr 37 [90-93] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_for_slice() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                for i in arr[6..-2..2] {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-143] (Public):
                    Namespace (Ident 22 [10-14] "test"): Item 1
                Item 1 [21-141] (Internal):
                    Parent: 0
                    Callable 0 [21-141] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [36-47] [Type Int[]]: Bind: Ident 3 [36-39] "arr"
                        output: Unit
                        functors: empty set
                        body: SpecDecl 4 [21-141]: Impl:
                            Block 5 [56-141] [Type Unit]:
                                Stmt 6 [66-135]: Expr: Expr 49 [66-135] [Type Unit]: Expr Block: Block 50 [66-135] [Type Unit]:
                                    Stmt 24 [0-0]: Local (Immutable):
                                        Pat 25 [75-88] [Type Int[]]: Bind: Ident 23 [75-88] "@array_id_23"
                                        Expr 10 [75-88] [Type Int[]]: Index:
                                            Expr 11 [75-78] [Type Int[]]: Var: Local 3
                                            Expr 12 [79-87] [Type Range]: Range:
                                                Expr 13 [79-80] [Type Int]: Lit: Int(6)
                                                Expr 14 [82-84] [Type Int]: UnOp (Neg):
                                                    Expr 15 [83-84] [Type Int]: Lit: Int(2)
                                                Expr 16 [86-87] [Type Int]: Lit: Int(2)
                                    Stmt 30 [0-0]: Local (Immutable):
                                        Pat 31 [75-88] [Type Int]: Bind: Ident 27 [75-88] "@len_id_27"
                                        Expr 28 [75-88] [Type Int]: Call:
                                            Expr 26 [75-88] [Type (Int[] -> Int)]: Var:
                                                res: Item 1 (Package 0)
                                                generics:
                                                    Int
                                            Expr 29 [75-88] [Type Int[]]: Var: Local 23
                                    Stmt 34 [75-88]: Local (Mutable):
                                        Pat 35 [75-88] [Type Int]: Bind: Ident 32 [75-88] "@index_id_32"
                                        Expr 33 [75-88] [Type Int]: Lit: Int(0)
                                    Stmt 47 [0-0]: Expr: Expr 48 [66-135] [Type Unit]: While:
                                        Expr 44 [75-88] [Type Bool]: BinOp (Lt):
                                            Expr 45 [75-88] [Type Int]: Var: Local 32
                                            Expr 46 [75-88] [Type Int]: Var: Local 27
                                        Block 17 [89-135] [Type Unit]:
                                            Stmt 36 [70-71]: Local (Immutable):
                                                Pat 8 [70-71] [Type Int]: Bind: Ident 9 [70-71] "i"
                                                Expr 37 [75-88] [Type Int]: Index:
                                                    Expr 38 [75-88] [Type Int[]]: Var: Local 23
                                                    Expr 39 [75-88] [Type Int]: Var: Local 32
                                            Stmt 18 [103-125]: Local (Immutable):
                                                Pat 19 [107-108] [Type String]: Bind: Ident 20 [107-108] "x"
                                                Expr 21 [111-124] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 41 [75-88]: Semi: Expr 42 [75-88] [Type Unit]: AssignOp (Add):
                                                Expr 43 [75-88] [Type Int]: Var: Local 32
                                                Expr 40 [75-88] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_for_range() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                for i in 0..4 {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-123] (Public):
                    Namespace (Ident 17 [10-14] "test"): Item 1
                Item 1 [21-121] (Internal):
                    Parent: 0
                    Callable 0 [21-121] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [35-37] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-121]: Impl:
                            Block 4 [45-121] [Type Unit]:
                                Stmt 5 [55-115]: Expr: Expr 59 [55-115] [Type Unit]: Expr Block: Block 60 [55-115] [Type Unit]:
                                    Stmt 19 [0-0]: Local (Immutable):
                                        Pat 20 [64-68] [Type Range]: Bind: Ident 18 [64-68] "@range_id_18"
                                        Expr 9 [64-68] [Type Range]: Range:
                                            Expr 10 [64-65] [Type Int]: Lit: Int(0)
                                            <no step>
                                            Expr 11 [67-68] [Type Int]: Lit: Int(4)
                                    Stmt 24 [64-68]: Local (Mutable):
                                        Pat 25 [64-68] [Type Int]: Bind: Ident 21 [64-68] "@index_id_21"
                                        Expr 22 [64-68] [Type Int]: Field:
                                            Expr 23 [64-68] [Type Range]: Var: Local 18
                                            Prim(Start)
                                    Stmt 29 [0-0]: Local (Immutable):
                                        Pat 30 [64-68] [Type Int]: Bind: Ident 26 [64-68] "@step_id_26"
                                        Expr 27 [64-68] [Type Int]: Field:
                                            Expr 28 [64-68] [Type Range]: Var: Local 18
                                            Prim(Step)
                                    Stmt 34 [0-0]: Local (Immutable):
                                        Pat 35 [64-68] [Type Int]: Bind: Ident 31 [64-68] "@end_id_31"
                                        Expr 32 [64-68] [Type Int]: Field:
                                            Expr 33 [64-68] [Type Range]: Var: Local 18
                                            Prim(End)
                                    Stmt 57 [0-0]: Expr: Expr 58 [55-115] [Type Unit]: While:
                                        Expr 42 [64-68] [Type Bool]: BinOp (OrL):
                                            Expr 43 [64-68] [Type Bool]: BinOp (AndL):
                                                Expr 44 [64-68] [Type Bool]: BinOp (Gt):
                                                    Expr 45 [64-68] [Type Int]: Var: Local 26
                                                    Expr 46 [64-68] [Type Int]: Lit: Int(0)
                                                Expr 47 [64-68] [Type Bool]: BinOp (Lte):
                                                    Expr 48 [64-68] [Type Int]: Var: Local 21
                                                    Expr 49 [64-68] [Type Int]: Var: Local 31
                                            Expr 50 [64-68] [Type Bool]: BinOp (AndL):
                                                Expr 51 [64-68] [Type Bool]: BinOp (Lt):
                                                    Expr 52 [64-68] [Type Int]: Var: Local 26
                                                    Expr 53 [64-68] [Type Int]: Lit: Int(0)
                                                Expr 54 [64-68] [Type Bool]: BinOp (Gte):
                                                    Expr 55 [64-68] [Type Int]: Var: Local 21
                                                    Expr 56 [64-68] [Type Int]: Var: Local 31
                                        Block 12 [69-115] [Type Unit]:
                                            Stmt 36 [59-60]: Local (Immutable):
                                                Pat 7 [59-60] [Type Int]: Bind: Ident 8 [59-60] "i"
                                                Expr 37 [64-68] [Type Int]: Var: Local 21
                                            Stmt 13 [83-105]: Local (Immutable):
                                                Pat 14 [87-88] [Type String]: Bind: Ident 15 [87-88] "x"
                                                Expr 16 [91-104] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 39 [64-68]: Semi: Expr 40 [64-68] [Type Unit]: AssignOp (Add):
                                                Expr 41 [64-68] [Type Int]: Var: Local 21
                                                Expr 38 [64-68] [Type Int]: Var: Local 26
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_for_reverse_range() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                for i in 4..-1..0 {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-127] (Public):
                    Namespace (Ident 19 [10-14] "test"): Item 1
                Item 1 [21-125] (Internal):
                    Parent: 0
                    Callable 0 [21-125] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [35-37] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-125]: Impl:
                            Block 4 [45-125] [Type Unit]:
                                Stmt 5 [55-119]: Expr: Expr 61 [55-119] [Type Unit]: Expr Block: Block 62 [55-119] [Type Unit]:
                                    Stmt 21 [0-0]: Local (Immutable):
                                        Pat 22 [64-72] [Type Range]: Bind: Ident 20 [64-72] "@range_id_20"
                                        Expr 9 [64-72] [Type Range]: Range:
                                            Expr 10 [64-65] [Type Int]: Lit: Int(4)
                                            Expr 11 [67-69] [Type Int]: UnOp (Neg):
                                                Expr 12 [68-69] [Type Int]: Lit: Int(1)
                                            Expr 13 [71-72] [Type Int]: Lit: Int(0)
                                    Stmt 26 [64-72]: Local (Mutable):
                                        Pat 27 [64-72] [Type Int]: Bind: Ident 23 [64-72] "@index_id_23"
                                        Expr 24 [64-72] [Type Int]: Field:
                                            Expr 25 [64-72] [Type Range]: Var: Local 20
                                            Prim(Start)
                                    Stmt 31 [0-0]: Local (Immutable):
                                        Pat 32 [64-72] [Type Int]: Bind: Ident 28 [64-72] "@step_id_28"
                                        Expr 29 [64-72] [Type Int]: Field:
                                            Expr 30 [64-72] [Type Range]: Var: Local 20
                                            Prim(Step)
                                    Stmt 36 [0-0]: Local (Immutable):
                                        Pat 37 [64-72] [Type Int]: Bind: Ident 33 [64-72] "@end_id_33"
                                        Expr 34 [64-72] [Type Int]: Field:
                                            Expr 35 [64-72] [Type Range]: Var: Local 20
                                            Prim(End)
                                    Stmt 59 [0-0]: Expr: Expr 60 [55-119] [Type Unit]: While:
                                        Expr 44 [64-72] [Type Bool]: BinOp (OrL):
                                            Expr 45 [64-72] [Type Bool]: BinOp (AndL):
                                                Expr 46 [64-72] [Type Bool]: BinOp (Gt):
                                                    Expr 47 [64-72] [Type Int]: Var: Local 28
                                                    Expr 48 [64-72] [Type Int]: Lit: Int(0)
                                                Expr 49 [64-72] [Type Bool]: BinOp (Lte):
                                                    Expr 50 [64-72] [Type Int]: Var: Local 23
                                                    Expr 51 [64-72] [Type Int]: Var: Local 33
                                            Expr 52 [64-72] [Type Bool]: BinOp (AndL):
                                                Expr 53 [64-72] [Type Bool]: BinOp (Lt):
                                                    Expr 54 [64-72] [Type Int]: Var: Local 28
                                                    Expr 55 [64-72] [Type Int]: Lit: Int(0)
                                                Expr 56 [64-72] [Type Bool]: BinOp (Gte):
                                                    Expr 57 [64-72] [Type Int]: Var: Local 23
                                                    Expr 58 [64-72] [Type Int]: Var: Local 33
                                        Block 14 [73-119] [Type Unit]:
                                            Stmt 38 [59-60]: Local (Immutable):
                                                Pat 7 [59-60] [Type Int]: Bind: Ident 8 [59-60] "i"
                                                Expr 39 [64-72] [Type Int]: Var: Local 23
                                            Stmt 15 [87-109]: Local (Immutable):
                                                Pat 16 [91-92] [Type String]: Bind: Ident 17 [91-92] "x"
                                                Expr 18 [95-108] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 41 [64-72]: Semi: Expr 42 [64-72] [Type Unit]: AssignOp (Add):
                                                Expr 43 [64-72] [Type Int]: Var: Local 23
                                                Expr 40 [64-72] [Type Int]: Var: Local 28
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_repeat() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                repeat {
                    let x = "Hello World";
                } until true;
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-128] (Public):
                    Namespace (Ident 13 [10-14] "test"): Item 1
                Item 1 [21-126] (Internal):
                    Parent: 0
                    Callable 0 [21-126] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [35-37] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-126]: Impl:
                            Block 4 [45-126] [Type Unit]:
                                Stmt 5 [55-120]: Semi: Expr 26 [55-119] [Type Unit]: Expr Block: Block 22 [55-119] [Type Unit]:
                                    Stmt 16 [0-0]: Local (Mutable):
                                        Pat 17 [115-119] [Type Bool]: Bind: Ident 14 [115-119] "@continue_cond_14"
                                        Expr 15 [115-119] [Type Bool]: Lit: Bool(true)
                                    Stmt 23 [0-0]: Expr: Expr 24 [55-119] [Type Unit]: While:
                                        Expr 25 [115-119] [Type Bool]: Var: Local 14
                                        Block 7 [62-108] [Type Unit]:
                                            Stmt 8 [76-98]: Local (Immutable):
                                                Pat 9 [80-81] [Type String]: Bind: Ident 10 [80-81] "x"
                                                Expr 11 [84-97] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 18 [115-119]: Semi: Expr 19 [115-119] [Type Unit]: Assign:
                                                Expr 20 [115-119] [Type Bool]: Var: Local 14
                                                Expr 21 [115-119] [Type Bool]: UnOp (NotL):
                                                    Expr 12 [115-119] [Type Bool]: Lit: Bool(true)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_repeat_fixup() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                repeat {
                    let x = "Hello World";
                } until true
                fixup {
                    let y = "Fixup";
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-182] (Public):
                    Namespace (Ident 18 [10-14] "test"): Item 1
                Item 1 [21-180] (Internal):
                    Parent: 0
                    Callable 0 [21-180] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [35-37] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-180]: Impl:
                            Block 4 [45-180] [Type Unit]:
                                Stmt 5 [55-174]: Expr: Expr 35 [55-174] [Type Unit]: Expr Block: Block 31 [55-174] [Type Unit]:
                                    Stmt 21 [0-0]: Local (Mutable):
                                        Pat 22 [115-119] [Type Bool]: Bind: Ident 19 [115-119] "@continue_cond_19"
                                        Expr 20 [115-119] [Type Bool]: Lit: Bool(true)
                                    Stmt 32 [0-0]: Expr: Expr 33 [55-174] [Type Unit]: While:
                                        Expr 34 [115-119] [Type Bool]: Var: Local 19
                                        Block 7 [62-108] [Type Unit]:
                                            Stmt 8 [76-98]: Local (Immutable):
                                                Pat 9 [80-81] [Type String]: Bind: Ident 10 [80-81] "x"
                                                Expr 11 [84-97] [Type String]: String:
                                                    Lit: "Hello World"
                                            Stmt 23 [115-119]: Semi: Expr 24 [115-119] [Type Unit]: Assign:
                                                Expr 25 [115-119] [Type Bool]: Var: Local 19
                                                Expr 26 [115-119] [Type Bool]: UnOp (NotL):
                                                    Expr 12 [115-119] [Type Bool]: Lit: Bool(true)
                                            Stmt 27 [134-174]: Expr: Expr 28 [134-174] [Type Unit]: If:
                                                Expr 29 [115-119] [Type Bool]: Var: Local 19
                                                Expr 30 [134-174] [Type Unit]: Expr Block: Block 13 [134-174] [Type Unit]:
                                                    Stmt 14 [148-164]: Local (Immutable):
                                                        Pat 15 [152-153] [Type String]: Bind: Ident 16 [152-153] "y"
                                                        Expr 17 [156-163] [Type String]: String:
                                                            Lit: "Fixup"
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_repeat_nested() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                let a = true;
                let b = false;
                let c = true;
                repeat {
                    repeat {
                        let x = "First";
                    } until a
                    fixup {
                        let y = "Second";
                    }
                } until b
                fixup {
                    repeat {
                        let z = "Third";
                    } until c;
                }
            }
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-403] (Public):
                    Namespace (Ident 43 [10-14] "test"): Item 1
                Item 1 [21-401] (Internal):
                    Parent: 0
                    Callable 0 [21-401] (operation):
                        name: Ident 1 [31-35] "Main"
                        input: Pat 2 [35-37] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [21-401]: Impl:
                            Block 4 [45-401] [Type Unit]:
                                Stmt 5 [55-68]: Local (Immutable):
                                    Pat 6 [59-60] [Type Bool]: Bind: Ident 7 [59-60] "a"
                                    Expr 8 [63-67] [Type Bool]: Lit: Bool(true)
                                Stmt 9 [77-91]: Local (Immutable):
                                    Pat 10 [81-82] [Type Bool]: Bind: Ident 11 [81-82] "b"
                                    Expr 12 [85-90] [Type Bool]: Lit: Bool(false)
                                Stmt 13 [100-113]: Local (Immutable):
                                    Pat 14 [104-105] [Type Bool]: Bind: Ident 15 [104-105] "c"
                                    Expr 16 [108-112] [Type Bool]: Lit: Bool(true)
                                Stmt 17 [122-395]: Expr: Expr 90 [122-395] [Type Unit]: Expr Block: Block 86 [122-395] [Type Unit]:
                                    Stmt 76 [0-0]: Local (Mutable):
                                        Pat 77 [291-292] [Type Bool]: Bind: Ident 74 [291-292] "@continue_cond_74"
                                        Expr 75 [291-292] [Type Bool]: Lit: Bool(true)
                                    Stmt 87 [0-0]: Expr: Expr 88 [122-395] [Type Unit]: While:
                                        Expr 89 [291-292] [Type Bool]: Var: Local 74
                                        Block 19 [129-284] [Type Unit]:
                                            Stmt 20 [143-274]: Expr: Expr 60 [143-274] [Type Unit]: Expr Block: Block 56 [143-274] [Type Unit]:
                                                Stmt 46 [0-0]: Local (Mutable):
                                                    Pat 47 [205-206] [Type Bool]: Bind: Ident 44 [205-206] "@continue_cond_44"
                                                    Expr 45 [205-206] [Type Bool]: Lit: Bool(true)
                                                Stmt 57 [0-0]: Expr: Expr 58 [143-274] [Type Unit]: While:
                                                    Expr 59 [205-206] [Type Bool]: Var: Local 44
                                                    Block 22 [150-198] [Type Unit]:
                                                        Stmt 23 [168-184]: Local (Immutable):
                                                            Pat 24 [172-173] [Type String]: Bind: Ident 25 [172-173] "x"
                                                            Expr 26 [176-183] [Type String]: String:
                                                                Lit: "First"
                                                        Stmt 48 [205-206]: Semi: Expr 49 [205-206] [Type Unit]: Assign:
                                                            Expr 50 [205-206] [Type Bool]: Var: Local 44
                                                            Expr 51 [205-206] [Type Bool]: UnOp (NotL):
                                                                Expr 27 [205-206] [Type Bool]: Var: Local 7
                                                        Stmt 52 [225-274]: Expr: Expr 53 [225-274] [Type Unit]: If:
                                                            Expr 54 [205-206] [Type Bool]: Var: Local 44
                                                            Expr 55 [225-274] [Type Unit]: Expr Block: Block 28 [225-274] [Type Unit]:
                                                                Stmt 29 [243-260]: Local (Immutable):
                                                                    Pat 30 [247-248] [Type String]: Bind: Ident 31 [247-248] "y"
                                                                    Expr 32 [251-259] [Type String]: String:
                                                                        Lit: "Second"
                                            Stmt 78 [291-292]: Semi: Expr 79 [291-292] [Type Unit]: Assign:
                                                Expr 80 [291-292] [Type Bool]: Var: Local 74
                                                Expr 81 [291-292] [Type Bool]: UnOp (NotL):
                                                    Expr 33 [291-292] [Type Bool]: Var: Local 11
                                            Stmt 82 [307-395]: Expr: Expr 83 [307-395] [Type Unit]: If:
                                                Expr 84 [291-292] [Type Bool]: Var: Local 74
                                                Expr 85 [307-395] [Type Unit]: Expr Block: Block 34 [307-395] [Type Unit]:
                                                    Stmt 35 [321-385]: Semi: Expr 73 [321-384] [Type Unit]: Expr Block: Block 69 [321-384] [Type Unit]:
                                                        Stmt 63 [0-0]: Local (Mutable):
                                                            Pat 64 [383-384] [Type Bool]: Bind: Ident 61 [383-384] "@continue_cond_61"
                                                            Expr 62 [383-384] [Type Bool]: Lit: Bool(true)
                                                        Stmt 70 [0-0]: Expr: Expr 71 [321-384] [Type Unit]: While:
                                                            Expr 72 [383-384] [Type Bool]: Var: Local 61
                                                            Block 37 [328-376] [Type Unit]:
                                                                Stmt 38 [346-362]: Local (Immutable):
                                                                    Pat 39 [350-351] [Type String]: Bind: Ident 40 [350-351] "z"
                                                                    Expr 41 [354-361] [Type String]: String:
                                                                        Lit: "Third"
                                                                Stmt 65 [383-384]: Semi: Expr 66 [383-384] [Type Unit]: Assign:
                                                                    Expr 67 [383-384] [Type Bool]: Var: Local 61
                                                                    Expr 68 [383-384] [Type Bool]: UnOp (NotL):
                                                                        Expr 42 [383-384] [Type Bool]: Var: Local 15
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_treats_for_loop_with_short_circuit_expression_explicit_int() {
    check(
        indoc! {r#"
        function Main() : Unit {
            let x = for i : Int in fail "" {};
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-65] (Public):
                    Namespace (Ident 14 [0-65] "test"): Item 1
                Item 1 [0-65] (Internal):
                    Parent: 0
                    Callable 0 [0-65] (function):
                        name: Ident 1 [9-13] "Main"
                        input: Pat 2 [13-15] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-65]: Impl:
                            Block 4 [23-65] [Type Unit]:
                                Stmt 5 [29-63]: Local (Immutable):
                                    Pat 6 [33-34] [Type Unit]: Bind: Ident 7 [33-34] "x"
                                    Expr 41 [37-62] [Type Unit]: Expr Block: Block 42 [37-62] [Type Unit]:
                                        Stmt 16 [0-0]: Local (Immutable):
                                            Pat 17 [52-59] [Type Unit]: Bind: Ident 15 [52-59] "@array_id_15"
                                            Expr 11 [52-59] [Type Unit]: Fail: Expr 12 [57-59] [Type String]: String:
                                                Lit: ""
                                        Stmt 22 [0-0]: Local (Immutable):
                                            Pat 23 [52-59] [Type Int]: Bind: Ident 19 [52-59] "@len_id_19"
                                            Expr 20 [52-59] [Type Int]: Call:
                                                Expr 18 [52-59] [Type (Int[] -> Int)]: Var:
                                                    res: Item 1 (Package 0)
                                                    generics:
                                                        Int
                                                Expr 21 [52-59] [Type Unit]: Var: Local 15
                                        Stmt 26 [52-59]: Local (Mutable):
                                            Pat 27 [52-59] [Type Int]: Bind: Ident 24 [52-59] "@index_id_24"
                                            Expr 25 [52-59] [Type Int]: Lit: Int(0)
                                        Stmt 39 [0-0]: Expr: Expr 40 [37-62] [Type Unit]: While:
                                            Expr 36 [52-59] [Type Bool]: BinOp (Lt):
                                                Expr 37 [52-59] [Type Int]: Var: Local 24
                                                Expr 38 [52-59] [Type Int]: Var: Local 19
                                            Block 13 [60-62] [Type Unit]:
                                                Stmt 28 [41-48]: Local (Immutable):
                                                    Pat 9 [41-48] [Type Int]: Bind: Ident 10 [41-42] "i"
                                                    Expr 29 [52-59] [Type Int]: Index:
                                                        Expr 30 [52-59] [Type Unit]: Var: Local 15
                                                        Expr 31 [52-59] [Type Int]: Var: Local 24
                                                Stmt 33 [52-59]: Semi: Expr 34 [52-59] [Type Unit]: AssignOp (Add):
                                                    Expr 35 [52-59] [Type Int]: Var: Local 24
                                                    Expr 32 [52-59] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_treats_for_loop_with_short_circuit_expression_explicit_non_int() {
    check(
        indoc! {r#"
        function Main() : Unit {
            let x = for i : String in fail "" {};
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-68] (Public):
                    Namespace (Ident 14 [0-68] "test"): Item 1
                Item 1 [0-68] (Internal):
                    Parent: 0
                    Callable 0 [0-68] (function):
                        name: Ident 1 [9-13] "Main"
                        input: Pat 2 [13-15] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-68]: Impl:
                            Block 4 [23-68] [Type Unit]:
                                Stmt 5 [29-66]: Local (Immutable):
                                    Pat 6 [33-34] [Type Unit]: Bind: Ident 7 [33-34] "x"
                                    Expr 41 [37-65] [Type Unit]: Expr Block: Block 42 [37-65] [Type Unit]:
                                        Stmt 16 [0-0]: Local (Immutable):
                                            Pat 17 [55-62] [Type Unit]: Bind: Ident 15 [55-62] "@array_id_15"
                                            Expr 11 [55-62] [Type Unit]: Fail: Expr 12 [60-62] [Type String]: String:
                                                Lit: ""
                                        Stmt 22 [0-0]: Local (Immutable):
                                            Pat 23 [55-62] [Type Int]: Bind: Ident 19 [55-62] "@len_id_19"
                                            Expr 20 [55-62] [Type Int]: Call:
                                                Expr 18 [55-62] [Type (String[] -> Int)]: Var:
                                                    res: Item 1 (Package 0)
                                                    generics:
                                                        String
                                                Expr 21 [55-62] [Type Unit]: Var: Local 15
                                        Stmt 26 [55-62]: Local (Mutable):
                                            Pat 27 [55-62] [Type Int]: Bind: Ident 24 [55-62] "@index_id_24"
                                            Expr 25 [55-62] [Type Int]: Lit: Int(0)
                                        Stmt 39 [0-0]: Expr: Expr 40 [37-65] [Type Unit]: While:
                                            Expr 36 [55-62] [Type Bool]: BinOp (Lt):
                                                Expr 37 [55-62] [Type Int]: Var: Local 24
                                                Expr 38 [55-62] [Type Int]: Var: Local 19
                                            Block 13 [63-65] [Type Unit]:
                                                Stmt 28 [41-51]: Local (Immutable):
                                                    Pat 9 [41-51] [Type String]: Bind: Ident 10 [41-42] "i"
                                                    Expr 29 [55-62] [Type String]: Index:
                                                        Expr 30 [55-62] [Type Unit]: Var: Local 15
                                                        Expr 31 [55-62] [Type Int]: Var: Local 24
                                                Stmt 33 [55-62]: Semi: Expr 34 [55-62] [Type Unit]: AssignOp (Add):
                                                    Expr 35 [55-62] [Type Int]: Var: Local 24
                                                    Expr 32 [55-62] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}
