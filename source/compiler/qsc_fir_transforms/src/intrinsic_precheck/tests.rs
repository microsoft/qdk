// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::PipelineStage;
use crate::test_utils::compile_and_run_pipeline_to_with_errors;
use expect_test::{Expect, expect};
use indoc::indoc;
use miette::Diagnostic;

fn check_precheck_errors(source: &str, expect: &Expect) {
    let (_, _, result) = compile_and_run_pipeline_to_with_errors(source, PipelineStage::Mono);
    let error_text: String = result
        .errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    expect.assert_eq(&error_text);
}

#[test]
fn unsupported_param_type_has_diagnostic_code() {
    let error = Error::UnsupportedParamType(
        "MyOp".to_string(),
        "(Int, Int)".to_string(),
        Span::default(),
    );
    let code = error.code().expect("should have diagnostic code");
    assert_eq!(
        code.to_string(),
        "Qsc.FirTransform.UnsupportedIntrinsicParamType"
    );
}

#[test]
fn unsupported_return_type_has_diagnostic_code() {
    let error = Error::UnsupportedReturnType(
        "MyOp".to_string(),
        "(Int, Int)".to_string(),
        Span::default(),
    );
    let code = error.code().expect("should have diagnostic code");
    assert_eq!(
        code.to_string(),
        "Qsc.FirTransform.UnsupportedIntrinsicReturnType"
    );
}

#[test]
fn intrinsic_with_tuple_param() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(pair : (Int, Int)) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { Foo((1, 2)); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported parameter type `(Int, Int)`"],
    );
}

#[test]
fn intrinsic_with_udt_param() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    struct MyPair { First : Int, Second : Int }
                    operation Foo(pair : MyPair) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { Foo(new MyPair { First = 1, Second = 2 }); }
                }
            "#},
        &expect![
            "intrinsic callable `Foo` has unsupported parameter type `UDT<Item 1 (Package 2)>`"
        ],
    );
}

#[test]
fn intrinsic_returning_tuple() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo() : (Int, Int) { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo(); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported return type `(Int, Int)`"],
    );
}

#[test]
fn intrinsic_returning_udt() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    struct MyPair { First : Int, Second : Int }
                    operation Foo() : MyPair { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo(); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported return type `UDT<Item 1 (Package 2)>`"],
    );
}

#[test]
fn simulatable_intrinsic_with_tuple_param() {
    // The FIR-transform precheck validates `@SimulatableIntrinsic` callables in
    // addition to `body intrinsic` ones (see the
    // `Intrinsic | SimulatableIntrinsic(_)` gate in intrinsic_precheck.rs). A
    // `@SimulatableIntrinsic` operation with a tuple parameter type is therefore
    // rejected here as an unsupported parameter type.
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    @SimulatableIntrinsic()
                    operation Foo(pair : (Int, Int)) : Unit {}
                    @EntryPoint()
                    operation Main() : Unit { Foo((1, 2)); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported parameter type `(Int, Int)`"],
    );
}

#[test]
fn simulatable_intrinsic_with_udt_param() {
    // The FIR-transform precheck validates `@SimulatableIntrinsic` callables in
    // addition to `body intrinsic` ones (see the
    // `Intrinsic | SimulatableIntrinsic(_)` gate in intrinsic_precheck.rs). A
    // `@SimulatableIntrinsic` operation with a UDT parameter type is therefore
    // rejected here as an unsupported parameter type.
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    struct MyPair { First : Int, Second : Int }
                    @SimulatableIntrinsic()
                    operation Foo(pair : MyPair) : Unit {}
                    @EntryPoint()
                    operation Main() : Unit { Foo(new MyPair { First = 1, Second = 2 }); }
                }
            "#},
        &expect![
            "intrinsic callable `Foo` has unsupported parameter type `UDT<Item 1 (Package 2)>`"
        ],
    );
}

#[test]
fn simulatable_intrinsic_returning_tuple() {
    // The FIR-transform precheck validates `@SimulatableIntrinsic` callables in
    // addition to `body intrinsic` ones (see the
    // `Intrinsic | SimulatableIntrinsic(_)` gate in intrinsic_precheck.rs). A
    // `@SimulatableIntrinsic` operation with a tuple return type is therefore
    // rejected here as an unsupported return type.
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    @SimulatableIntrinsic()
                    operation Foo() : (Int, Int) { return (1, 2); }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo(); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported return type `(Int, Int)`"],
    );
}

#[test]
fn simulatable_intrinsic_returning_udt() {
    // The FIR-transform precheck validates `@SimulatableIntrinsic` callables in
    // addition to `body intrinsic` ones (see the
    // `Intrinsic | SimulatableIntrinsic(_)` gate in intrinsic_precheck.rs). A
    // `@SimulatableIntrinsic` operation with a UDT return type is therefore
    // rejected here as an unsupported return type.
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    struct MyPair { First : Int, Second : Int }
                    @SimulatableIntrinsic()
                    operation Foo() : MyPair { return new MyPair { First = 1, Second = 2 }; }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo(); }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported return type `UDT<Item 1 (Package 2)>`"],
    );
}

#[test]
fn intrinsic_with_both_unsupported_param_and_return() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(pair : (Int, Int)) : (Int, Int) { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo((1, 2)); }
                }
            "#},
        &expect![[r#"
                intrinsic callable `Foo` has unsupported parameter type `(Int, Int)`
                intrinsic callable `Foo` has unsupported return type `(Int, Int)`"#]],
    );
}

#[test]
fn intrinsic_with_primitive_param() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(q : Qubit) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {
                        use q = Qubit();
                        Foo(q);
                    }
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn intrinsic_with_multiple_primitive_params() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(a : Qubit, b : Qubit) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {
                        use q = Qubit();
                        Foo(q, q);
                    }
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn intrinsic_returning_unit() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(q : Qubit) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {
                        use q = Qubit();
                        Foo(q);
                    }
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn unreachable_intrinsic_not_checked() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(pair : (Int, Int)) : Unit { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {}
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn generic_intrinsic_with_type_param() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo<'T>(a : 'T) : 'T { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit { let _ = Foo(1); }
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn measurement_intrinsic_with_tuple_return_is_allowed() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    @Measurement()
                    operation Meas(q : Qubit) : (Result, Result) { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {
                        use q = Qubit();
                        let _ = Meas(q);
                    }
                }
            "#},
        &expect![[""]],
    );
}

#[test]
fn non_measurement_intrinsic_with_tuple_return_still_rejected() {
    check_precheck_errors(
        indoc! {r#"
                namespace Test {
                    operation Foo(q : Qubit) : (Result, Result) { body intrinsic; }
                    @EntryPoint()
                    operation Main() : Unit {
                        use q = Qubit();
                        let _ = Foo(q);
                    }
                }
            "#},
        &expect!["intrinsic callable `Foo` has unsupported return type `(Result, Result)`"],
    );
}
