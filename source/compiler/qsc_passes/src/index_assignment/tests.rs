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

use crate::index_assignment::ConvertToWSlash;

fn check(file: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let package_id = store.new_package_id();
    let mut unit = compile(
        &store,
        &[],
        sources,
        package_id,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    ConvertToWSlash {
        assigner: &mut unit.assigner,
    }
    .visit_package(&mut unit.package);
    Validator::default().visit_package(&unit.package);
    expect.assert_eq(&unit.package.to_string());
}

#[test]
fn convert_array_assign() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [0, 1];
            arr[0] = 3;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-69] (Public):
                    Namespace (Ident 17 [0-69] "test"): Item 1
                Item 1 [0-69] (Internal):
                    Parent: 0
                    Callable 0 [0-69] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-69]: Impl:
                            Block 4 [24-69] [Type Unit]:
                                Stmt 5 [30-51]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-50] [Type Int[]]: Array:
                                        Expr 9 [45-46] [Type Int]: Lit: Int(0)
                                        Expr 10 [48-49] [Type Int]: Lit: Int(1)
                                Stmt 11 [56-67]: Semi: Expr 25 [56-66] [Type Unit]: Expr Block: Block 26 [56-66] [Type Unit]:
                                    Stmt 19 [0-0]: Local (Immutable):
                                        Pat 20 [60-61] [Type Int]: Bind: Ident 18 [60-61] "@index_18"
                                        Expr 15 [60-61] [Type Int]: Lit: Int(0)
                                    Stmt 24 [0-0]: Expr: Expr 22 [0-0] [Type Unit]: AssignIndex:
                                        Expr 21 [56-59] [Type Int[]]: Var: Local 7
                                        Expr 23 [60-61] [Type Int]: Var: Local 18
                                        Expr 16 [65-66] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_2d_array_assign() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [[0, 1], [2, 3]];
            arr[0][1] = 3;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-82] (Public):
                    Namespace (Ident 23 [0-82] "test"): Item 1
                Item 1 [0-82] (Internal):
                    Parent: 0
                    Callable 0 [0-82] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-82]: Impl:
                            Block 4 [24-82] [Type Unit]:
                                Stmt 5 [30-61]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[][]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-60] [Type Int[][]]: Array:
                                        Expr 9 [45-51] [Type Int[]]: Array:
                                            Expr 10 [46-47] [Type Int]: Lit: Int(0)
                                            Expr 11 [49-50] [Type Int]: Lit: Int(1)
                                        Expr 12 [53-59] [Type Int[]]: Array:
                                            Expr 13 [54-55] [Type Int]: Lit: Int(2)
                                            Expr 14 [57-58] [Type Int]: Lit: Int(3)
                                Stmt 15 [66-80]: Semi: Expr 39 [66-79] [Type Unit]: Expr Block: Block 40 [66-79] [Type Unit]:
                                    Stmt 25 [0-0]: Local (Immutable):
                                        Pat 26 [70-71] [Type Int]: Bind: Ident 24 [70-71] "@index_24"
                                        Expr 20 [70-71] [Type Int]: Lit: Int(0)
                                    Stmt 28 [0-0]: Local (Immutable):
                                        Pat 29 [73-74] [Type Int]: Bind: Ident 27 [73-74] "@index_27"
                                        Expr 21 [73-74] [Type Int]: Lit: Int(1)
                                    Stmt 38 [0-0]: Expr: Expr 36 [0-0] [Type Unit]: AssignIndex:
                                        Expr 35 [66-69] [Type Int[][]]: Var: Local 7
                                        Expr 37 [70-71] [Type Int]: Var: Local 24
                                        Expr 33 [0-0] [Type Int[]]: UpdateIndex:
                                            Expr 31 [0-0] [Type Int[]]: Index:
                                                Expr 30 [66-69] [Type Int[][]]: Var: Local 7
                                                Expr 32 [70-71] [Type Int]: Var: Local 24
                                            Expr 34 [73-74] [Type Int]: Var: Local 27
                                            Expr 22 [78-79] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_3d_array_assign() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [[[0b000, 0b001], [0b010, 0b011]], [[0b100, 0b101], [0b110, 0b111]]];
            arr[0][1][1] = 0b1111;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-142] (Public):
                    Namespace (Ident 33 [0-142] "test"): Item 1
                Item 1 [0-142] (Internal):
                    Parent: 0
                    Callable 0 [0-142] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-142]: Impl:
                            Block 4 [24-142] [Type Unit]:
                                Stmt 5 [30-113]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[][][]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-112] [Type Int[][][]]: Array:
                                        Expr 9 [45-77] [Type Int[][]]: Array:
                                            Expr 10 [46-60] [Type Int[]]: Array:
                                                Expr 11 [47-52] [Type Int]: Lit: Int(0)
                                                Expr 12 [54-59] [Type Int]: Lit: Int(1)
                                            Expr 13 [62-76] [Type Int[]]: Array:
                                                Expr 14 [63-68] [Type Int]: Lit: Int(2)
                                                Expr 15 [70-75] [Type Int]: Lit: Int(3)
                                        Expr 16 [79-111] [Type Int[][]]: Array:
                                            Expr 17 [80-94] [Type Int[]]: Array:
                                                Expr 18 [81-86] [Type Int]: Lit: Int(4)
                                                Expr 19 [88-93] [Type Int]: Lit: Int(5)
                                            Expr 20 [96-110] [Type Int[]]: Array:
                                                Expr 21 [97-102] [Type Int]: Lit: Int(6)
                                                Expr 22 [104-109] [Type Int]: Lit: Int(7)
                                Stmt 23 [118-140]: Semi: Expr 59 [118-139] [Type Unit]: Expr Block: Block 60 [118-139] [Type Unit]:
                                    Stmt 35 [0-0]: Local (Immutable):
                                        Pat 36 [122-123] [Type Int]: Bind: Ident 34 [122-123] "@index_34"
                                        Expr 29 [122-123] [Type Int]: Lit: Int(0)
                                    Stmt 38 [0-0]: Local (Immutable):
                                        Pat 39 [125-126] [Type Int]: Bind: Ident 37 [125-126] "@index_37"
                                        Expr 30 [125-126] [Type Int]: Lit: Int(1)
                                    Stmt 41 [0-0]: Local (Immutable):
                                        Pat 42 [128-129] [Type Int]: Bind: Ident 40 [128-129] "@index_40"
                                        Expr 31 [128-129] [Type Int]: Lit: Int(1)
                                    Stmt 58 [0-0]: Expr: Expr 56 [0-0] [Type Unit]: AssignIndex:
                                        Expr 55 [118-121] [Type Int[][][]]: Var: Local 7
                                        Expr 57 [122-123] [Type Int]: Var: Local 34
                                        Expr 53 [0-0] [Type Int[][]]: UpdateIndex:
                                            Expr 51 [0-0] [Type Int[][]]: Index:
                                                Expr 50 [118-121] [Type Int[][][]]: Var: Local 7
                                                Expr 52 [122-123] [Type Int]: Var: Local 34
                                            Expr 54 [125-126] [Type Int]: Var: Local 37
                                            Expr 48 [0-0] [Type Int[]]: UpdateIndex:
                                                Expr 46 [0-0] [Type Int[]]: Index:
                                                    Expr 44 [0-0] [Type Int[][]]: Index:
                                                        Expr 43 [118-121] [Type Int[][][]]: Var: Local 7
                                                        Expr 45 [122-123] [Type Int]: Var: Local 34
                                                    Expr 47 [125-126] [Type Int]: Var: Local 37
                                                Expr 49 [128-129] [Type Int]: Var: Local 40
                                                Expr 32 [133-139] [Type Int]: Lit: Int(15)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_array_assign_range() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [0, 1, 2, 3];
            arr[1..3] = [4, 5, 6];
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-86] (Public):
                    Namespace (Ident 24 [0-86] "test"): Item 1
                Item 1 [0-86] (Internal):
                    Parent: 0
                    Callable 0 [0-86] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-86]: Impl:
                            Block 4 [24-86] [Type Unit]:
                                Stmt 5 [30-57]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-56] [Type Int[]]: Array:
                                        Expr 9 [45-46] [Type Int]: Lit: Int(0)
                                        Expr 10 [48-49] [Type Int]: Lit: Int(1)
                                        Expr 11 [51-52] [Type Int]: Lit: Int(2)
                                        Expr 12 [54-55] [Type Int]: Lit: Int(3)
                                Stmt 13 [62-84]: Semi: Expr 32 [62-83] [Type Unit]: Expr Block: Block 33 [62-83] [Type Unit]:
                                    Stmt 26 [0-0]: Local (Immutable):
                                        Pat 27 [66-70] [Type Range]: Bind: Ident 25 [66-70] "@index_25"
                                        Expr 17 [66-70] [Type Range]: Range:
                                            Expr 18 [66-67] [Type Int]: Lit: Int(1)
                                            <no step>
                                            Expr 19 [69-70] [Type Int]: Lit: Int(3)
                                    Stmt 31 [0-0]: Expr: Expr 29 [0-0] [Type Unit]: AssignIndex:
                                        Expr 28 [62-65] [Type Int[]]: Var: Local 7
                                        Expr 30 [66-70] [Type Range]: Var: Local 25
                                        Expr 20 [74-83] [Type Int[]]: Array:
                                            Expr 21 [75-76] [Type Int]: Lit: Int(4)
                                            Expr 22 [78-79] [Type Int]: Lit: Int(5)
                                            Expr 23 [81-82] [Type Int]: Lit: Int(6)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_array_assign_single_with_range() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [0, 1, 2, 3];
            arr[1..3][1] = 3;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-81] (Public):
                    Namespace (Ident 23 [0-81] "test"): Item 1
                Item 1 [0-81] (Internal):
                    Parent: 0
                    Callable 0 [0-81] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-81]: Impl:
                            Block 4 [24-81] [Type Unit]:
                                Stmt 5 [30-57]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-56] [Type Int[]]: Array:
                                        Expr 9 [45-46] [Type Int]: Lit: Int(0)
                                        Expr 10 [48-49] [Type Int]: Lit: Int(1)
                                        Expr 11 [51-52] [Type Int]: Lit: Int(2)
                                        Expr 12 [54-55] [Type Int]: Lit: Int(3)
                                Stmt 13 [62-79]: Semi: Expr 39 [62-78] [Type Unit]: Expr Block: Block 40 [62-78] [Type Unit]:
                                    Stmt 25 [0-0]: Local (Immutable):
                                        Pat 26 [66-70] [Type Range]: Bind: Ident 24 [66-70] "@index_24"
                                        Expr 18 [66-70] [Type Range]: Range:
                                            Expr 19 [66-67] [Type Int]: Lit: Int(1)
                                            <no step>
                                            Expr 20 [69-70] [Type Int]: Lit: Int(3)
                                    Stmt 28 [0-0]: Local (Immutable):
                                        Pat 29 [72-73] [Type Int]: Bind: Ident 27 [72-73] "@index_27"
                                        Expr 21 [72-73] [Type Int]: Lit: Int(1)
                                    Stmt 38 [0-0]: Expr: Expr 36 [0-0] [Type Unit]: AssignIndex:
                                        Expr 35 [62-65] [Type Int[]]: Var: Local 7
                                        Expr 37 [66-70] [Type Range]: Var: Local 24
                                        Expr 33 [0-0] [Type Int[]]: UpdateIndex:
                                            Expr 31 [0-0] [Type Int[]]: Index:
                                                Expr 30 [62-65] [Type Int[]]: Var: Local 7
                                                Expr 32 [66-70] [Type Range]: Var: Local 24
                                            Expr 34 [72-73] [Type Int]: Var: Local 27
                                            Expr 22 [77-78] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_array_assign_op() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [0, 1];
            arr[0] += 3;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-70] (Public):
                    Namespace (Ident 17 [0-70] "test"): Item 1
                Item 1 [0-70] (Internal):
                    Parent: 0
                    Callable 0 [0-70] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-70]: Impl:
                            Block 4 [24-70] [Type Unit]:
                                Stmt 5 [30-51]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-50] [Type Int[]]: Array:
                                        Expr 9 [45-46] [Type Int]: Lit: Int(0)
                                        Expr 10 [48-49] [Type Int]: Lit: Int(1)
                                Stmt 11 [56-68]: Semi: Expr 29 [56-67] [Type Unit]: Expr Block: Block 30 [56-67] [Type Unit]:
                                    Stmt 19 [0-0]: Local (Immutable):
                                        Pat 20 [60-61] [Type Int]: Bind: Ident 18 [60-61] "@index_18"
                                        Expr 15 [60-61] [Type Int]: Lit: Int(0)
                                    Stmt 28 [0-0]: Expr: Expr 26 [0-0] [Type Unit]: AssignIndex:
                                        Expr 25 [56-59] [Type Int[]]: Var: Local 7
                                        Expr 27 [60-61] [Type Int]: Var: Local 18
                                        Expr 24 [56-67] [Type Unit]: BinOp (Add):
                                            Expr 22 [0-0] [Type Int]: Index:
                                                Expr 21 [56-59] [Type Int[]]: Var: Local 7
                                                Expr 23 [60-61] [Type Int]: Var: Local 18
                                            Expr 16 [66-67] [Type Int]: Lit: Int(3)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn convert_2d_array_assign_op() {
    check(
        indoc! {r#"
        operation Main() : Unit {
            mutable arr = [[0, 1], [2, 3]];
            arr[0][1] *= 2;
        }
        "#},
        &expect![[r#"
            Package:
                Item 0 [0-83] (Public):
                    Namespace (Ident 23 [0-83] "test"): Item 1
                Item 1 [0-83] (Internal):
                    Parent: 0
                    Callable 0 [0-83] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [0-83]: Impl:
                            Block 4 [24-83] [Type Unit]:
                                Stmt 5 [30-61]: Local (Mutable):
                                    Pat 6 [38-41] [Type Int[][]]: Bind: Ident 7 [38-41] "arr"
                                    Expr 8 [44-60] [Type Int[][]]: Array:
                                        Expr 9 [45-51] [Type Int[]]: Array:
                                            Expr 10 [46-47] [Type Int]: Lit: Int(0)
                                            Expr 11 [49-50] [Type Int]: Lit: Int(1)
                                        Expr 12 [53-59] [Type Int[]]: Array:
                                            Expr 13 [54-55] [Type Int]: Lit: Int(2)
                                            Expr 14 [57-58] [Type Int]: Lit: Int(3)
                                Stmt 15 [66-81]: Semi: Expr 45 [66-80] [Type Unit]: Expr Block: Block 46 [66-80] [Type Unit]:
                                    Stmt 25 [0-0]: Local (Immutable):
                                        Pat 26 [70-71] [Type Int]: Bind: Ident 24 [70-71] "@index_24"
                                        Expr 20 [70-71] [Type Int]: Lit: Int(0)
                                    Stmt 28 [0-0]: Local (Immutable):
                                        Pat 29 [73-74] [Type Int]: Bind: Ident 27 [73-74] "@index_27"
                                        Expr 21 [73-74] [Type Int]: Lit: Int(1)
                                    Stmt 44 [0-0]: Expr: Expr 42 [0-0] [Type Unit]: AssignIndex:
                                        Expr 41 [66-69] [Type Int[][]]: Var: Local 7
                                        Expr 43 [70-71] [Type Int]: Var: Local 24
                                        Expr 39 [0-0] [Type Int[]]: UpdateIndex:
                                            Expr 37 [0-0] [Type Int[]]: Index:
                                                Expr 36 [66-69] [Type Int[][]]: Var: Local 7
                                                Expr 38 [70-71] [Type Int]: Var: Local 24
                                            Expr 40 [73-74] [Type Int]: Var: Local 27
                                            Expr 35 [66-80] [Type Unit]: BinOp (Mul):
                                                Expr 33 [0-0] [Type Int]: Index:
                                                    Expr 31 [0-0] [Type Int[]]: Index:
                                                        Expr 30 [66-69] [Type Int[][]]: Var: Local 7
                                                        Expr 32 [70-71] [Type Int]: Var: Local 24
                                                    Expr 34 [73-74] [Type Int]: Var: Local 27
                                                Expr 22 [79-80] [Type Int]: Lit: Int(2)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}
