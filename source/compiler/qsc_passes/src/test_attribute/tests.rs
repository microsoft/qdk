// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{validate::Validator, visit::Visitor};

use crate::test_attribute::validate_test_attributes;

fn check(file: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let package_id = store.new_package_id();
    let mut unit = compile(
        &store,
        &[],
        sources,
        package_id,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = validate_test_attributes(&mut unit.package);
    Validator::default().visit_package(&unit.package);
    if errors.is_empty() {
        expect.assert_eq(&unit.package.to_string());
    } else {
        expect.assert_debug_eq(&errors);
    }
}

#[test]
fn callable_cant_have_params() {
    check(
        indoc! {"
        namespace test {
            @Test()
            operation A(q : Qubit) : Unit {

            }
        }
        "},
        &expect![[r#"
            [
                CallableHasParameters(
                    Span {
                        lo: 33,
                        hi: 71,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn callable_cant_have_type_params() {
    check(
        indoc! {"
        namespace test {
            @Test()
            operation A<'T>() : Unit {

            }
        }
        "},
        &expect![[r#"
            [
                CallableHasTypeParameters(
                    Span {
                        lo: 43,
                        hi: 44,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn conditionally_compile_out_test() {
    check(
        indoc! {"
        namespace test {
            @Test()
            @Config(Base)
            operation A<'T>() : Unit {

            }
        }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-86] (Public):
                    Namespace (Ident 0 [10-14] "test"): <empty>"#]],
    );
}

#[test]
fn callable_is_valid_test_callable() {
    check(
        indoc! {"
        namespace test {
            @Test()
            operation A() : Unit {

            }
        }
        "},
        &expect![[r#"
            Package:
                Item 0 [0-64] (Public):
                    Namespace (Ident 5 [10-14] "test"): Item 1
                Item 1 [21-62] (Internal):
                    Parent: 0
                    Test
                    Callable 0 [33-62] (operation):
                        name: Ident 1 [43-44] "A"
                        input: Pat 2 [44-46] [Type Unit]: Unit
                        output: Unit
                        functors: empty set
                        body: SpecDecl 3 [33-62]: Impl:
                            Block 4 [54-62]: <empty>
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}
