// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{validate::Validator, visit::Visitor};

use crate::conjugate_invert::invert_conjugate_exprs;

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

    let errors = invert_conjugate_exprs(store.core(), &mut unit.package, &mut unit.assigner);
    Validator::default().visit_package(&unit.package);
    if errors.is_empty() {
        expect.assert_eq(&unit.package.to_string());
    } else {
        expect.assert_debug_eq(&errors);
    }
}

#[test]
fn conjugate_invert() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    within {
                        B(1);
                        B(2);
                    }
                    apply {
                        B(3);
                        B(4);
                    }
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-218] (Public):
                    Namespace (Ident 31 [10-14] "Test"): Item 1, Item 2
                Item 1 [21-58] (Internal):
                    Parent: 0
                    Callable 0 [21-58] (operation):
                        name: Ident 1 [31-32] "B"
                        input: Pat 2 [33-40] [Type Int]: Bind: Ident 3 [33-34] "i"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 4 [21-58]: Impl:
                            Block 5 [56-58]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [63-216] (Internal):
                    Parent: 0
                    Callable 6 [63-216] (operation):
                        name: Ident 7 [73-74] "A"
                        input: Pat 8 [74-76] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 9 [63-216]: Impl:
                            Block 10 [84-216] [Type Unit]:
                                Stmt 11 [94-210]: Expr: Expr 54 [0-0] [Type Unit]: Expr Block: Block 47 [0-0] [Type Unit]:
                                    Stmt 48 [0-0]: Expr: Expr 49 [0-0] [Type Unit]: Expr Block: Block 13 [101-148] [Type Unit]:
                                        Stmt 14 [115-120]: Semi: Expr 15 [115-119] [Type Unit]: Call:
                                            Expr 16 [115-116] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 17 [117-118] [Type Int]: Lit: Int(1)
                                        Stmt 18 [133-138]: Semi: Expr 19 [133-137] [Type Unit]: Call:
                                            Expr 20 [133-134] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 21 [135-136] [Type Int]: Lit: Int(2)
                                    Stmt 44 [0-0]: Local (Immutable):
                                        Pat 45 [0-0] [Type Unit]: Bind: Ident 43 [0-0] "@apply_res"
                                        Expr 46 [0-0] [Type Unit]: Expr Block: Block 22 [163-210] [Type Unit]:
                                            Stmt 23 [177-182]: Semi: Expr 24 [177-181] [Type Unit]: Call:
                                                Expr 25 [177-178] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 26 [179-180] [Type Int]: Lit: Int(3)
                                            Stmt 27 [195-200]: Semi: Expr 28 [195-199] [Type Unit]: Call:
                                                Expr 29 [195-196] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 30 [197-198] [Type Int]: Lit: Int(4)
                                    Stmt 50 [0-0]: Expr: Expr 51 [0-0] [Type Unit]: Expr Block: Block 32 [101-148] [Type Unit]:
                                        Stmt 33 [133-138]: Semi: Expr 34 [133-137] [Type Unit]: Call:
                                            Expr 35 [133-134] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                Expr 36 [133-134] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 37 [135-136] [Type Int]: Lit: Int(2)
                                        Stmt 38 [115-120]: Semi: Expr 39 [115-119] [Type Unit]: Call:
                                            Expr 40 [115-116] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                Expr 41 [115-116] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 42 [117-118] [Type Int]: Lit: Int(1)
                                    Stmt 52 [0-0]: Expr: Expr 53 [0-0] [Type Unit]: Var: Local 43
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn conjugate_invert_with_output() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Int {
                    let val = within {
                        B(1);
                        B(2);
                    }
                    apply {
                        B(3);
                        B(4);
                        7
                    };
                    val
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-254] (Public):
                    Namespace (Ident 37 [10-14] "Test"): Item 1, Item 2
                Item 1 [21-58] (Internal):
                    Parent: 0
                    Callable 0 [21-58] (operation):
                        name: Ident 1 [31-32] "B"
                        input: Pat 2 [33-40] [Type Int]: Bind: Ident 3 [33-34] "i"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 4 [21-58]: Impl:
                            Block 5 [56-58]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [63-252] (Internal):
                    Parent: 0
                    Callable 6 [63-252] (operation):
                        name: Ident 7 [73-74] "A"
                        input: Pat 8 [74-76] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 9 [63-252]: Impl:
                            Block 10 [83-252] [Type Int]:
                                Stmt 11 [93-234]: Local (Immutable):
                                    Pat 12 [97-100] [Type Int]: Bind: Ident 13 [97-100] "val"
                                    Expr 60 [0-0] [Type Int]: Expr Block: Block 53 [0-0] [Type Int]:
                                        Stmt 54 [0-0]: Expr: Expr 55 [0-0] [Type Unit]: Expr Block: Block 15 [110-157] [Type Unit]:
                                            Stmt 16 [124-129]: Semi: Expr 17 [124-128] [Type Unit]: Call:
                                                Expr 18 [124-125] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 19 [126-127] [Type Int]: Lit: Int(1)
                                            Stmt 20 [142-147]: Semi: Expr 21 [142-146] [Type Unit]: Call:
                                                Expr 22 [142-143] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 23 [144-145] [Type Int]: Lit: Int(2)
                                        Stmt 50 [0-0]: Local (Immutable):
                                            Pat 51 [0-0] [Type Int]: Bind: Ident 49 [0-0] "@apply_res"
                                            Expr 52 [0-0] [Type Int]: Expr Block: Block 24 [172-233] [Type Int]:
                                                Stmt 25 [186-191]: Semi: Expr 26 [186-190] [Type Unit]: Call:
                                                    Expr 27 [186-187] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 28 [188-189] [Type Int]: Lit: Int(3)
                                                Stmt 29 [204-209]: Semi: Expr 30 [204-208] [Type Unit]: Call:
                                                    Expr 31 [204-205] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 32 [206-207] [Type Int]: Lit: Int(4)
                                                Stmt 33 [222-223]: Expr: Expr 34 [222-223] [Type Int]: Lit: Int(7)
                                        Stmt 56 [0-0]: Expr: Expr 57 [0-0] [Type Unit]: Expr Block: Block 38 [110-157] [Type Unit]:
                                            Stmt 39 [142-147]: Semi: Expr 40 [142-146] [Type Unit]: Call:
                                                Expr 41 [142-143] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                    Expr 42 [142-143] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 43 [144-145] [Type Int]: Lit: Int(2)
                                            Stmt 44 [124-129]: Semi: Expr 45 [124-128] [Type Unit]: Call:
                                                Expr 46 [124-125] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                    Expr 47 [124-125] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 48 [126-127] [Type Int]: Lit: Int(1)
                                        Stmt 58 [0-0]: Expr: Expr 59 [0-0] [Type Int]: Var: Local 49
                                Stmt 35 [243-246]: Expr: Expr 36 [243-246] [Type Int]: Var: Local 13
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn nested_conjugate_invert() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    within {
                        B(0);
                        within {
                            B(1);
                            B(2);
                        }
                        apply {
                            B(3);
                            B(4);
                        }
                    }
                    apply {
                        B(5);
                        B(6);
                    }
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-357] (Public):
                    Namespace (Ident 47 [10-14] "Test"): Item 1, Item 2
                Item 1 [21-58] (Internal):
                    Parent: 0
                    Callable 0 [21-58] (operation):
                        name: Ident 1 [31-32] "B"
                        input: Pat 2 [33-40] [Type Int]: Bind: Ident 3 [33-34] "i"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 4 [21-58]: Impl:
                            Block 5 [56-58]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [63-355] (Internal):
                    Parent: 0
                    Callable 6 [63-355] (operation):
                        name: Ident 7 [73-74] "A"
                        input: Pat 8 [74-76] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 9 [63-355]: Impl:
                            Block 10 [84-355] [Type Unit]:
                                Stmt 11 [94-349]: Expr: Expr 87 [0-0] [Type Unit]: Expr Block: Block 80 [0-0] [Type Unit]:
                                    Stmt 81 [0-0]: Expr: Expr 82 [0-0] [Type Unit]: Expr Block: Block 13 [101-287] [Type Unit]:
                                        Stmt 14 [115-120]: Semi: Expr 15 [115-119] [Type Unit]: Call:
                                            Expr 16 [115-116] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 17 [117-118] [Type Int]: Lit: Int(0)
                                        Stmt 18 [133-277]: Expr: Expr 110 [0-0] [Type Unit]: Expr Block: Block 103 [0-0] [Type Unit]:
                                            Stmt 104 [0-0]: Expr: Expr 105 [0-0] [Type Unit]: Expr Block: Block 20 [140-199] [Type Unit]:
                                                Stmt 21 [158-163]: Semi: Expr 22 [158-162] [Type Unit]: Call:
                                                    Expr 23 [158-159] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 24 [160-161] [Type Int]: Lit: Int(1)
                                                Stmt 25 [180-185]: Semi: Expr 26 [180-184] [Type Unit]: Call:
                                                    Expr 27 [180-181] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 28 [182-183] [Type Int]: Lit: Int(2)
                                            Stmt 100 [0-0]: Local (Immutable):
                                                Pat 101 [0-0] [Type Unit]: Bind: Ident 99 [0-0] "@apply_res"
                                                Expr 102 [0-0] [Type Unit]: Expr Block: Block 29 [218-277] [Type Unit]:
                                                    Stmt 30 [236-241]: Semi: Expr 31 [236-240] [Type Unit]: Call:
                                                        Expr 32 [236-237] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                        Expr 33 [238-239] [Type Int]: Lit: Int(3)
                                                    Stmt 34 [258-263]: Semi: Expr 35 [258-262] [Type Unit]: Call:
                                                        Expr 36 [258-259] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                        Expr 37 [260-261] [Type Int]: Lit: Int(4)
                                            Stmt 106 [0-0]: Expr: Expr 107 [0-0] [Type Unit]: Expr Block: Block 88 [140-199] [Type Unit]:
                                                Stmt 89 [180-185]: Semi: Expr 90 [180-184] [Type Unit]: Call:
                                                    Expr 91 [180-181] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                        Expr 92 [180-181] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 93 [182-183] [Type Int]: Lit: Int(2)
                                                Stmt 94 [158-163]: Semi: Expr 95 [158-162] [Type Unit]: Call:
                                                    Expr 96 [158-159] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                        Expr 97 [158-159] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 98 [160-161] [Type Int]: Lit: Int(1)
                                            Stmt 108 [0-0]: Expr: Expr 109 [0-0] [Type Unit]: Var: Local 99
                                    Stmt 77 [0-0]: Local (Immutable):
                                        Pat 78 [0-0] [Type Unit]: Bind: Ident 76 [0-0] "@apply_res"
                                        Expr 79 [0-0] [Type Unit]: Expr Block: Block 38 [302-349] [Type Unit]:
                                            Stmt 39 [316-321]: Semi: Expr 40 [316-320] [Type Unit]: Call:
                                                Expr 41 [316-317] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 42 [318-319] [Type Int]: Lit: Int(5)
                                            Stmt 43 [334-339]: Semi: Expr 44 [334-338] [Type Unit]: Call:
                                                Expr 45 [334-335] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 46 [336-337] [Type Int]: Lit: Int(6)
                                    Stmt 83 [0-0]: Expr: Expr 84 [0-0] [Type Unit]: Expr Block: Block 48 [101-287] [Type Unit]:
                                        Stmt 49 [133-277]: Expr: Expr 133 [0-0] [Type Unit]: Expr Block: Block 126 [0-0] [Type Unit]:
                                            Stmt 127 [0-0]: Expr: Expr 128 [0-0] [Type Unit]: Expr Block: Block 51 [140-199] [Type Unit]:
                                                Stmt 52 [158-163]: Semi: Expr 53 [158-162] [Type Unit]: Call:
                                                    Expr 54 [158-159] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 55 [160-161] [Type Int]: Lit: Int(1)
                                                Stmt 56 [180-185]: Semi: Expr 57 [180-184] [Type Unit]: Call:
                                                    Expr 58 [180-181] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 59 [182-183] [Type Int]: Lit: Int(2)
                                            Stmt 123 [0-0]: Local (Immutable):
                                                Pat 124 [0-0] [Type Unit]: Bind: Ident 122 [0-0] "@apply_res"
                                                Expr 125 [0-0] [Type Unit]: Expr Block: Block 60 [218-277] [Type Unit]:
                                                    Stmt 61 [258-263]: Semi: Expr 62 [258-262] [Type Unit]: Call:
                                                        Expr 63 [258-259] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                            Expr 64 [258-259] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                        Expr 65 [260-261] [Type Int]: Lit: Int(4)
                                                    Stmt 66 [236-241]: Semi: Expr 67 [236-240] [Type Unit]: Call:
                                                        Expr 68 [236-237] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                            Expr 69 [236-237] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                        Expr 70 [238-239] [Type Int]: Lit: Int(3)
                                            Stmt 129 [0-0]: Expr: Expr 130 [0-0] [Type Unit]: Expr Block: Block 111 [140-199] [Type Unit]:
                                                Stmt 112 [180-185]: Semi: Expr 113 [180-184] [Type Unit]: Call:
                                                    Expr 114 [180-181] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                        Expr 115 [180-181] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 116 [182-183] [Type Int]: Lit: Int(2)
                                                Stmt 117 [158-163]: Semi: Expr 118 [158-162] [Type Unit]: Call:
                                                    Expr 119 [158-159] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                        Expr 120 [158-159] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                    Expr 121 [160-161] [Type Int]: Lit: Int(1)
                                            Stmt 131 [0-0]: Expr: Expr 132 [0-0] [Type Unit]: Var: Local 122
                                        Stmt 71 [115-120]: Semi: Expr 72 [115-119] [Type Unit]: Call:
                                            Expr 73 [115-116] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                Expr 74 [115-116] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 75 [117-118] [Type Int]: Lit: Int(0)
                                    Stmt 85 [0-0]: Expr: Expr 86 [0-0] [Type Unit]: Var: Local 76
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn conjugate_invalid_op_fails() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit {}
                operation A() : Unit {
                    within {
                        B(1);
                        B(2);
                    }
                    apply {
                        B(3);
                        B(4);
                    }
                }
            }
        "},
        &expect![[r#"
            [
                AdjGen(
                    MissingAdjFunctor(
                        Span {
                            lo: 126,
                            hi: 127,
                        },
                    ),
                ),
                AdjGen(
                    MissingAdjFunctor(
                        Span {
                            lo: 108,
                            hi: 109,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn conjugate_not_separable_fail() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    within {
                        let x = B(1);
                        B(2);
                    }
                    apply {
                        B(3);
                        B(4);
                    }
                }
            }
        "},
        &expect![[r#"
            [
                AdjGen(
                    LogicSep(
                        OpCallForbidden(
                            Span {
                                lo: 123,
                                hi: 127,
                            },
                        ),
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn conjugate_mutable_update_in_apply_fail() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    mutable a = 1;
                    within {
                        let x = a;
                        B(2);
                    }
                    apply {
                        set a = 3;
                        B(4);
                    }
                }
            }
        "},
        &expect![[r#"
            [
                ApplyAssign(
                    Span {
                        lo: 209,
                        hi: 210,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn conjugate_return_in_apply_fail() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    mutable a = 1;
                    within {
                        let x = a;
                        B(2);
                    }
                    apply {
                        return ();
                    }
                }
            }
        "},
        &expect![[r#"
            [
                ReturnForbidden(
                    Span {
                        lo: 205,
                        hi: 214,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn conjugate_mutable_correct_use_succeeds() {
    check(
        indoc! {"
            namespace Test {
                operation B(i : Int) : Unit is Adj {}
                operation A() : Unit {
                    mutable a = 1;
                    within {
                        let x = a;
                        B(1);
                        B(2);
                        let y = x;
                    }
                    apply {
                        mutable b = a;
                        set b = 0;
                        B(3);
                        B(4);
                    }
                }
            }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-337] (Public):
                    Namespace (Ident 51 [10-14] "Test"): Item 1, Item 2
                Item 1 [21-58] (Internal):
                    Parent: 0
                    Callable 0 [21-58] (operation):
                        name: Ident 1 [31-32] "B"
                        input: Pat 2 [33-40] [Type Int]: Bind: Ident 3 [33-34] "i"
                        output: Unit
                        functors: Adj
                        body: SpecDecl 4 [21-58]: Impl:
                            Block 5 [56-58]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>
                Item 2 [63-335] (Internal):
                    Parent: 0
                    Callable 6 [63-335] (operation):
                        name: Ident 7 [73-74] "A"
                        input: Pat 8 [74-76] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 9 [63-335]: Impl:
                            Block 10 [84-335] [Type Unit]:
                                Stmt 11 [94-108]: Local (Mutable):
                                    Pat 12 [102-103] [Type Int]: Bind: Ident 13 [102-103] "a"
                                    Expr 14 [106-107] [Type Int]: Lit: Int(1)
                                Stmt 15 [117-329]: Expr: Expr 82 [0-0] [Type Unit]: Expr Block: Block 75 [0-0] [Type Unit]:
                                    Stmt 76 [0-0]: Expr: Expr 77 [0-0] [Type Unit]: Expr Block: Block 17 [124-217] [Type Unit]:
                                        Stmt 18 [138-148]: Local (Immutable):
                                            Pat 19 [142-143] [Type Int]: Bind: Ident 20 [142-143] "x"
                                            Expr 21 [146-147] [Type Int]: Var: Local 13
                                        Stmt 22 [161-166]: Semi: Expr 23 [161-165] [Type Unit]: Call:
                                            Expr 24 [161-162] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 25 [163-164] [Type Int]: Lit: Int(1)
                                        Stmt 26 [179-184]: Semi: Expr 27 [179-183] [Type Unit]: Call:
                                            Expr 28 [179-180] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 29 [181-182] [Type Int]: Lit: Int(2)
                                        Stmt 30 [197-207]: Local (Immutable):
                                            Pat 31 [201-202] [Type Int]: Bind: Ident 32 [201-202] "y"
                                            Expr 33 [205-206] [Type Int]: Var: Local 20
                                    Stmt 72 [0-0]: Local (Immutable):
                                        Pat 73 [0-0] [Type Unit]: Bind: Ident 71 [0-0] "@apply_res"
                                        Expr 74 [0-0] [Type Unit]: Expr Block: Block 34 [232-329] [Type Unit]:
                                            Stmt 35 [246-260]: Local (Mutable):
                                                Pat 36 [254-255] [Type Int]: Bind: Ident 37 [254-255] "b"
                                                Expr 38 [258-259] [Type Int]: Var: Local 13
                                            Stmt 39 [273-283]: Semi: Expr 40 [273-282] [Type Unit]: Assign:
                                                Expr 41 [277-278] [Type Int]: Var: Local 37
                                                Expr 42 [281-282] [Type Int]: Lit: Int(0)
                                            Stmt 43 [296-301]: Semi: Expr 44 [296-300] [Type Unit]: Call:
                                                Expr 45 [296-297] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 46 [298-299] [Type Int]: Lit: Int(3)
                                            Stmt 47 [314-319]: Semi: Expr 48 [314-318] [Type Unit]: Call:
                                                Expr 49 [314-315] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                                Expr 50 [316-317] [Type Int]: Lit: Int(4)
                                    Stmt 78 [0-0]: Expr: Expr 79 [0-0] [Type Unit]: Expr Block: Block 52 [124-217] [Type Unit]:
                                        Stmt 53 [138-148]: Local (Immutable):
                                            Pat 54 [142-143] [Type Int]: Bind: Ident 55 [142-143] "x"
                                            Expr 56 [146-147] [Type Int]: Var: Local 13
                                        Stmt 57 [197-207]: Local (Immutable):
                                            Pat 58 [201-202] [Type Int]: Bind: Ident 59 [201-202] "y"
                                            Expr 60 [205-206] [Type Int]: Var: Local 55
                                        Stmt 61 [179-184]: Semi: Expr 62 [179-183] [Type Unit]: Call:
                                            Expr 63 [179-180] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                Expr 64 [179-180] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 65 [181-182] [Type Int]: Lit: Int(2)
                                        Stmt 66 [161-166]: Semi: Expr 67 [161-165] [Type Unit]: Call:
                                            Expr 68 [161-162] [Type (Int => Unit is Adj)]: UnOp (Functor Adj):
                                                Expr 69 [161-162] [Type (Int => Unit is Adj)]: Var: Item 1 (Package 1)
                                            Expr 70 [163-164] [Type Int]: Lit: Int(1)
                                    Stmt 80 [0-0]: Expr: Expr 81 [0-0] [Type Unit]: Var: Local 71
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}
