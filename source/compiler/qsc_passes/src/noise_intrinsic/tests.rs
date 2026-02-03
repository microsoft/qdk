// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_intrinsic::validate_noise_intrinsic_declarations;
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};

fn check(file: &str, expect: &Expect) {
    let sources = SourceMap::new([("test".into(), file.into())], Some("".into()));
    let unit = compile(
        &PackageStore::new(compile::core()),
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );

    let errors = validate_noise_intrinsic_declarations(&unit.package);
    expect.assert_debug_eq(&errors);
}

#[test]
fn test_noise_intrinsic_attr_on_non_intrinsic_issues_error() {
    check(
        indoc! {r#"
        namespace Test {
            @NoiseIntrinsic()
            operation Foo(q: Qubit) : Unit {
                
            }
        }
    "#},
        &expect![[r#"
            [
                NotIntrinsic(
                    Span {
                        lo: 54,
                        hi: 57,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn test_noise_intrinsic_with_non_unit_return_issues_error() {
    check(
        indoc! {r#"
        namespace Test {
            @NoiseIntrinsic()
            operation Foo(q: Qubit) : Int {
                body intrinsic;
            }
        }
    "#},
        &expect![[r#"
            [
                NonUnitOutput(
                    Span {
                        lo: 44,
                        hi: 105,
                    },
                ),
            ]
        "#]],
    );
}
