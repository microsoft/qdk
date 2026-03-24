// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes)]

use crate::reset::validate_reset_declarations;
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};

fn check(file: &str, expr: &str, expect: &Expect) {
    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );

    let errors = validate_reset_declarations(&unit.package);
    expect.assert_debug_eq(&errors);
}

#[test]
fn test_custom_reset_declaration_with_simulatable_intrinsic_attr() {
    check(
        indoc! {"
            namespace Test {
                @Reset()
                @SimulatableIntrinsic()
                operation CustomReset(target: Qubit) : Unit {
                    Reset(target);
                }
            }"},
        "",
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_custom_reset_declaration() {
    check(
        indoc! {"
            namespace Test {
                @Reset()
                operation CustomReset(target: Qubit) : Unit { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn test_reset_with_no_arguments_error() {
    check(
        indoc! {"
            namespace Test {
                @Reset()
                operation CustomReset() : Unit { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NoArguments(
                    Span {
                        lo: 56,
                        hi: 58,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_non_intrinsic_reset_error() {
    check(
        indoc! {"
            namespace Test {
                @Reset()
                operation CustomReset(target: Qubit) : Unit { Reset(target); }
            }"},
        "",
        &expect![[r#"
            [
                NotIntrinsic(
                    Span {
                        lo: 45,
                        hi: 56,
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
                @Reset()
                operation CustomReset(target: Qubit, n: Int) : Unit { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NonQubitArgument(
                    Span {
                        lo: 56,
                        hi: 79,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_non_unit_output_error() {
    check(
        indoc! {"
            namespace Test {
                @Reset()
                operation CustomReset(target: Qubit) : Reset { body intrinsic; }
            }"},
        "",
        &expect![[r#"
            [
                NonUnitOutput(
                    Span {
                        lo: 35,
                        hi: 99,
                    },
                ),
            ]
        "#]],
    );
}
