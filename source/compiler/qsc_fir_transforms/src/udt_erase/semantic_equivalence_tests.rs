// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(feature = "slow-proptest-tests")]
use indoc::formatdoc;
use indoc::indoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;

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

#[cfg(feature = "slow-proptest-tests")]
fn udt_erasure_pattern() -> impl Strategy<Value = String> {
    (1..=4usize, prop::bool::ANY).prop_map(|(field_count, use_copy_update)| {
        let fields = (0..field_count)
            .map(|field_index| format!("F{field_index} : Int"))
            .collect::<Vec<_>>()
            .join(", ");
        let assignments = (0..field_count)
            .map(|field_index| format!("F{field_index} = {field_index}"))
            .collect::<Vec<_>>()
            .join(", ");

        if use_copy_update {
            let updated_field = field_count - 1;
            let result = (0..field_count)
                .map(|field_index| format!("updated.F{field_index}"))
                .collect::<Vec<_>>()
                .join(" + ");

            formatdoc! {r#"
                namespace Test {{
                    struct Generated {{ {fields} }}

                    @EntryPoint()
                    function Main() : Int {{
                        let record = new Generated {{ {assignments} }};
                        let updated = new Generated {{ ...record, F{updated_field} = 99 }};
                        {result}
                    }}
                }}
            "#}
        } else {
            let result = (0..field_count)
                .map(|field_index| format!("record.F{field_index}"))
                .collect::<Vec<_>>()
                .join(" + ");

            formatdoc! {r#"
                namespace Test {{
                    struct Generated {{ {fields} }}

                    @EntryPoint()
                    function Main() : Int {{
                        let record = new Generated {{ {assignments} }};
                        {result}
                    }}
                }}
            "#}
        }
    })
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn udt_erasure_preserves_semantics(source in udt_erasure_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
