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
    let std = std(&store, TargetCapabilityFlags::empty());
    let std = store.insert(std);

    let sources = SourceMap::new([("test".into(), code.into())], None);
    let unit = compile(
        &store,
        &[(std, None)],
        sources,
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
fn input_sizes_apply_to_flattened_array_dimensions() {
    let (item, operation) = compile_one_operation(
        r#"
        namespace Test {
            @CircuitRenderingOptions(inputSizes=[1, 2, 3, 4, 5])
            operation Test(q: Qubit, q1: Qubit[][], q2: Qubit[], q3: Qubit[][]) : Result[] {
            }
        }
    "#,
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation)
        .expect("expression expected");

    expect![[r#"
        {
                    use qs = Qubit[26];
                    (Test.Test)(qs[0], [qs[1..2]], qs[3..5], [qs[6..10], qs[11..15], qs[16..20], qs[21..25]]);
                    let r: Result[] = [];
                    r
                }"#]]
    .assert_eq(&expr);
}

#[test]
fn extra_input_sizes_are_ignored() {
    let (item, operation) = compile_one_operation(
        r#"
        namespace Test {
            @CircuitRenderingOptions(inputSizes=[3, 4, 9])
            operation Test(q1: Qubit[], q2: Qubit[]) : Result[] {
            }
        }
    "#,
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation)
        .expect("expression expected");

    expect![[r"
        {
                    use qs = Qubit[7];
                    (Test.Test)(qs[0..2], qs[3..6]);
                    let r: Result[] = [];
                    r
                }"]]
    .assert_eq(&expr);
}

#[test]
fn missing_input_sizes_use_default() {
    let (item, operation) = compile_one_operation(
        r#"
        namespace Test {
            @CircuitRenderingOptions(inputSizes=[3])
            operation Test(q1: Qubit[][], q2: Qubit[]) : Result[] {
            }
        }
    "#,
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation)
        .expect("expression expected");

    expect![[r"
        {
                    use qs = Qubit[8];
                    (Test.Test)([qs[0..1], qs[2..3], qs[4..5]], qs[6..7]);
                    let r: Result[] = [];
                    r
                }"]]
    .assert_eq(&expr);
}

#[test]
fn excessive_input_sizes_are_rejected() {
    let (item, operation) = compile_one_operation(
        r#"
        namespace Test {
            @CircuitRenderingOptions(inputSizes=[101, 100])
            operation Test(qs: Qubit[][]) : Result[] {
            }
        }
    "#,
    );

    let expr = entry_expr_for_qubit_operation(&item, FunctorApp::default(), &operation);

    expect![[r#"
        Err(
            TooManyQubits,
        )
    "#]]
    .assert_debug_eq(&expr);
}

#[test]
fn qubit_array_parameters_allocate_flat_register_slices() {
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
                    (Test.Test)(qs[0..1], [qs[2..3], qs[4..5]], [[qs[6..7], qs[8..9]], [qs[10..11], qs[12..13]]], qs[14]);
                    let r: Result[] = [];
                    r
                }"]].assert_eq(&expr);
}
