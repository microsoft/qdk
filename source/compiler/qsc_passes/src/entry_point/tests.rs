// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{PackageType, entry_point::generate_entry_expr};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};

fn check(file: &str, expr: &str, expect: &Expect) {
    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let mut unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe);
    if errors.is_empty() {
        expect.assert_eq(
            &unit
                .package
                .entry
                .expect("entry should be present in success case")
                .to_string(),
        );
    } else {
        expect.assert_debug_eq(&errors);
    }
}

#[test]
fn generated_generic_entry_has_concrete_arrow_and_preserves_type_bounds() {
    use qsc_hir::{hir::ExprKind, ty::Ty};
    for (source, output) in [
        (
            "@EntryPoint() operation Main<'T>() : 'T { fail \"unreachable\" }",
            Ty::UNIT,
        ),
        (
            "@EntryPoint() operation Main<'T>() : Int { 42 }",
            Ty::Prim(qsc_hir::ty::Prim::Int),
        ),
        (
            "@EntryPoint() operation Main<'T : Eq>() : 'T { fail \"unreachable\" }",
            Ty::UNIT,
        ),
        (
            "@EntryPoint() operation Main<'T : Show + Eq>() : 'T[] { fail \"unreachable\" }",
            Ty::Array(Box::new(Ty::UNIT)),
        ),
        (
            "@EntryPoint() operation Main<'T : Show, 'U : Eq>() : ('T, 'U) { fail \"unreachable\" }",
            Ty::Tuple(vec![Ty::UNIT, Ty::UNIT]),
        ),
    ] {
        let mut unit = compile(
            &PackageStore::new(compile::core()),
            &[],
            SourceMap::new([("test".into(), source.into())], None),
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        );
        assert!(unit.errors.is_empty(), "{source}: {:?}", unit.errors);
        assert!(
            generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe).is_empty()
        );
        let entry = unit.package.entry.expect("generated entry");
        assert_eq!(entry.ty, output);
        let ExprKind::Call(callee, args) = entry.kind else {
            panic!("generated call")
        };
        let Ty::Arrow(arrow) = callee.ty else {
            panic!("callee must be arrow-typed")
        };
        assert_eq!(*arrow.input.borrow(), args.ty);
        assert_eq!(*arrow.output.borrow(), output);
    }
    for (bounds, body) in [
        ("Add", "'T { fail \"unreachable\" }"),
        ("Add", "Unit {}"),
        ("Eq + Add", "Unit {}"),
        ("Exp[Int]", "Unit {}"),
        ("Iterable[Bool]", "Unit {}"),
    ] {
        let source = format!("@EntryPoint() operation Main<'T : {bounds}>() : {body}");
        let mut unit = compile(
            &PackageStore::new(compile::core()),
            &[],
            SourceMap::new([("test".into(), source.into())], None),
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        );
        assert!(unit.errors.is_empty(), "{:?}", unit.errors);
        let errors = generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe);
        let [crate::Error::EntryPoint(super::Error::GenericBounds(name, span))] = errors.as_slice()
        else {
            panic!("Unit cannot satisfy {bounds}: {errors:?}")
        };
        assert_eq!(name, "'T");
        assert_eq!(*span, qsc_data_structures::span::Span { lo: 24, hi: 28 });
        assert_eq!(
            errors[0].to_string(),
            "cannot infer a concrete entry-point type satisfying the bounds of `'T`"
        );
        assert!(unit.package.entry.is_none());
    }
}

#[test]
fn test_entry_point_attr_to_expr() {
    check(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int { 41 + 1 }
            }"},
        "",
        &expect![[r#"
            Expr 12 [0-0] [Type Int]: Call:
                Expr 11 [50-54] [Type (Unit => Int)]: Var: Item 1 (Package 1)
                Expr 10 [54-56] [Type Unit]: Unit"#]],
    );
}

#[test]
fn test_entry_point_attr_missing_implies_main() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Int { 41 + 1 }
            }"},
        "",
        &expect![[r#"
            Expr 12 [0-0] [Type Int]: Call:
                Expr 11 [32-36] [Type (Unit => Int)]: Var: Item 1 (Package 1)
                Expr 10 [36-38] [Type Unit]: Unit"#]],
    );
}

#[test]
fn test_entry_point_attr_missing_implies_main_alernate_casing_not_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation main() : Int { 41 + 1 }
            }"},
        "",
        &expect![[r#"
            [
                EntryPoint(
                    NotFound,
                ),
            ]
        "#]],
    );
}

#[test]
fn test_entry_point_attr_missing_without_main_error() {
    check(
        indoc! {"
            namespace Test {
                operation Main2() : Int { 41 + 1 }
            }"},
        "",
        &expect![[r#"
            [
                EntryPoint(
                    NotFound,
                ),
            ]
        "#]],
    );
}

#[test]
fn test_entry_point_attr_multiple() {
    check(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int { 41 + 1 }

                @EntryPoint()
                operation Main2() : Int { 40 + 1 }
            }"},
        "",
        &expect![[r#"
            [
                EntryPoint(
                    Duplicate(
                        "Main",
                        Span {
                            lo: 50,
                            hi: 54,
                        },
                    ),
                ),
                EntryPoint(
                    Duplicate(
                        "Main2",
                        Span {
                            lo: 107,
                            hi: 112,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn test_entry_point_main_multiple() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Int { 41 + 1 }
            }
            namespace Test2 {
                operation Main() : Int { 40 + 1 }
            }"},
        "",
        &expect![[r#"
            [
                EntryPoint(
                    Duplicate(
                        "Main",
                        Span {
                            lo: 32,
                            hi: 36,
                        },
                    ),
                ),
                EntryPoint(
                    Duplicate(
                        "Main",
                        Span {
                            lo: 90,
                            hi: 94,
                        },
                    ),
                ),
            ]
        "#]],
    );
}
