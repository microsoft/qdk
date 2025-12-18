// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;
use qsc_data_structures::{
    functors::FunctorApp, language_features::LanguageFeatures, source::SourceMap,
    target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{PackageStore, compile, core, std};
use qsc_hir::hir::{Item, ItemKind};

fn compile_one_operation(code: &str) -> (Item, String) {
    let core_pkg = core();
    let mut store = PackageStore::new(core_pkg);
    let std_id = store.new_package_id();
    store.insert(std_id, std(std_id, &store, TargetCapabilityFlags::empty()));

    let sources = SourceMap::new([("test".into(), code.into())], None);
    let test_id = store.new_package_id();
    let unit = compile(
        &store,
        &[(std_id, None)],
        sources,
        test_id,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    let mut callables = unit.package.items.values().filter_map(|i| {
        if let ItemKind::Callable(decl) = &i.kind {
            Some((i, decl.name.name.clone()))
        } else {
            None
        }
    });
    let mut namespaces = unit.package.items.values().filter_map(|i| {
        if let ItemKind::Namespace(ident, _) = &i.kind {
            Some(ident.clone())
        } else {
            None
        }
    });
    let (only_callable, callable_name) = callables.next().expect("Expected exactly one callable");
    assert!(callables.next().is_none(), "Expected exactly one callable");
    let only_namespace = namespaces.next().expect("Expected exactly one namespace");
    assert!(
        namespaces.next().is_none(),
        "Expected exactly one namespace"
    );
    (
        only_callable.clone(),
        format!("{}.{callable_name}", only_namespace.name()),
    )
}

#[test]
fn no_params() {
    let (item, operation) = compile_one_operation(
        r"
        namespace Test {
            operation Test() : Result[] {
            }
        }
    ",
    );
    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation);
    expect![[r#"
        Ok(
            "{\n            use qs = Qubit[0];\n            (Test.Test)();\n            let r: Result[] = [];\n            r\n        }",
        )
    "#]]
    .assert_debug_eq(&expr);
}

#[test]
fn non_qubit_params() {
    let (item, operation) = compile_one_operation(
        r"
        namespace Test {
            operation Test(q1: Qubit, q2: Qubit, i: Int) : Result[] {
            }
        }
    ",
    );
    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation);
    expect![[r#"
        Err(
            NoQubitParameters,
        )
    "#]]
    .assert_debug_eq(&expr);
}

#[test]
fn non_qubit_array_param() {
    let (item, operation) = compile_one_operation(
        r"
        namespace Test {
            operation Test(q1: Qubit[], q2: Qubit[][], i: Int[]) : Result[] {
            }
        }
    ",
    );
    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation);
    expect![[r#"
        Err(
            NoQubitParameters,
        )
    "#]]
    .assert_debug_eq(&expr);
}

#[test]
fn qubit_params() {
    let (item, operation) = compile_one_operation(
        r"
        namespace Test {
            operation Test(q1: Qubit, q2: Qubit) : Result[] {
            }
        }
    ",
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation)
        .expect("expression expected");

    expect![[r"
        {
                    use qs = Qubit[2];
                    (Test.Test)(qs[0], qs[1]);
                    let r: Result[] = [];
                    r
                }"]]
    .assert_eq(&expr);
}

#[test]
fn qubit_array_params() {
    let (item, operation) = compile_one_operation(
        r"
        namespace Test {
            operation Test(q1: Qubit[], q2: Qubit[][], q3: Qubit[][][], q: Qubit) : Result[] {
            }
        }
    ",
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation)
        .expect("expression expected");

    expect![[r"
        {
                    use qs = Qubit[15];
                    (Test.Test)(qs[0..1], Microsoft.Quantum.Arrays.Chunks(2, qs[2..5]), Microsoft.Quantum.Arrays.Chunks(2, Microsoft.Quantum.Arrays.Chunks(2, qs[6..13])), qs[14]);
                    let r: Result[] = [];
                    r
                }"]].assert_eq(&expr);
}
