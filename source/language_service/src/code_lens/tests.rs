// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::get_code_lenses;
use crate::{
    Encoding,
    compilation::Compilation,
    protocol::OpenQasmMode,
    test_utils::{
        compile_notebook_with_fake_stdlib, compile_with_fake_stdlib_and_markers_no_cursor,
    },
};
use expect_test::{Expect, expect};
use qsc::PackageType;
use std::sync::Arc;

fn check(source_with_markers: &str, expect: &Expect) {
    let (compilation, expected_code_lens_ranges) =
        compile_with_fake_stdlib_and_markers_no_cursor(source_with_markers, true);
    let mut actual_code_lenses = get_code_lenses(&compilation, "<source>", Encoding::Utf8);

    for expected_range in &expected_code_lens_ranges {
        assert!(
            actual_code_lenses
                .iter()
                .any(|cl| cl.range == *expected_range),
            "expected range not found in actual code lenses: {expected_range:?}"
        );
    }

    for actual_range in actual_code_lenses.iter().map(|cl| cl.range) {
        assert!(
            expected_code_lens_ranges.iter().any(|r| r == &actual_range),
            "got code lens for unexpected range: {actual_range:?}"
        );
    }

    let actual = expected_code_lens_ranges
        .into_iter()
        .enumerate()
        .map(move |(i, r)| {
            actual_code_lenses.sort_by_key(|cl| cl.range == r);
            let partition_point = actual_code_lenses.partition_point(|cl| cl.range != r);
            let for_this_range = actual_code_lenses.drain(partition_point..);
            (i, for_this_range.map(|cl| cl.command).collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();
    expect.assert_debug_eq(&actual);
}

#[test]
fn one_entrypoint() {
    check(
        r#"
        namespace Test {
            @EntryPoint()
            ◉operation Test() : Unit{
            }◉
        }"#,
        &expect![[r#"
            [
                (
                    0,
                    [
                        Run(
                            "Test.Test()",
                        ),
                        Histogram(
                            "Test.Test()",
                        ),
                        Estimate(
                            "Test.Test()",
                        ),
                        Debug(
                            "Test.Test()",
                        ),
                        Circuit(
                            OperationInfo {
                                operation: "Test.Test",
                                total_num_qubits: 0,
                            },
                        ),
                    ],
                ),
            ]
        "#]],
    );
}

#[test]
fn main_function() {
    check(
        r#"
        namespace Test {
            ◉operation Main() : Unit {
            }◉

            ◉operation Foo() : Unit{
            }◉
        }"#,
        &expect![[r#"
            [
                (
                    0,
                    [
                        Run(
                            "Test.Main()",
                        ),
                        Histogram(
                            "Test.Main()",
                        ),
                        Estimate(
                            "Test.Main()",
                        ),
                        Debug(
                            "Test.Main()",
                        ),
                        Circuit(
                            OperationInfo {
                                operation: "Test.Main",
                                total_num_qubits: 0,
                            },
                        ),
                    ],
                ),
                (
                    1,
                    [
                        Run(
                            "Test.Foo()",
                        ),
                        Histogram(
                            "Test.Foo()",
                        ),
                        Estimate(
                            "Test.Foo()",
                        ),
                        Debug(
                            "Test.Foo()",
                        ),
                        Circuit(
                            OperationInfo {
                                operation: "Test.Foo",
                                total_num_qubits: 0,
                            },
                        ),
                    ],
                ),
            ]
        "#]],
    );
}

#[test]
fn no_entrypoint_code_lens_in_notebook() {
    let compilation = compile_notebook_with_fake_stdlib(
        [(
            "cell1",
            "@EntryPoint()
            operation Main() : Unit {}",
        )]
        .into_iter(),
    );

    let lenses = get_code_lenses(&compilation, "cell1", Encoding::Utf8);
    assert!(
        lenses.is_empty(),
        "entrypoint code lenses should not be present in notebooks"
    );
}

#[test]
fn qubit_operation_circuit() {
    check(
        r#"
        namespace Test {
            ◉operation Foo(q: Qubit) : Unit {
            }◉
        }"#,
        &expect![[r#"
            [
                (
                    0,
                    [
                        Circuit(
                            OperationInfo {
                                operation: "Test.Foo",
                                total_num_qubits: 1,
                            },
                        ),
                    ],
                ),
            ]
        "#]],
    );
}

#[test]
fn qubit_arrays_operation_circuit() {
    check(
        r#"
        namespace Test {
            ◉operation Foo(q: Qubit, q1: Qubit[], q2: Qubit[][]) : Unit {
            }◉
        }"#,
        &expect![[r#"
            [
                (
                    0,
                    [
                        Circuit(
                            OperationInfo {
                                operation: "Test.Foo",
                                total_num_qubits: 7,
                            },
                        ),
                    ],
                ),
            ]
        "#]],
    );
}

#[test]
fn no_code_lenses_with_compilation_errors() {
    let source = r#"
        namespace Test {
            operation Main() : Unit {
                foo  // undefined variable - compilation error
            }
        }"#;

    let (compilation, _) = compile_with_fake_stdlib_and_markers_no_cursor(source, true);

    // Verify the compilation actually has errors
    assert!(
        !compilation.compile_errors.is_empty(),
        "Test should have compilation errors"
    );

    let lenses = get_code_lenses(&compilation, "<source>", Encoding::Utf8);
    assert!(
        lenses.is_empty(),
        "code lenses should not be present when there are compilation errors"
    );
}

const OPENQASM_PROGRAM: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
"#;

fn openqasm_lenses(mode: OpenQasmMode) -> Vec<crate::protocol::CodeLens> {
    let compilation = Compilation::new_qasm(
        PackageType::Exe,
        vec![(Arc::from("<source>"), Arc::from(OPENQASM_PROGRAM))],
        vec![],
        &Arc::from("test project"),
        mode,
    );
    get_code_lenses(&compilation, "<source>", Encoding::Utf8)
}

#[test]
fn no_code_lenses_for_openqasm_in_spec_mode() {
    // The same program in qdk mode has lenses, so the suppression is the mode's
    // doing rather than the program having nothing to offer.
    assert!(!openqasm_lenses(OpenQasmMode::Qdk).is_empty());
    assert!(openqasm_lenses(OpenQasmMode::Spec).is_empty());
}
