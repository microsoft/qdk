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

fn compile_entry_candidate(source: &str, entry: Option<&str>) -> compile::CompileUnit {
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

/// Generated calls must carry the validated defaults even for parameters absent
/// from their result type. The declaration remains generic for other callers.
#[test]
fn generated_generic_entries_preserve_concrete_signatures_and_type_arguments() {
    use qsc_hir::{
        hir::{ExprKind, ItemKind},
        ty::{GenericArg, Ty},
    };
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
        (
            "function Main<'T : Eq, 'U : Show>() : Int { 42 }",
            Ty::Prim(qsc_hir::ty::Prim::Int),
        ),
    ] {
        let mut unit = compile_entry_candidate(source, None);
        let original_declaration = unit
            .package
            .items
            .values()
            .find_map(|item| {
                if let ItemKind::Callable(decl) = &item.kind {
                    Some(decl.clone())
                } else {
                    None
                }
            })
            .expect("entry declaration");
        assert!(
            generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe).is_empty()
        );
        let entry = unit.package.entry.as_ref().expect("generated entry");
        assert_eq!(entry.ty, output);
        let ExprKind::Call(callee, args) = &entry.kind else {
            panic!("generated call")
        };
        let Ty::Arrow(arrow) = &callee.ty else {
            panic!("callee must be arrow-typed")
        };
        assert_eq!(*arrow.input.borrow(), args.ty);
        assert_eq!(*arrow.output.borrow(), output);
        let ExprKind::Var(_, generic_args) = &callee.kind else {
            panic!("generated callee");
        };
        assert_eq!(
            generic_args,
            &vec![GenericArg::Ty(Ty::UNIT); original_declaration.generics.len()],
            "{source}"
        );
        let declaration = unit
            .package
            .items
            .values()
            .find_map(|item| {
                if let ItemKind::Callable(decl) = &item.kind {
                    Some(decl)
                } else {
                    None
                }
            })
            .expect("entry declaration");
        assert_eq!(declaration.generics, original_declaration.generics);
        assert_eq!(declaration.output, original_declaration.output);
    }
}

/// There is no argument or expected-result evidence at a generated entry.
/// Reject defaults that fail bounds rather than arbitrarily choosing another type.
#[test]
fn generated_generic_entries_reject_unsatisfied_defaults_at_the_entry_name() {
    use miette::Diagnostic;

    for (parameters, body, rejected) in [
        ("'T : Add", "'T { fail \"unreachable\" }", "'T"),
        ("'T : Add", "Unit {}", "'T"),
        ("'T : Eq + Add", "Unit {}", "'T"),
        ("'T : Exp[Int]", "Unit {}", "'T"),
        ("'T : Iterable[Bool]", "Unit {}", "'T"),
        ("'T : Eq, 'U : Add", "Unit {}", "'U"),
        ("'T, 'U : Iterable['T]", "Unit {}", "'U"),
    ] {
        let source = format!("@EntryPoint() operation Main<{parameters}>() : {body}");
        let mut unit = compile_entry_candidate(&source, None);
        let errors = generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe);
        let [crate::Error::EntryPoint(super::Error::GenericBounds(name, span))] = errors.as_slice()
        else {
            panic!("Unit cannot satisfy {parameters}: {errors:?}")
        };
        assert_eq!(name, rejected);
        assert_eq!(&source[span.lo as usize..span.hi as usize], "Main");
        assert_eq!(
            errors[0].code().expect("diagnostic code").to_string(),
            "Qdk.Qsc.EntryPoint.GenericBounds"
        );
        assert_eq!(
            errors[0].to_string(),
            format!(
                "cannot infer a concrete entry-point type satisfying the bounds of `{rejected}`"
            )
        );
        assert!(unit.package.entry.is_none());
    }
}

/// Defaults apply only to the selected entry. The unused Main requires a type
/// supporting Add, but must not prevent an explicitly attributed entry from running.
#[test]
fn attributed_entry_defaults_do_not_apply_to_unselected_generic_main() {
    use qsc_hir::{
        hir::{ExprKind, ItemKind, Res},
        ty::{GenericArg, Prim, Ty},
    };
    let mut unit = compile_entry_candidate(
        r#"
        function Main<'T : Add>() : 'T { fail "not the entry" }
        @EntryPoint()
        function Run<'U : Eq>() : Int { 42 }
        "#,
        None,
    );
    assert!(
        generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe).is_empty()
    );
    let entry = unit.package.entry.as_ref().expect("generated entry");
    assert_eq!(entry.ty, Ty::Prim(Prim::Int));
    let ExprKind::Call(callee, _) = &entry.kind else {
        panic!("generated entry call");
    };
    let ExprKind::Var(Res::Item(target), args) = &callee.kind else {
        panic!("generated entry callee");
    };
    let ItemKind::Callable(decl) = &unit.package.items.get(target.item).expect("target").kind
    else {
        panic!("entry declaration");
    };
    assert_eq!(decl.name.name.as_ref(), "Run");
    assert_eq!(args, &[GenericArg::Ty(Ty::UNIT)]);
}

/// An explicit call supplies its own type evidence, even if the file also has
/// a generic Main whose bounds the generated-entry defaults could not satisfy.
#[test]
fn explicit_entry_bypasses_defaulting_of_a_constrained_generic_main() {
    use qsc_hir::{
        hir::ExprKind,
        ty::{GenericArg, Prim, Ty},
    };

    let mut unit = compile_entry_candidate(
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
        generate_entry_expr(&mut unit.package, &mut unit.assigner, PackageType::Exe).is_empty()
    );
    let entry = unit.package.entry.as_ref().expect("explicit entry");
    assert_eq!(entry.to_string(), before);
    let ExprKind::Call(callee, _) = &entry.kind else {
        panic!("explicit call")
    };
    let ExprKind::Var(_, args) = &callee.kind else {
        panic!("explicit callee")
    };
    assert_eq!(args, &[GenericArg::Ty(Ty::Prim(Prim::Int))]);
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
