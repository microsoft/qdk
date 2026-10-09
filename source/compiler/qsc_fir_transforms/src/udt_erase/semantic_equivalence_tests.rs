// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;

#[test]
fn callable_capability_weakening_preserves_constructor_shape_and_gate_behavior() {
    for body in [
        "let op : Qubit => Unit = H; op(q);",
        "let g = F(H, 1) w/ N <- 2; g::Op(q);",
        "let g = new S { Op = H, N = 1 }; g.Op(q);",
        "let ops : (Qubit => Unit)[] = [H]; ops[0](q);",
    ] {
        let source = format!(
            r#"
            newtype F = (Op : Qubit => Unit is Adj, N : Int);
            struct S {{ Op : Qubit => Unit, N : Int }}
            @EntryPoint() operation Main() : Result {{
                use q = Qubit();
                {body}
                H(q);
                MResetZ(q)
            }}
        "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(false)),
        );
        let qir = crate::test_utils::generate_qir(&source);
        assert_eq!(
            qir.matches("call void @__quantum__qis__h__body").count(),
            2,
            "{qir}"
        );
    }
}

fn check_integer_semantics_and_qir(source: &str, expected: i64) {
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(expected),
    );
    let qir = crate::test_utils::generate_qir(source);
    let records: Vec<_> = qir
        .lines()
        .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
        .collect();
    assert_eq!(records.len(), 1, "{source}\n{qir}");
    assert!(
        records[0].contains(&format!("i64 {expected},")),
        "{source}\n{qir}"
    );
}

#[test]
fn stored_constructors_preserve_value_shapes() {
    // The argument writes choice so defunctionalization must keep the call indirect.
    for (declaration, argument, result) in [
        ("newtype Data = (Value : Int);", "23", "data::Value"),
        (
            "newtype Data = (Value : (Int, Int));",
            "(2, 3)",
            "let (a, b) = data::Value; 10 * a + b",
        ),
        (
            "newtype Data = (First : Int, Second : Int);",
            "(2, 3)",
            "10 * data::First + data::Second",
        ),
        ("newtype Data = (Value : Int,);", "(23,)", "data::Value"),
        ("newtype Data = Unit;", "()", "let _ = data; 23"),
    ] {
        let source = formatdoc! {r#"
            {declaration}
            @EntryPoint() operation Main() : Int {{
                mutable choice = 0;
                let factories = [Data, Data];
                let data = factories[choice]({{ choice = 1; {argument} }});
                {result}
            }}
        "#};
        check_integer_semantics_and_qir(&source, 23);
    }
}

#[test]
fn constructor_lookup_precedes_argument_evaluation() {
    let source = r#"
        newtype Data = (Value : Int);
        @EntryPoint() operation Main() : Int {
            mutable order = 0;
            mutable choice = 0;
            let factories = [Data, Data];
            let data = factories[{ order = order * 10 + 1; choice }]({
                order = order * 10 + 2;
                choice = 1;
                23
            });
            100 * order + data::Value
        }
    "#;
    check_integer_semantics_and_qir(source, 1223);
}

#[test]
fn constructor_index_error_precedes_argument_failure() {
    let source = r#"
        newtype Data = (Value : Int);
        @EntryPoint() operation Main() : Int {
            mutable choice = 2;
            let factories = [Data, Data];
            let data = factories[choice]({ choice = 0; fail "argument" });
            data::Value
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source).expect_err("index must fail");
    assert!(error.contains("IndexOutOfRange"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn cross_package_constructors_preserve_value_shapes() {
    let first = "namespace First { export Data; newtype Data = (Value : Int); }";
    let second = r#"
        namespace Second {
            export Run;
            newtype Data = (Value : (Int, Int));
            operation Run() : Int {
                mutable choice = 0;
                let scalarFactories = [First.Data, First.Data];
                let tupleFactories = [Data, Data];
                let scalar = scalarFactories[choice]({ choice = 1; 23 });
                let tuple = tupleFactories[choice]({ choice = 0; (2, 3) });
                let (a, b) = tuple::Value;
                100 * scalar::Value + 10 * a + b
            }
        }
    "#;
    let source = "@EntryPoint() operation Main() : Int { Second.Run() }";
    let (original, package_id) =
        crate::test_utils::compile_to_fir_with_two_libraries(first, second, source);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&original, package_id),
        Ok(qsc_eval::val::Value::Int(2323))
    );
    let (transformed, package_id) =
        crate::test_utils::compile_and_run_pipeline_to_with_two_libraries(
            first,
            second,
            source,
            crate::PipelineStage::Full,
        );
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&transformed, package_id),
        Ok(qsc_eval::val::Value::Int(2323))
    );
}

#[test]
fn return_in_replacement_skips_record_evaluation() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
            newtype Box = (Value : Int);
            function Record() : Box { fail "record must not run" }
            function Legacy(stop : Bool) : Int {
                let _ = Record() w/ Value <- {
                    if stop { return 7; }
                    0
                };
                0
            }
            @EntryPoint() operation Main() : Int { Legacy(true) }
        "#,
        qsc_eval::val::Value::Int(7),
    );
}

#[test]
fn return_in_copy_source_skips_overrides() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
            struct Pair { A : Int, B : Int }
            function Copy(stop : Bool) : Int {
                let _ = new Pair {
                    ...{
                        if stop { return 8; }
                        new Pair { A = 1, B = 2 }
                    },
                    A = fail "override must not run"
                };
                0
            }
            @EntryPoint() operation Main() : Int { Copy(true) }
        "#,
        qsc_eval::val::Value::Int(8),
    );
}

#[test]
fn constructor_and_field_erasure_preserves_values() {
    for body in [
        "Data(23)::Value",
        "Outer(Data(23))::Inner::Value",
        "(Data(0) w/ Value <- 23)::Value",
        "(Outer(Data(0)) w/ Inner <- Data(23))::Inner::Value",
        "let bound = Data(23); bound::Value",
        "Pair(23, 99)::First",
    ] {
        let source = formatdoc! {r#"
            newtype Data = (Value : Int);
            newtype Outer = (Inner : Data);
            newtype Pair = (First : Int, Second : Int);
            @EntryPoint() operation Main() : Int {{ {body} }}
        "#};
        check_integer_semantics_and_qir(&source, 23);
    }
}

#[test]
fn constructor_and_field_erasure_preserves_error_spans() {
    for body in [
        "Data(fail \"expected\")::Value",
        "Outer(Data(fail \"expected\"))::Inner::Value",
        "(Source() w/ Value <- 23)::Value",
        "(Data(0) w/ Value <- (fail \"expected\"))::Value",
    ] {
        let source = formatdoc! {r#"
            newtype Data = (Value : Int);
            newtype Outer = (Inner : Data);
            function Source() : Data {{ fail "expected" }}
            @EntryPoint() operation Main() : Int {{ {body} }}
        "#};
        let (store, package) = crate::test_utils::compile_to_fir(&source);
        let (result, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
        let failure = result.expect_err("source must fail");
        assert!(
            failure.starts_with("UserFail(\"expected\","),
            "{body}: {failure}"
        );
        assert!(trace.is_empty(), "{body}: {trace:?}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn immutable_field_update_evaluates_record_once() {
    for (declaration, initial, field) in [
        (
            "newtype Data = (First : Int, Second : Int, Third : Int);",
            "Data(1, 2, 3)",
            "Second",
        ),
        ("newtype Data = (Value : Int);", "Data(1)", "Value"),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                {declaration}
                @EntryPoint()
                operation Main() : Int {{
                    mutable count = 0;
                    let _ = ({{ set count += 1; {initial} }}) w/ {field} <- 9;
                    count
                }}
            }}
        "#};
        assert_eq!(
            crate::test_utils::eval_qsharp_original(&source),
            Ok(qsc_eval::val::Value::Int(1))
        );
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn immutable_field_update_evaluates_replacement_before_record() {
    for (declaration, initial, field, value, expected) in [
        (
            "newtype Data = (First : Int, Second : Int, Third : Int);",
            "Data(1, 2, 3)",
            "Second",
            "updated::First * 10 + updated::Second",
            2119,
        ),
        (
            "newtype Data = (Value : Int);",
            "Data(1)",
            "Value",
            "updated::Value",
            2109,
        ),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                {declaration}
                @EntryPoint()
                operation Main() : Int {{
                    mutable order = 0;
                    let updated = ({{
                        set order = order * 10 + 1;
                        {initial}
                    }}) w/ {field} <- {{
                        set order = order * 10 + 2;
                        9
                    }};
                    order * 100 + {value}
                }}
            }}
        "#};
        assert_eq!(
            crate::test_utils::eval_qsharp_original(&source),
            Ok(qsc_eval::val::Value::Int(expected))
        );
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn field_update_propagates_record_failure() {
    for (declaration, field) in [
        (
            "newtype Data = (First : Int, Second : Int, Third : Int);",
            "Second",
        ),
        ("newtype Data = (Value : Int);", "Value"),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                {declaration}
                function Source() : Data {{ fail "record failure" }}
                @EntryPoint()
                operation Main() : Int {{
                    let updated = Source() w/ {field} <- 9;
                    updated::{field}
                }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let fail_span = store
            .get(package_id)
            .exprs
            .iter()
            .find_map(|(_, expr)| {
                matches!(expr.kind, qsc_fir::fir::ExprKind::Fail(_)).then_some(expr.span)
            })
            .expect("source must contain a fail expression");
        assert_eq!(
            crate::test_utils::try_eval_fir_entry(&store, package_id),
            Err(format!(
                "{:?}",
                qsc_eval::Error::UserFail(
                    "record failure".into(),
                    (
                        qsc_lowerer::map_fir_package_to_hir(fail_span.package),
                        fail_span.span
                    )
                        .into()
                )
            ))
        );
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn field_assignment_reads_record_after_replacement() {
    let source = indoc! {r#"
        namespace Test {
            newtype Data = (First : Int, Second : Int, Third : Int);
            @EntryPoint()
            operation Main() : Int {
                mutable data = Data(1, 2, 3);
                set data w/= Second <- {
                    set data = Data(4, 5, 6);
                    9
                };
                data::First * 100 + data::Second * 10 + data::Third
            }
        }
    "#};
    assert_eq!(
        crate::test_utils::eval_qsharp_original(source),
        Ok(qsc_eval::val::Value::Int(496))
    );
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn struct_fields_evaluate_in_source_order() {
    let source = indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            @EntryPoint()
            operation Main() : Int {
                mutable order = 0;
                let pair = new Pair {
                    Second = { set order = order * 10 + 2; 20 },
                    First = { set order = order * 10 + 1; 10 }
                };
                order * 1000 + pair.First * 10 + pair.Second
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(21_120)));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn struct_copy_snapshots_source_before_overrides() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
            struct Data { First : Int, Second : Int }
            @EntryPoint() operation Main() : Int {
                mutable original = new Data { First = 13, Second = 2 };
                let copied = new Data {
                    ...original,
                    First = {
                        original = new Data { First = 13, Second = 5 };
                        6
                    }
                };
                copied.Second * 100 + copied.First * 10 + original.Second
            }
        "#,
        qsc_eval::val::Value::Int(265),
    );
}

#[test]
fn callable_copy_preserves_source_snapshot() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
            struct Choice { Callable : Int -> Int, Weight : Int }
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            @EntryPoint() operation Main() : Int {
                mutable original = new Choice { Callable = Add11, Weight = 2 };
                let copied = new Choice {
                    ...original,
                    Callable = {
                        original = new Choice { Callable = Add11, Weight = 5 };
                        Times3
                    }
                };
                copied.Weight * 100 + copied.Callable(2) * 10 + original.Weight
            }
        "#,
        qsc_eval::val::Value::Int(265),
    );
}

#[test]
fn callable_copy_preserves_aliased_source_snapshot() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
            struct Choice { Callable : Int -> Int, Weight : Int }
            function Add11(value : Int) : Int { value + 11 }
            function Times3(value : Int) : Int { value * 3 }
            @EntryPoint() operation Main() : Int {
                mutable original = new Choice { Callable = Add11, Weight = 2 };
                let snapshot = original;
                let copied = new Choice {
                    ...snapshot,
                    Callable = {
                        original = new Choice { Callable = Add11, Weight = 5 };
                        Times3
                    }
                };
                copied.Weight * 100 + copied.Callable(2) * 10 + original.Weight
            }
        "#,
        qsc_eval::val::Value::Int(265),
    );
}

fn struct_copy_order_sources(
    source_tail: &str,
    replacement_tail: &str,
) -> impl Iterator<Item = String> {
    [
        (
            "struct Data { First : Int, Second : Int, Third : Int }",
            "new Data { First = 7, Second = 2, Third = 3 }",
        ),
        ("struct Data { First : Int }", "new Data { First = 7 }"),
    ]
    .into_iter()
    .map(move |(declaration, initial)| {
        formatdoc! {r#"
            {declaration}
            @EntryPoint() operation Main() : Int {{
                let original = {initial};
                mutable order = 0;
                let copied = new Data {{
                    ...{{ order = order * 10 + 1; {source_tail} }},
                    First = {{ order = order * 10 + 2; {replacement_tail} }}
                }};
                order * 100 + copied.First
            }}
        "#}
    })
}

#[test]
fn copy_source_precedes_override_evaluation() {
    for source in struct_copy_order_sources("original", "6") {
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(1206),
        );
    }
}

#[test]
fn copy_source_failure_skips_overrides() {
    for source in struct_copy_order_sources(
        r#"fail $"source-{order}""#,
        r#"fail $"replacement-{order}""#,
    ) {
        let error = crate::test_utils::eval_qsharp_original(&source).expect_err("source must fail");
        assert!(error.contains("source-1"), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn override_failure_occurs_after_copy_source() {
    for source in struct_copy_order_sources("original", r#"fail $"replacement-{order}""#) {
        let error =
            crate::test_utils::eval_qsharp_original(&source).expect_err("override must fail");
        assert!(error.contains("replacement-12"), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn field_updates_preserve_replacement_before_record() {
    for (source, expected) in super::test_cases::field_update_order_cases().chain([
        (
            super::test_cases::NESTED_FIELD_UPDATE_ORDER.to_string(),
            375,
        ),
        (super::test_cases::SINGLE_FIELD_UPDATE_ORDER.to_string(), 74),
    ]) {
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
    }
}

#[test]
fn field_updates_evaluate_record_once_after_replacement() {
    for field in ["A", "B", "C"] {
        let source = indoc::formatdoc! {r#"
            struct Triple {{ A : Int, B : Int, C : Int }}
            function Make() : Triple {{ Message("record"); new Triple {{ A=1, B=2, C=3 }} }}
            function Replace() : Int {{ Message("replace"); 7 }}
            @EntryPoint() operation Main() : Int {{
                let p=Make() w/ {field} <- Replace();
                p.A+p.B+p.C
            }}
        "#};
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn field_update_failure_precedes_record_failure() {
    let source = r#"
        struct Pair { A : Int, B : Int }
        function Make() : Pair { fail "record" }
        function Replace() : Int { fail "replace" }
        @EntryPoint() operation Main() : Int {
            let p=Make() w/ B <- Replace();
            p.A+p.B
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source).expect_err("replacement must fail");
    assert!(error.contains("replace"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn whole_value_update_still_evaluates_record() {
    let source = r#"
        newtype Only = (N : Int);
        function Make() : Only { fail "record" }
        function Replace() : Int { Message("replace"); 7 }
        @EntryPoint() operation Main() : Int {
            let p=Make() w/ N <- Replace();
            p::N
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source).expect_err("record must fail");
    assert!(error.contains("record"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn field_updates_preserve_quantum_operand_order() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        super::test_cases::QUANTUM_FIELD_UPDATE_ORDER,
        qsc_eval::val::Value::Int(103),
    );
}

#[test]
fn field_updates_preserve_cross_package_operand_order() {
    let library = r#"
        namespace Lib {
            struct Pair { A : Int, B : Int }
            function Make(n : Int) : Pair { new Pair { A=n, B=2 } }
            function Update() : Int {
                mutable n=0;
                let p=Make(n) w/ B <- {set n=3;n};
                100*p.A+p.B
            }
            export Update;
        }
    "#;
    let source = r#"
        @EntryPoint() operation Main() : Int { Lib.Update() }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(303)),
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn field_assignment_updates_single_field_array() {
    let source = r#"
        struct Config { Values : Int[] }
        @EntryPoint() operation Main() : Int {
            mutable config = new Config { Values = [1,2] };
            set config w/= Values <- [3,4];
            config.Values[0]
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(3),
    );
}

#[test]
fn nested_assignment_reads_record_after_replacement() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        newtype Triple = (A : Int, (B : Int, C : Int));
        function Make(n : Int) : Triple { Triple(n,(n+1,n+2)) }
        @EntryPoint() operation Main() : Int {
            mutable p=Make(0);
            set p w/= B <- {set p=Make(3);7};
            100*p::A+10*p::B+p::C
        }
        "#,
        qsc_eval::val::Value::Int(375),
    );
}

#[test]
fn struct_erasure_preserves_conditional_callable_field_order() {
    for (source, expected) in super::test_cases::conditional_callable_field_cases() {
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
    }
}

#[test]
fn struct_erasure_preserves_initializer_order() {
    for (source, expected) in super::test_cases::struct_initializer_order_cases() {
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
    }
}

#[test]
fn struct_erasure_preserves_copy_snapshots() {
    for (source, expected) in super::test_cases::struct_copy_snapshot_cases().chain([
        (super::test_cases::PURE_STRUCT_COPY.to_string(), 446),
        (super::test_cases::SINGLE_FIELD_COPY.to_string(), 4),
    ]) {
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
    }
}

#[test]
fn struct_erasure_preserves_initializer_messages() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        struct Pair { Head : Int, Tail : Int }
        newtype Wrapper = (Value : Pair);
        function Log(label : String, n : Int) : Int { Message(label); n }
        function Read(w : Wrapper) : Int { let p=w::Value; 100*p.Head+p.Tail }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Pair { Tail=Log("tail",8), Head=Log("head",4) }))
        }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn struct_erasure_evaluates_copy_once_before_overrides() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        struct Triple { A : Int, B : Int, C : Int }
        newtype Wrapper = (Value : Triple);
        function Original() : Triple { Message("copy"); new Triple { A=1, B=2, C=3 } }
        function Override() : Int { Message("override"); 4 }
        function Read(w : Wrapper) : Int { let p=w::Value; 100*p.A+10*p.B+p.C }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Triple { ...Original(), A=Override() }))
        }
        "#,
        qsc_eval::val::Value::Int(423),
    );
}

#[test]
fn struct_erasure_keeps_fully_overridden_copy_effects() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        struct Only { N : Int }
        newtype Wrapper = (Value : Only);
        function Original() : Only { Message("copy"); new Only { N=1 } }
        function Override() : Int { Message("override"); 4 }
        function Read(w : Wrapper) : Int { (w::Value).N }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Only { ...Original(), N=Override() }))
        }
        "#,
        qsc_eval::val::Value::Int(4),
    );
}

#[test]
fn struct_erasure_preserves_nested_initializer_order() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        struct Pair { Head : Int, Tail : Int }
        struct Outer { Pair : Pair, Last : Int }
        newtype Wrapper = (Value : Outer);
        function Read(w : Wrapper) : Int {
            let p=w::Value;
            10000*p.Pair.Head+100*p.Pair.Tail+p.Last
        }
        @EntryPoint() operation Main() : Int {
            mutable n=0;
            Read(Wrapper(new Outer {
                Last={set n=9;n},
                Pair=new Pair { Tail={set n=8;n}, Head=n }
            }))
        }
        "#,
        qsc_eval::val::Value::Int(80809),
    );
}

#[test]
fn struct_erasure_preserves_cross_package_initializer_order() {
    crate::test_utils::check_semantic_equivalence_with_library(
        r#"
        namespace Lib {
            struct Pair { Head : Int, Tail : Int }
            newtype Wrapper = (Value : Pair);
            function Make() : Wrapper {
                mutable n=0;
                Wrapper(new Pair { Tail={set n=8;n}, Head=n })
            }
            function Read(w : Wrapper) : Int { let p=w::Value; 100*p.Head+p.Tail }
            export Make, Read;
        }
        "#,
        r#"
        import Lib.*;
        @EntryPoint() operation Main() : Int { Read(Make()) }
        "#,
    );
}

#[test]
fn struct_erasure_preserves_first_initializer_failure() {
    let source = r#"
        struct Pair { Head : Int, Tail : Int }
        newtype Wrapper = (Value : Pair);
        function Fail(label : String) : Int { fail label }
        function Read(w : Wrapper) : Int { let p=w::Value; p.Head+p.Tail }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Pair { Tail=Fail("tail"), Head=Fail("head") }))
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the first field initializer should fail");
    assert!(error.contains("tail"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn struct_erasure_preserves_copy_failure_before_override_failure() {
    let source = r#"
        struct Only { N : Int }
        newtype Wrapper = (Value : Only);
        function Original() : Only { fail "copy" }
        function Override() : Int { fail "override" }
        function Read(w : Wrapper) : Int { (w::Value).N }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Only { ...Original(), N=Override() }))
        }
    "#;
    let error =
        crate::test_utils::eval_qsharp_original(source).expect_err("the copy source should fail");
    assert!(error.contains("copy"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn tuple_erased_newtype_preserves_data_only_parameter() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        newtype Wrapper = (Value : (Int, Int));
        function Read(w : Wrapper) : Int {
            let (a,b)=w::Value;
            100*a+b
        }
        @EntryPoint() operation Main() : Int { Read(Wrapper((4,8))) }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn nested_tuple_erased_newtypes_preserve_data() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        newtype Inner = (Value : (Int, Int));
        newtype Outer = (Inner : Inner);
        function Read(w : Outer) : Int {
            let (a,b)=w::Inner::Value;
            100*a+b
        }
        @EntryPoint() operation Main() : Int { Read(Outer(Inner((4,8)))) }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn tuple_erased_newtype_preserves_factory_effects_once() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        newtype Wrapper = (Value : (Int, Int));
        function Make() : Wrapper { Message("value"); Wrapper((4,8)) }
        @EntryPoint() operation Main() : Int {
            let (a,b)=Make()::Value;
            100*a+b
        }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn tuple_erased_newtype_preserves_inline_constructor_projection() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        newtype Wrapper = (Value : (Int, Int));
        @EntryPoint() operation Main() : Int {
            let (a,b)=Wrapper((4,8))::Value;
            100*a+b
        }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn tuple_erased_newtype_preserves_struct_payload() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        r#"
        struct Pair { Head : Int, Tail : Int }
        newtype Wrapper = (Value : Pair);
        function Read(w : Wrapper) : Int {
            let pair=w::Value;
            100*pair.Head+pair.Tail
        }
        @EntryPoint() operation Main() : Int {
            Read(Wrapper(new Pair { Head=4, Tail=8 }))
        }
        "#,
        qsc_eval::val::Value::Int(408),
    );
}

#[test]
fn tuple_erased_newtype_preserves_cross_package_data() {
    crate::test_utils::check_semantic_equivalence_with_library(
        r#"
        namespace Lib {
            newtype Wrapper = (Value : (Int, Int));
            function Read(w : Wrapper) : Int {
                let (a,b)=w::Value;
                100*a+b
            }
            export Wrapper, Read;
        }
        "#,
        r#"
        import Lib.*;
        @EntryPoint() operation Main() : Int { Read(Wrapper((4,8))) }
        "#,
    );
}

#[test]
fn udt_construction_and_field_access_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let p = new Pair { X = 5, Y = 3 };
                p.X - p.Y
            }
        }
    "#});
}

#[test]
fn udt_returned_from_function_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Wrapper { Value : Int }

            function MakeWrapper(v : Int) : Wrapper {
                new Wrapper { Value = v }
            }

            @EntryPoint()
            function Main() : Int {
                let w = MakeWrapper(42);
                w.Value
            }
        }
    "#});
}

#[test]
fn nested_udt_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Inner { A : Int, B : Int }
            struct Outer { First : Inner, Second : Int }

            @EntryPoint()
            function Main() : Int {
                let inner = new Inner { A = 10, B = 20 };
                let outer = new Outer { First = inner, Second = 30 };
                outer.First.A + outer.First.B + outer.Second
            }
        }
    "#});
}

#[test]
fn array_of_udt_preserves_semantics() {
    // UDT values stored in an array: erasure must recurse through the array
    // element type (`resolve_ty` array arm) so that element construction and
    // field access remain semantically equivalent after the struct is erased
    // to a tuple.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Point { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let points = [
                    new Point { X = 1, Y = 2 },
                    new Point { X = 3, Y = 4 },
                    new Point { X = 5, Y = 6 }
                ];
                points[0].X + points[1].Y + points[2].X
            }
        }
    "#});
}

#[test]
fn nested_udt_copy_update_preserves_semantics() {
    // A UDT field holding another UDT, updated via nested copy-update. Erasure
    // must recurse through the inner UDT type and preserve copy-update of the
    // nested field, so the original and erased programs agree.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Core { A : Int, B : Int }
            struct Outer { Inner : Core, Tag : Int }

            @EntryPoint()
            function Main() : Int {
                let outer = new Outer { Inner = new Core { A = 1, B = 2 }, Tag = 3 };
                let bumped = new Outer { ...outer, Inner = new Core { ...outer.Inner, B = 20 } };
                bumped.Inner.A + bumped.Inner.B + bumped.Tag
            }
        }
    "#});
}

#[test]
fn pretty_print_after_udt_erase_is_non_empty() {
    let source = indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }
        }
    "#};
    let (store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::UdtErase);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    // After UDT erasure the rendered Q# replaces struct construction with
    // tuple literals and uses `::Item<N>` field access. Verify non-empty.
    assert!(
        !rendered.is_empty(),
        "pretty-printed Q# after UDT erasure should not be empty"
    );
}

#[test]
fn struct_construction_and_copy_preserve_field_positions() {
    for field_count in 1..=4 {
        let fields = (0..field_count)
            .map(|field_index| format!("F{field_index} : Int"))
            .collect::<Vec<_>>()
            .join(", ");
        let assignments = (0..field_count)
            .map(|field_index| format!("F{field_index} = {field_index}"))
            .collect::<Vec<_>>()
            .join(", ");

        for use_copy_update in [false, true] {
            let (update, result_name) = if use_copy_update {
                let updated_field = field_count - 1;
                (
                    format!("let updated = new Generated {{ ...record, F{updated_field} = 99 }};"),
                    "updated",
                )
            } else {
                (String::new(), "record")
            };
            let result = (0..field_count)
                .map(|field_index| format!("{result_name}.F{field_index}"))
                .collect::<Vec<_>>()
                .join(", ");

            let source = formatdoc! {r#"
                namespace Test {{
                    struct Generated {{ {fields} }}

                    @EntryPoint()
                    function Main() : Int[] {{
                        let record = new Generated {{ {assignments} }};
                        {update}
                        [{result}]
                    }}
                }}
            "#};
            crate::test_utils::check_semantic_equivalence(&source);
        }
    }
}

#[test]
fn nested_struct_copy_preserves_callable_snapshot_before_mutating_source() {
    let source = indoc! {r#"
        namespace Test {
            struct Choice { Callable : Int -> Int, Weight : Int }
            struct Envelope { Inner : Choice, Tag : Int }
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function UseEnvelope(envelope : Envelope) : Int {
                envelope.Inner.Callable(2) * 1000 + envelope.Inner.Weight * 10 + envelope.Tag
            }
            @EntryPoint()
            operation Main() : Int {
                mutable original = new Envelope {
                    Inner = new Choice { Callable = Make(3), Weight = 7 }, Tag = 2
                };
                let copied = new Envelope {
                    ...original,
                    Inner = new Choice {
                        ...original.Inner,
                        Weight = {
                            set original = new Envelope {
                                Inner = new Choice { Callable = Make(17), Weight = 19 }, Tag = 5
                            };
                            11
                        }
                    }
                };
                UseEnvelope(copied) * 100000 + UseEnvelope(original)
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(511_219_195)));
    crate::test_utils::check_semantic_equivalence(source);
}
