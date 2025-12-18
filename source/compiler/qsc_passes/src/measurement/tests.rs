// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes)]

use crate::measurement::validate_measurement_declarations;
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};

fn check(file: &str, expr: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let package_id = store.new_package_id();
    let unit = compile(
        &store,
        &[],
        sources,
        package_id,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );

    let errors = validate_measurement_declarations(&unit.package);
    expect.assert_debug_eq(&errors);
}

#[test]
fn test_custom_measurement_declaration_with_simulatable_intrinsic_attr() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                @SimulatableIntrinsic()
                operation Mx(target: Qubit) : Result {
                    H(target);
                    M(target)
                }
            }"},
        "",
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_custom_measurement_declaration() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx(target: Qubit) : Result { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_measurement_with_no_arguments_error() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx() : Result { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NoArguments(
                    Span {
                        lo: 53,
                        hi: 55,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_non_intrinsic_measurement_error() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx(target: Qubit) : Result { M(target) }
            }"},
        "",
        &expect![[r#"
            [
                NotIntrinsic(
                    Span {
                        lo: 51,
                        hi: 53,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_non_qubit_arguments_error() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx(target: Qubit, n: Int) : Result { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NonQubitArgument(
                    Span {
                        lo: 53,
                        hi: 76,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_unit_output_error() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx(target: Qubit) : Unit { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NonResultOutput(
                    Span {
                        lo: 41,
                        hi: 95,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_non_result_output_error() {
    check(
        indoc! {"
            namespace Test {
                @Measurement()
                operation Mx(target: Qubit) : (Result, Int) { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NonResultOutput(
                    Span {
                        lo: 51,
                        hi: 53,
                    },
                ),
            ]
        "#]],
    );
}
