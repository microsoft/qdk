// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::visit::Visitor;

use crate::borrowck::Checker;

fn check(expr: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let sources: SourceMap = SourceMap::new([("test".into(), "".into())], Some(expr.into()));
    let package_id = store.new_package_id();
    let unit = compile(
        &store,
        &[],
        sources,
        package_id,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let mut borrow_check = Checker::default();
    borrow_check.visit_package(&unit.package);
    expect.assert_debug_eq(&borrow_check.errors);
}

#[test]
fn assign_invalid_expr() {
    check(
        "set 0 = 1",
        &expect![[r#"
            [
                Unassignable(
                    Span {
                        lo: 4,
                        hi: 5,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn assignop_mutability_expr() {
    check(
        indoc! {"{
            let x = false;
            set x or= true;
            x
        }"},
        &expect![[r#"
            [
                Mutability(
                    Span {
                        lo: 29,
                        hi: 30,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn assignupdate_immutable_expr() {
    check(
        indoc! {"{
            let x = [1, 2, 3];
            set x w/= 2 <- 4;
            x
        }"},
        &expect![[r#"
            [
                Mutability(
                    Span {
                        lo: 33,
                        hi: 34,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn block_mutable_immutable_expr() {
    check(
        indoc! {"{
            let x = 0;
            set x = 1;
        }"},
        &expect![[r#"
            [
                Mutability(
                    Span {
                        lo: 25,
                        hi: 26,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn block_mutable_expr() {
    check(
        indoc! {"{
            mutable x = 0;
            set x = 1;
        }"},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn for_loop_iterator_immutable_expr() {
    check(
        "for i in 0..10 { set i = 0; }",
        &expect![[r#"
            [
                Mutability(
                    Span {
                        lo: 21,
                        hi: 22,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn complex_expr_not_assignable() {
    check(
        indoc! {"{
            mutable (x, y) = (0, 0);
            set if false { x } else { y } = 1;
        }"},
        &expect![[r#"
            [
                Unassignable(
                    Span {
                        lo: 39,
                        hi: 64,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn lambda_mutable_closure() {
    check(
        indoc! {"{
            mutable x = 1;
            let f = y -> x + y;
        }"},
        &expect![[r#"
            [
                MutableClosure(
                    Span {
                        lo: 33,
                        hi: 43,
                    },
                ),
            ]
        "#]],
    );
}
