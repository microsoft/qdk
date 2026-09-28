// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;

#[test]
fn struct_initializers_preserve_source_order_before_field_reordering() {
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
fn fir_value_preservation_copy_source_snapshot() {
    for body in [
        "mutable original = new Data { First = 13, Second = 2 }; let copied = new Data { ...original, First = { set original = new Data { First = 13, Second = 5 }; 6 } }; copied.Second * 100 + copied.First * 10 + original.Second",
        "mutable original = new Choice { Callable = Add11, Weight = 2 }; let copied = new Choice { ...original, Callable = { set original = new Choice { Callable = Add11, Weight = 5 }; Times3 } }; copied.Weight * 100 + copied.Callable(2) * 10 + original.Weight",
        "mutable original = new Choice { Callable = Add11, Weight = 2 }; let snapshot = original; let copied = new Choice { ...snapshot, Callable = { set original = new Choice { Callable = Add11, Weight = 5 }; Times3 } }; copied.Weight * 100 + copied.Callable(2) * 10 + original.Weight",
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                struct Data {{ First : Int, Second : Int }}
                struct Choice {{ Callable : Int -> Int, Weight : Int }}
                function Add11(value : Int) : Int {{ value + 11 }}
                function Times3(value : Int) : Int {{ value * 3 }}
                @EntryPoint()
                operation Main() : Int {{ {body} }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(265)));
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn fir_value_preservation_copy_nested_callable_source() {
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

#[test]
fn fir_value_preservation_copy_source_and_failure_order() {
    for (declaration, initial, replacements, access) in [
        (
            "struct Data { First : Int, Second : Int, Third : Int }",
            "new Data { First = 7, Second = 2, Third = 3 }",
            "First = replacement",
            "copied.First",
        ),
        (
            "struct Data { First : Int }",
            "new Data { First = 7 }",
            "First = replacement",
            "copied.First",
        ),
    ] {
        for (source_tail, replacement_tail, expected) in [
            (initial, "6", Ok(1206)),
            (
                "fail $\"source-{order}\"",
                "fail $\"replacement-{order}\"",
                Err("source-1"),
            ),
            (
                initial,
                "fail $\"replacement-{order}\"",
                Err("replacement-12"),
            ),
        ] {
            let replacement = format!("{{ set order = order * 10 + 2; {replacement_tail} }}");
            let fields = replacements.replace("replacement", &replacement);
            let source = formatdoc! {r#"
                namespace Test {{
                    {declaration}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable order = 0;
                        let copied = new Data {{
                            ...{{ set order = order * 10 + 1; {source_tail} }},
                            {fields}
                        }};
                        order * 100 + {access}
                    }}
                }}
            "#};
            let (store, package_id) = crate::test_utils::compile_to_fir(&source);
            let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
            match expected {
                Ok(value) => assert_eq!(result, Ok(qsc_eval::val::Value::Int(value))),
                Err(marker) => assert!(result.expect_err("original must fail").contains(marker)),
            }
            crate::test_utils::check_semantic_equivalence(&source);
        }
    }
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
