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

fn compile_entry_candidate_no_errors(source: &str, entry: Option<&str>) -> compile::CompileUnit {
    let unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        SourceMap::new([("test".into(), source.into())], entry.map(Into::into)),
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    unit
}

fn compile_entry_candidates(source: &str, entry: Option<&str>) -> compile::CompileUnit {
    compile(
        &PackageStore::new(compile::core()),
        &[],
        SourceMap::new([("test".into(), source.into())], entry.map(Into::into)),
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    )
}

#[test]
fn generated_entries_reject_unresolved_type_parameters() {
    use miette::Diagnostic;

    for (parameters, body) in [
        ("'T", "'T { fail \"unreachable\" }"),
        ("'T", "Int { 42 }"),
        ("'T : Eq", "'T { fail \"unreachable\" }"),
        ("'T : Show + Eq", "'T[] { fail \"unreachable\" }"),
        ("'T : Show, 'U : Eq", "('T, 'U) { fail \"unreachable\" }"),
        ("'T : Eq, 'U : Show", "Int { 42 }"),
        ("'T : Add", "'T { fail \"unreachable\" }"),
        ("'T : Add", "Unit {}"),
        ("'T : Eq + Add", "Unit {}"),
        ("'T : Exp[Int]", "Unit {}"),
        ("'T : Iterable[Bool]", "Unit {}"),
        ("'T : Eq, 'U : Add", "Unit {}"),
        ("'T, 'U : Iterable['T]", "Unit {}"),
    ] {
        for attribute in ["", "@EntryPoint() "] {
            for kind in ["function", "operation"] {
                let source = format!("{attribute}{kind} Main<{parameters}>() : {body}");
                for package_type in [PackageType::Lib, PackageType::Exe] {
                    let mut unit = compile_entry_candidates(&source, None);
                    let before = unit.package.to_string();
                    let errors =
                        generate_entry_expr(&mut unit.package, &mut unit.assigner, package_type);
                    let [crate::Error::EntryPoint(super::Error::MissingTypeArguments(span))] =
                        errors.as_slice()
                    else {
                        panic!("{source}: {errors:?}");
                    };
                    assert_eq!(&source[span.lo as usize..span.hi as usize], "Main");
                    assert_eq!(
                        errors[0].code().expect("diagnostic code").to_string(),
                        "Qdk.Qsc.EntryPoint.MissingTypeArguments"
                    );

                    assert!(
                        errors[0]
                            .help()
                            .expect("concrete entry guidance")
                            .to_string()
                            .contains("concrete type evidence")
                    );
                    assert!(unit.package.entry.is_none());
                    assert_eq!(unit.package.to_string(), before);
                }
            }
        }
    }
}

#[test]
fn attributed_entry_ignores_unselected_generic_main() {
    use qsc_hir::{
        hir::{ExprKind, ItemKind, Res},
        ty::{Prim, Ty},
    };
    for package_type in [PackageType::Lib, PackageType::Exe] {
        let mut unit = compile_entry_candidate_no_errors(
            r#"
            function Main<'T : Add>() : 'T { fail "not the entry" }
            @EntryPoint()
            function Run() : Int { 42 }
            "#,
            None,
        );
        assert!(
            generate_entry_expr(&mut unit.package, &mut unit.assigner, package_type).is_empty()
        );
        let entry = unit.package.entry.as_ref().expect("generated entry");
        assert_eq!(entry.ty, Ty::Prim(Prim::Int));
        let ExprKind::Call(callee, _) = &entry.kind else {
            panic!("generated entry call");
        };
        let ExprKind::Var(Res::Item(target), arguments) = &callee.kind else {
            panic!("generated entry callee");
        };
        let ItemKind::Callable(declaration) =
            &unit.package.items.get(target.item).expect("target").kind
        else {
            panic!("entry declaration");
        };
        assert_eq!(declaration.name.name.as_ref(), "Run");
        assert!(arguments.is_empty());
    }
}

#[test]
fn explicit_entry_preserves_inferred_type_arguments() {
    use qsc_hir::{
        hir::ExprKind,
        ty::{GenericArg, Prim, Ty},
    };
    for package_type in [PackageType::Lib, PackageType::Exe] {
        let mut unit = compile_entry_candidate_no_errors(
            r#"
            namespace Test {
                @EntryPoint()
                function Main<'T : Add>() : 'T { fail "not the entry" }
                function Identity<'T : Add>(value : 'T) : 'T { value }
            }
            "#,
            Some("Test.Identity(42)"),
        );
        let before = unit
            .package
            .entry
            .as_ref()
            .expect("explicit entry")
            .to_string();
        assert!(
            generate_entry_expr(&mut unit.package, &mut unit.assigner, package_type).is_empty()
        );
        let entry = unit.package.entry.as_ref().expect("explicit entry");
        assert_eq!(entry.to_string(), before);
        let ExprKind::Call(callee, _) = &entry.kind else {
            panic!("explicit call");
        };
        let ExprKind::Var(_, arguments) = &callee.kind else {
            panic!("explicit callee");
        };
        assert_eq!(arguments, &[GenericArg::Ty(Ty::Prim(Prim::Int))]);
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
