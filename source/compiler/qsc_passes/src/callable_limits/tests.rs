// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::visit::Visitor;

use crate::callable_limits::CallableLimits;

fn check(file: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let package_id = store.new_package_id();
    let unit = compile(
        &store,
        &[],
        sources,
        package_id,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let mut call_limits = CallableLimits::default();
    call_limits.visit_package(&unit.package);
    let errors = call_limits.errors;
    expect.assert_debug_eq(&errors);
}

#[test]
fn funcs_cannot_use_conj() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit {
                    within {} apply {}
                }
            }
        "},
        &expect![[r#"
            [
                Conjugate(
                    Span {
                        lo: 51,
                        hi: 69,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn funcs_cannot_have_functors() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit is Adj {}
            }
        "},
        &expect![[r#"
            [
                Functor(
                    Span {
                        lo: 30,
                        hi: 31,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn funcs_cannot_call_ops() {
    check(
        indoc! {"
            namespace Test {
                operation A() : Unit {}
                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            [
                OpCall(
                    Span {
                        lo: 79,
                        hi: 82,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn funcs_cannot_allocate_qubits() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit {
                    use q = Qubit();
                }
            }
        "},
        &expect![[r#"
            [
                QubitAlloc(
                    Span {
                        lo: 51,
                        hi: 67,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn funcs_cannot_use_repeat() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit {
                    repeat {} until true;
                }
            }
        "},
        &expect![[r#"
            [
                Repeat(
                    Span {
                        lo: 51,
                        hi: 71,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn funcs_cannot_have_specs() {
    check(
        indoc! {"
            namespace Test {
                function A() : Unit {
                    body ... {}
                    adjoint self;
                }
            }
        "},
        &expect![[r#"
            [
                Spec(
                    Span {
                        lo: 21,
                        hi: 90,
                    },
                ),
                Functor(
                    Span {
                        lo: 30,
                        hi: 31,
                    },
                ),
            ]
        "#]],
    );
}
